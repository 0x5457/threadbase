//! Thread-level queued input listing.

use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::parse_queue_status;
use threadbase_shared::{user_color, QueueItemDto, QueueStatus, UserBrief};

#[derive(Debug, FromRow)]
struct QueueRow {
    id: String,
    thread_id: String,
    author_id: String,
    text: String,
    status: String,
    position: i64,
    superseded: i64,
    created_at: i64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<QueueItemDto>, ServerError> {
    let rows = sqlx::query_as::<_, QueueRow>(
        "SELECT q.id, q.thread_id, q.author_id, q.text, q.status, q.position, q.superseded,
                q.created_at, u.username, u.display_name, u.avatar_url
         FROM queued_inputs q JOIN users u ON u.id = q.author_id
         WHERE q.thread_id = ? AND q.status IN ('queued','sending','superseded')
         ORDER BY q.position ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(dto).collect())
}

fn dto(row: QueueRow) -> QueueItemDto {
    QueueItemDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        author: UserBrief {
            id: row.author_id.clone().into(),
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            color: user_color(&row.author_id).to_owned(),
        },
        text: row.text,
        status: parse_queue_status(&row.status).unwrap_or(QueueStatus::Queued),
        position: u32::try_from(row.position).unwrap_or(0),
        created_at: row.created_at,
        superseded: row.superseded != 0,
    }
}

use crate::error::ServerError as QueueError;
use crate::events::EventStore;
use threadbase_shared::{Actor, ThreadEvent, ThreadId};

pub async fn mark_superseded(
    state_store: &crate::state::StateStore,
    thread_id: &str,
    turn_id: Option<&str>,
) -> Result<(), QueueError> {
    state_store
        .supersede_queue(thread_id, turn_id)
        .await
        .map_err(|error| QueueError::internal(error.to_string()))?;
    Ok(())
}

pub async fn publish(
    pool: &SqlitePool,
    events: &EventStore,
    thread_id: &str,
) -> Result<(), QueueError> {
    let items = list_for_thread(pool, thread_id).await?;
    events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::system(),
            ThreadEvent::QueueUpdated { items },
            Default::default(),
        )
        .await
        .map_err(|error| QueueError::internal(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::events::test_support::seed_thread;

    #[tokio::test]
    async fn cancelling_marks_queued_inputs_superseded() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        sqlx::query(
            "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position, created_at, updated_at)
             VALUES('q1', ?, ?, 'queued', 'queued', 1, ?, ?)",
        )
        .bind(thread.as_str())
        .bind(user.as_str())
        .bind(threadbase_shared::now_ms())
        .bind(threadbase_shared::now_ms())
        .execute(&pool)
        .await
        .unwrap();
        let (state_tx, _) = tokio::sync::broadcast::channel(1);
        let state_store = crate::state::StateStore::new(pool.clone(), state_tx);
        mark_superseded(&state_store, thread.as_str(), Some("turn-1"))
            .await
            .unwrap();
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].superseded);
    }

    #[tokio::test]
    async fn queued_inputs_survive_and_keep_order() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        for (position, text) in [(1, "first"), (2, "second")] {
            sqlx::query(
                "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position, created_at, updated_at)
                 VALUES(?, ?, ?, ?, 'queued', ?, ?, ?)",
            )
            .bind(ulid::Ulid::new().to_string())
            .bind(thread.as_str())
            .bind(user.as_str())
            .bind(text)
            .bind(position)
            .bind(threadbase_shared::now_ms())
            .bind(threadbase_shared::now_ms())
            .execute(&pool)
            .await
            .unwrap();
        }
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "first");
        assert_eq!(items[1].text, "second");
        assert!(items.iter().all(|item| item.status == QueueStatus::Queued));
    }
}
