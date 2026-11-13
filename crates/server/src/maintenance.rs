//! Offline maintenance for the append-only event log.
//!
//! Every token chunk is stored as its own row. Reads already fold those chunks
//! into one block, but the rows keep accumulating: a single long turn can write
//! tens of thousands of `thinking_delta` events. `Compact` rewrites the stored
//! log with the same fold a cold page uses, so replays keep showing the same
//! transcript while the table (and its index) shrinks.
//!
//! Usage and plan events are the exception: reads only need the newest, but the
//! stored history is kept for reports, so those rows are never removed.

use crate::config::Config;
use crate::db;
use crate::events::EventStore;
use anyhow::Context;
use sqlx::{QueryBuilder, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use threadbase_shared::ThreadEvent;

/// What one `compact` run did, or would do with `--dry-run`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CompactReport {
    pub threads: u64,
    pub rows_before: u64,
    pub rows_removed: u64,
    pub rows_folded: u64,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub dry_run: bool,
    pub vacuumed: bool,
}

/// Fold every thread's log and optionally reclaim the freed pages.
///
/// Row rewrites are ordinary WAL transactions, so they are safe while the
/// server is running. `VACUUM` needs an exclusive lock and a 140 MB database
/// will block clients for a moment; callers only ask for it explicitly.
pub async fn compact(
    config: &Config,
    dry_run: bool,
    vacuum: bool,
) -> anyhow::Result<CompactReport> {
    let bytes_before = file_bytes(&config.db_path);
    let pool = db::connect(&config.db_path).await?;
    let thread_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM threads ORDER BY id")
        .fetch_all(&pool)
        .await?;
    let mut report = CompactReport {
        dry_run,
        bytes_before,
        ..Default::default()
    };
    let store = EventStore::new(pool.clone());
    for thread_id in &thread_ids {
        let task = compact_thread(&store, &pool, thread_id, dry_run)
            .await
            .with_context(|| format!("compact thread {thread_id}"))?;
        report.threads += 1;
        report.rows_before += task.rows_before;
        report.rows_removed += task.rows_removed;
        report.rows_folded += task.rows_folded;
        if task.rows_removed > 0 || task.rows_folded > 0 {
            println!(
                "{}thread {thread_id}: {} rows, {} removed, {} folded",
                if dry_run { "would compact " } else { "" },
                task.rows_before,
                task.rows_removed,
                task.rows_folded,
            );
        }
    }

    if !dry_run {
        if vacuum {
            match sqlx::query("VACUUM").execute(&pool).await {
                Ok(_) => report.vacuumed = true,
                Err(error) => println!(
                    "vacuum skipped: {error}\n\
                     stop the server and run `threadbase compact --vacuum` to reclaim the pages."
                ),
            }
        }
        // Fold the WAL back into the file and truncate it. This must run after
        // VACUUM: in WAL mode the rebuilt pages live in the WAL, so a size
        // check before the checkpoint still reports the old file.
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await;
    }
    drop(pool);
    report.bytes_after = file_bytes(&config.db_path);
    Ok(report)
}

#[derive(Debug, Default, Clone, Copy)]
struct ThreadCompact {
    rows_before: u64,
    rows_removed: u64,
    rows_folded: u64,
}

async fn compact_thread(
    store: &EventStore,
    pool: &SqlitePool,
    thread_id: &str,
    dry_run: bool,
) -> anyhow::Result<ThreadCompact> {
    let originals: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT seq, kind, payload_json FROM events WHERE thread_id = ? ORDER BY seq ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    let rows_before = u64::try_from(originals.len()).unwrap_or(u64::MAX);
    if originals.len() < 2 {
        return Ok(ThreadCompact {
            rows_before,
            ..Default::default()
        });
    }
    let stored: HashMap<i64, &str> = originals
        .iter()
        .map(|(seq, _, payload)| (*seq, payload.as_str()))
        .collect();

    let kept = store.compact_thread_log(thread_id).await?;
    let kept_seqs: HashSet<u64> = kept.iter().map(|event| event.seq).collect();
    // Only streamed text is rewritten; the kept tool/notice rows already match
    // the stored bytes.
    let mut folded: Vec<(i64, String)> = Vec::new();
    for event in &kept {
        let foldable = matches!(
            event.payload,
            ThreadEvent::MessageDelta { .. } | ThreadEvent::ThinkingDelta { .. }
        );
        if !foldable {
            continue;
        }
        let seq = i64::try_from(event.seq).unwrap_or(i64::MAX);
        let Some(original) = stored.get(&seq) else {
            continue;
        };
        let serialized = serde_json::to_string(&event.payload)?;
        if serialized.as_str() != *original {
            folded.push((seq, serialized));
        }
    }

    let mut removed: Vec<i64> = Vec::new();
    for (seq, kind, _) in &originals {
        let seq_u64 = u64::try_from(*seq).unwrap_or(u64::MAX);
        if kept_seqs.contains(&seq_u64) {
            continue;
        }
        // Reads fold these to the newest, but the history is worth keeping.
        if kind == "usage_updated" || kind == "plan_updated" {
            continue;
        }
        removed.push(*seq);
    }

    let rows_removed = u64::try_from(removed.len()).unwrap_or(u64::MAX);
    let rows_folded = u64::try_from(folded.len()).unwrap_or(u64::MAX);
    if !dry_run && (rows_removed > 0 || rows_folded > 0) {
        let mut tx = pool.begin().await?;
        for (seq, payload) in &folded {
            sqlx::query("UPDATE events SET payload_json = ? WHERE thread_id = ? AND seq = ?")
                .bind(payload)
                .bind(thread_id)
                .bind(seq)
                .execute(&mut *tx)
                .await?;
        }
        for chunk in removed.chunks(500) {
            let mut delete = QueryBuilder::new("DELETE FROM events WHERE thread_id = ");
            delete.push_bind(thread_id);
            delete.push(" AND seq IN (");
            let mut separated = delete.separated(", ");
            for seq in chunk {
                separated.push_bind(*seq);
            }
            separated.push_unseparated(")");
            delete.build().execute(&mut *tx).await?;
        }
        tx.commit().await?;
    }
    Ok(ThreadCompact {
        rows_before,
        rows_removed,
        rows_folded,
    })
}

fn file_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::test_support::seed_thread;
    use crate::events::{EventMeta, EventStore};
    use threadbase_shared::{
        now_ms, Actor, MessageDto, MessageId, MessageRole, ThreadEvent, ToolCallData, ToolKind,
        ToolStatus, Usage,
    };

    fn tool_call(status: ToolStatus) -> ToolCallData {
        ToolCallData {
            id: "call-1".into(),
            thread_id: "".into(),
            title: "shell".into(),
            kind: ToolKind::Execute,
            status,
            input: None,
            output: None,
            locations: Vec::new(),
            diff: None,
            terminal_id: None,
            subagent_id: None,
            created_at: now_ms(),
            updated_at: now_ms(),
        }
    }

    async fn count(pool: &SqlitePool, kind: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE kind = ?")
            .bind(kind)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn folds_streams_but_keeps_usage_history() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let store = EventStore::new(pool.clone());
        let thinking = MessageId::new();
        let stream = MessageId::new();
        let completed = MessageId::new();

        for chunk in ["Hel", "lo", " world"] {
            store
                .append(
                    &thread,
                    Actor::agent("Agent"),
                    ThreadEvent::ThinkingDelta {
                        message_id: thinking.clone(),
                        text: chunk.into(),
                    },
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        for (id, text) in [(&stream, "a"), (&stream, "b")] {
            store
                .append(
                    &thread,
                    Actor::agent("Agent"),
                    ThreadEvent::MessageDelta {
                        message_id: id.clone(),
                        role: MessageRole::Agent,
                        text: text.into(),
                        author: None,
                    },
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        for text in ["par", "tial"] {
            store
                .append(
                    &thread,
                    Actor::agent("Agent"),
                    ThreadEvent::MessageDelta {
                        message_id: completed.clone(),
                        role: MessageRole::Agent,
                        text: text.into(),
                        author: None,
                    },
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        store
            .append(
                &thread,
                Actor::agent("Agent"),
                ThreadEvent::MessageCompleted {
                    message: MessageDto {
                        id: completed.clone(),
                        thread_id: thread.clone(),
                        turn_id: None,
                        role: MessageRole::Agent,
                        author: None,
                        authors: Vec::new(),
                        text: "the final answer".into(),
                        model: None,
                        streaming: false,
                        created_at: now_ms(),
                        attachments: Vec::new(),
                    },
                },
                EventMeta::default(),
            )
            .await
            .unwrap();
        for status in [
            ToolStatus::Pending,
            ToolStatus::InProgress,
            ToolStatus::Completed,
        ] {
            let mut call = tool_call(status);
            call.thread_id = thread.clone();
            store
                .append(
                    &thread,
                    Actor::agent("Agent"),
                    ThreadEvent::ToolCallUpdate { call },
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        for total in [10, 20] {
            store
                .append(
                    &thread,
                    Actor::agent("Agent"),
                    ThreadEvent::UsageUpdated {
                        usage: Usage {
                            input_tokens: total,
                            output_tokens: 0,
                            total_tokens: total,
                            context_used: None,
                            context_size: None,
                        },
                    },
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        store
            .append(
                &thread,
                Actor::system(),
                ThreadEvent::SystemNotice {
                    level: "info".into(),
                    message: "note".into(),
                },
                EventMeta::default(),
            )
            .await
            .unwrap();

        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        let dry = compact_thread(&store, &pool, thread.as_str(), true)
            .await
            .unwrap();
        assert_eq!(dry.rows_before, u64::try_from(before).unwrap());
        assert_eq!(dry.rows_removed, 2 + 1 + 2 + 2);
        assert_eq!(dry.rows_folded, 2);
        let after_dry: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after_dry, before, "a dry run must not write");

        let done = compact_thread(&store, &pool, thread.as_str(), false)
            .await
            .unwrap();
        assert_eq!(done.rows_removed, dry.rows_removed);
        assert_eq!(done.rows_folded, dry.rows_folded);

        // Thinking chunks folded into one row carrying the whole text.
        let (thinking_rows, thinking_text): (i64, String) = sqlx::query_as(
            "SELECT COUNT(*), MAX(json_extract(payload_json, '$.text')) FROM events
              WHERE kind = 'thinking_delta'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(thinking_rows, 1);
        assert_eq!(thinking_text, "Hello world");
        // A still-streaming message keeps one folded delta; a completed one
        // needs none because the completion carries its text.
        let (streaming_rows, streaming_text): (i64, String) = sqlx::query_as(
            "SELECT COUNT(*), MAX(json_extract(payload_json, '$.text')) FROM events
              WHERE kind = 'message_delta'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(streaming_rows, 1);
        assert_eq!(streaming_text, "ab");
        assert_eq!(count(&pool, "message_completed").await, 1);
        assert_eq!(count(&pool, "tool_call_update").await, 1);
        // Usage is history, not a fold candidate.
        assert_eq!(count(&pool, "usage_updated").await, 2);
        assert_eq!(count(&pool, "system_notice").await, 1);

        // A second pass has nothing left to do.
        let again = compact_thread(&store, &pool, thread.as_str(), false)
            .await
            .unwrap();
        assert_eq!(again.rows_removed, 0);
        assert_eq!(again.rows_folded, 0);
    }
}
