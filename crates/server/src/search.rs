//! FTS5 index for thread titles, prompts, messages and comments.
//!
//! Events stay authoritative. The index is a derived read model, updated by
//! `EventStore::append` for conversation rows and by the thread/comment
//! writers for titles and deletions. A one-time backfill covers databases
//! created before the index existed.

use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_shared::{SearchHitDto, SearchResponse, ThreadEvent, ThreadId};

/// Replace one document in the index. FTS5 rows are deleted by their
/// thread/kind/ref coordinates, so re-indexing is idempotent.
pub async fn index_document(
    pool: &SqlitePool,
    thread_id: &str,
    kind: &str,
    ref_id: &str,
    title: &str,
    body: &str,
    ts: i64,
) -> Result<(), ServerError> {
    if body.trim().is_empty() && title.trim().is_empty() {
        return Ok(());
    }
    sqlx::query("DELETE FROM search_fts WHERE thread_id = ? AND kind = ? AND ref_id = ?")
        .bind(thread_id)
        .bind(kind)
        .bind(ref_id)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO search_fts(thread_id, kind, ref_id, title, body, ts)
         VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(thread_id)
    .bind(kind)
    .bind(ref_id)
    .bind(title)
    .bind(body)
    .bind(ts)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn index_thread(pool: &SqlitePool, thread_id: &str) -> Result<(), ServerError> {
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT title, created_at FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(pool)
            .await?;
    let Some((title, created_at)) = row else {
        return Ok(());
    };
    index_document(
        pool, thread_id, "thread", thread_id, &title, &title, created_at,
    )
    .await
}

pub async fn index_event(pool: &SqlitePool, thread_id: &str, event: &ThreadEvent, ts: i64) {
    let result = match event {
        ThreadEvent::TurnStarted {
            message_id,
            prompt,
            author,
            ..
        } => {
            index_document(
                pool,
                thread_id,
                "message",
                message_id.as_str(),
                &author.display_name,
                prompt,
                ts,
            )
            .await
        }
        ThreadEvent::MessageCompleted { message } => {
            let title = message
                .author
                .as_ref()
                .map(|author| author.display_name.as_str())
                .unwrap_or("Agent");
            index_document(
                pool,
                thread_id,
                "message",
                message.id.as_str(),
                title,
                &message.text,
                message.created_at,
            )
            .await
        }
        ThreadEvent::CommentAdded { comment } | ThreadEvent::CommentUpdated { comment } => {
            index_document(
                pool,
                thread_id,
                "comment",
                comment.id.as_str(),
                &comment.author.display_name,
                &comment.body,
                comment.updated_at,
            )
            .await
        }
        ThreadEvent::ThreadUpdated { title: Some(_), .. } => index_thread(pool, thread_id).await,
        _ => Ok(()),
    };
    if let Err(error) = result {
        tracing::warn!(thread_id, %error, "could not update the search index");
    }
}

pub async fn remove_thread(pool: &SqlitePool, thread_id: &str) -> Result<(), ServerError> {
    sqlx::query("DELETE FROM search_fts WHERE thread_id = ?")
        .bind(thread_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_comment(
    pool: &SqlitePool,
    thread_id: &str,
    comment_id: &str,
) -> Result<(), ServerError> {
    sqlx::query("DELETE FROM search_fts WHERE thread_id = ? AND kind = 'comment' AND ref_id = ?")
        .bind(thread_id)
        .bind(comment_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct SearchRow {
    thread_id: String,
    thread_title: String,
    project_id: String,
    project_name: String,
    kind: String,
    ref_id: String,
    snippet: String,
    ts: i64,
}

/// Sanitize user input into an FTS5 AND query of quoted terms. Quoting keeps
/// operators and punctuation from turning into syntax errors.
fn fts_query(raw: &str) -> Option<String> {
    let mut terms = Vec::new();
    for word in raw.split_whitespace() {
        // The term is quoted below, so only the quote character itself needs
        // escaping; operators and code punctuation are literal text.
        let cleaned: String = word.chars().filter(|ch| *ch != '"').collect();
        if cleaned.is_empty() {
            continue;
        }
        terms.push(format!("\"{cleaned}\""));
    }
    (!terms.is_empty()).then(|| terms.join(" "))
}

pub async fn search(
    pool: &SqlitePool,
    raw: &str,
    project_id: Option<&str>,
    limit: usize,
) -> Result<SearchResponse, ServerError> {
    let raw: String = raw.chars().take(200).collect();
    let Some(query) = fts_query(&raw) else {
        return Ok(SearchResponse { hits: Vec::new() });
    };
    let limit = limit.clamp(1, 50);
    let rows: Vec<SearchRow> = sqlx::query_as(
        "SELECT search_fts.thread_id, t.title AS thread_title, t.project_id,
                p.name AS project_name, search_fts.kind, search_fts.ref_id,
                snippet(search_fts, 4, '', '', '…', 16) AS snippet, search_fts.ts
         FROM search_fts
         JOIN threads t ON t.id = search_fts.thread_id
         JOIN projects p ON p.id = t.project_id
         WHERE search_fts MATCH ?
           AND (? = '' OR t.project_id = ?)
         ORDER BY bm25(search_fts), search_fts.ts DESC
         LIMIT ?",
    )
    .bind(query)
    .bind(project_id.unwrap_or_default())
    .bind(project_id.unwrap_or_default())
    .bind(i64::try_from(limit).unwrap_or(50))
    .fetch_all(pool)
    .await?;
    Ok(SearchResponse {
        hits: rows
            .into_iter()
            .map(|row| SearchHitDto {
                thread_id: ThreadId::from(row.thread_id),
                thread_title: row.thread_title,
                project_id: row.project_id.into(),
                project_name: row.project_name,
                kind: row.kind,
                ref_id: row.ref_id,
                snippet: row.snippet,
                ts: row.ts,
            })
            .collect(),
    })
}

/// Backfill once when the index is empty but threads exist.
pub async fn ensure_indexed(pool: &SqlitePool) -> Result<(), ServerError> {
    let indexed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM search_fts")
        .fetch_one(pool)
        .await?;
    let threads: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM threads")
        .fetch_one(pool)
        .await?;
    if indexed == 0 && threads > 0 {
        rebuild(pool).await?;
    }
    Ok(())
}

/// Rebuild the whole index from threads, events and comments.
pub async fn rebuild(pool: &SqlitePool) -> Result<(), ServerError> {
    sqlx::query("DELETE FROM search_fts").execute(pool).await?;
    let thread_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM threads")
        .fetch_all(pool)
        .await?;
    for thread_id in &thread_ids {
        reindex_thread(pool, thread_id).await?;
    }
    Ok(())
}

/// Rebuild one thread's documents. Used after a rewind deleted part of its
/// event log, where incremental removal would leave orphaned prompts behind.
pub async fn reindex_thread(pool: &SqlitePool, thread_id: &str) -> Result<(), ServerError> {
    sqlx::query("DELETE FROM search_fts WHERE thread_id = ?")
        .bind(thread_id)
        .execute(pool)
        .await?;
    index_thread(pool, thread_id).await?;
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT ts, payload_json FROM events
         WHERE thread_id = ? AND kind IN ('turn_started', 'message_completed')
         ORDER BY seq ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    for (ts, payload) in rows {
        if let Ok(event) = serde_json::from_str::<ThreadEvent>(&payload) {
            index_event(pool, thread_id, &event, ts).await;
        }
    }
    // Comments come from their live table, not events: a deleted comment has
    // no tombstone event and must not be resurrected by a rebuild.
    let comments: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT c.id, COALESCE(u.display_name, u.username), c.body, c.updated_at
         FROM comments c JOIN users u ON u.id = c.author_id
         WHERE c.thread_id = ?",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    for (comment_id, author, body, ts) in comments {
        index_document(pool, thread_id, "comment", &comment_id, &author, &body, ts).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use threadbase_shared::now_ms;
    use ulid::Ulid;

    async fn fixture() -> (SqlitePool, String, String) {
        let pool = db::connect_memory().await.unwrap();
        let user = Ulid::new().to_string();
        let project = Ulid::new().to_string();
        let thread = Ulid::new().to_string();
        sqlx::query("INSERT INTO users(id, username, display_name, created_at) VALUES(?, ?, ?, ?)")
            .bind(&user)
            .bind("ada")
            .bind("Ada")
            .bind(now_ms())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at)
             VALUES(?, 'repo', '/tmp/repo', 'main', ?, ?, ?)",
        )
        .bind(&project)
        .bind(&user)
        .bind(now_ms())
        .bind(now_ms())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch,
                created_by, created_at, updated_at)
             VALUES(?, ?, 'Histogram work', 'idle', 'codex', 'thread/test', 'main', ?, ?, ?)",
        )
        .bind(&thread)
        .bind(&project)
        .bind(&user)
        .bind(now_ms())
        .bind(now_ms())
        .execute(&pool)
        .await
        .unwrap();
        index_thread(&pool, &thread).await.unwrap();
        (pool, project, thread)
    }

    #[tokio::test]
    async fn indexes_and_searches_messages_with_project_filter() {
        let (pool, project, thread) = fixture().await;
        index_document(
            &pool,
            &thread,
            "message",
            "m1",
            "Ada",
            "Please rebuild the histogram aggregation",
            now_ms(),
        )
        .await
        .unwrap();
        let hits = search(&pool, "histogram", Some(&project), 20)
            .await
            .unwrap()
            .hits;
        assert_eq!(hits.len(), 2, "thread title and message both match");
        assert!(hits.iter().any(|hit| hit.kind == "message"));
        assert!(hits
            .iter()
            .all(|hit| hit.project_id.as_str() == project.as_str()));
        assert!(search(&pool, "missing", None, 20)
            .await
            .unwrap()
            .hits
            .is_empty());
    }

    #[tokio::test]
    async fn event_appends_update_the_index() {
        let (pool, _, thread) = fixture().await;
        let store = crate::events::EventStore::new(pool.clone());
        let author = threadbase_shared::UserBrief {
            id: "u1".into(),
            username: "ada".into(),
            display_name: "Ada".into(),
            avatar_url: None,
            color: "#a1c181".into(),
        };
        store
            .append(
                &ThreadId::from(thread.clone()),
                threadbase_shared::Actor::user(&author),
                ThreadEvent::TurnStarted {
                    turn_id: threadbase_shared::TurnId::new(),
                    message_id: threadbase_shared::MessageId::new(),
                    author: author.clone(),
                    authors: vec![author],
                    prompt: "find the needle in this haystack".into(),
                    attachments: Vec::new(),
                },
                crate::events::EventMeta {
                    delivery: Some(threadbase_shared::Delivery::Injected),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let hits = search(&pool, "needle", None, 20).await.unwrap().hits;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "message");
    }

    #[tokio::test]
    async fn reindexing_replaces_stale_documents() {
        let (pool, _, thread) = fixture().await;
        index_document(&pool, &thread, "message", "m1", "Ada", "old text", now_ms())
            .await
            .unwrap();
        index_document(&pool, &thread, "message", "m1", "Ada", "new text", now_ms())
            .await
            .unwrap();
        let hits = search(&pool, "old", None, 20).await.unwrap().hits;
        assert!(hits.is_empty());
        let hits = search(&pool, "new", None, 20).await.unwrap().hits;
        assert_eq!(hits.len(), 1);
        remove_thread(&pool, &thread).await.unwrap();
        assert!(search(&pool, "new", None, 20)
            .await
            .unwrap()
            .hits
            .is_empty());
    }
}
