//! Thread worktree history: the checkpoint commits a thread accumulates.

use crate::error::ServerError;
use crate::threads::ThreadRow;
use sqlx::SqlitePool;
use threadbase_shared::{CommitSummaryDto, ThreadHistoryDto};
use threadbase_workspace::git;

use super::worktree;

/// Commits newer than the thread's base, newest first. Each snapshot is
/// written immediately *before* a turn starts, so the work it contains belongs
/// to the turn that started before it; that is the prompt attached here.
pub async fn history(pool: &SqlitePool, row: &ThreadRow) -> Result<ThreadHistoryDto, ServerError> {
    let head = match worktree(row) {
        Ok(path) => git::head_sha(&path).await.ok(),
        Err(_) => None,
    };
    let Some(base) = row.base_sha.clone() else {
        return Ok(ThreadHistoryDto {
            base_branch: row.base_branch.clone(),
            base_sha: None,
            head,
            entries: Vec::new(),
        });
    };
    let Some(path) = row.worktree_path.as_deref().map(std::path::PathBuf::from) else {
        return Ok(ThreadHistoryDto {
            base_branch: row.base_branch.clone(),
            base_sha: Some(base),
            head,
            entries: Vec::new(),
        });
    };
    if head.is_none() {
        return Ok(ThreadHistoryDto {
            base_branch: row.base_branch.clone(),
            base_sha: Some(base),
            head,
            entries: Vec::new(),
        });
    }
    let range = format!("{base}..HEAD");
    // A missing or rewritten base (for example after Apply changes) must not
    // turn the History panel into an error; no rows is the honest answer.
    let commits = git::log(&path, &range, 100).await.unwrap_or_default();
    // The newest 500 turns are enough to label the newest 100 commits; older
    // prompts are dropped instead of shipping whole transcripts on a poll.
    let turns: Vec<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ts, turn_id, prompt FROM (
             SELECT ts, seq,
                    json_extract(payload_json, '$.turnId') AS turn_id,
                    substr(json_extract(payload_json, '$.prompt'), 1, 512) AS prompt
             FROM events
             WHERE thread_id = ? AND kind = 'turn_started'
             ORDER BY ts DESC, seq DESC
             LIMIT 500
         ) ORDER BY ts ASC, seq ASC",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut entries = Vec::with_capacity(commits.len());
    // Commits arrive newest first and turns oldest first. Walk the turn list
    // backwards: for each commit, the last turn that had started by its
    // authoring time is the turn whose work it captured.
    let mut cursor = turns.len();
    for commit in commits {
        let at = commit.at.saturating_mul(1000);
        while cursor > 0 && turns[cursor - 1].0 > at {
            cursor -= 1;
        }
        let (turn_id, prompt) = match cursor.checked_sub(1).and_then(|index| turns.get(index)) {
            Some((_, turn_id, prompt)) => (turn_id.clone(), preview(prompt.as_deref())),
            None => (None, None),
        };
        entries.push(CommitSummaryDto {
            sha: commit.sha,
            short_sha: commit.short_sha,
            at,
            author: commit.author,
            checkpoint: commit.subject == git::TURN_SNAPSHOT_MESSAGE,
            subject: commit.subject,
            additions: commit.additions,
            deletions: commit.deletions,
            files: commit.files,
            turn_id,
            turn_prompt: prompt,
        });
    }

    Ok(ThreadHistoryDto {
        base_branch: row.base_branch.clone(),
        base_sha: Some(base),
        head,
        entries,
    })
}

/// One quiet line for the history row. Prompts can be a whole paragraph;
/// keeping the first 160 characters is enough to identify the turn and keeps
/// the response small when a thread has dozens of checkpoints.
fn preview(prompt: Option<&str>) -> Option<String> {
    let prompt = prompt?;
    let single_line = prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned();
    if single_line.is_empty() {
        return None;
    }
    let mut out: String = single_line.chars().take(160).collect();
    if single_line.chars().count() > 160 {
        out.push('…');
    }
    Some(out)
}
