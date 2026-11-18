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
    // Checkpoints are the exact markers of "a turn started here", oldest first.
    let markers: Vec<(String, String)> = sqlx::query_as(
        "SELECT sha, turn_id FROM checkpoints WHERE thread_id = ? ORDER BY created_at ASC, rowid ASC",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    // Prompts label the rows; the newest 500 turns cover the newest 100 commits.
    let prompts: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT json_extract(payload_json, '$.turnId'),
                substr(json_extract(payload_json, '$.prompt'), 1, 512)
           FROM events WHERE thread_id = ? AND kind = 'turn_started'
          ORDER BY seq DESC LIMIT 500",
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let prompts: std::collections::HashMap<String, String> = prompts
        .into_iter()
        .filter_map(|(turn_id, prompt)| Some((turn_id?, prompt?)))
        .collect();

    // Walk the commits newest first. Work above the newest checkpoint belongs to
    // the newest turn; the checkpoint commit itself holds the work of the turn
    // before it, because it was written immediately before that turn started.
    // `marker` counts the checkpoints still below the walk, so `markers[marker-1]`
    // is always the owner of the commit being looked at.
    let mut marker = markers.len();
    let mut entries = Vec::with_capacity(commits.len());
    for commit in commits {
        if marker > 0 && markers[marker - 1].0 == commit.sha {
            marker -= 1;
        }
        let authored_by = (marker > 0).then(|| markers[marker - 1].1.clone());
        let at = commit.at.saturating_mul(1000);
        let turn_prompt = authored_by
            .as_deref()
            .and_then(|turn_id| prompts.get(turn_id))
            .and_then(|prompt| preview(Some(prompt)));
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
            turn_id: authored_by,
            turn_prompt,
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
