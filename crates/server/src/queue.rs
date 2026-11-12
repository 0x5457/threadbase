//! Thread-level input queue: persistence, lifecycle transitions and events.

use crate::error::ServerError;
use crate::events::EventStore;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use threadbase_core::state::{parse_queue_status, queue_status_str, QueueAction, QueueMachine};
use threadbase_shared::{
    now_ms, user_color, Actor, ApiError, QueueItemDto, QueueStatus, ThreadEvent, ThreadId,
    UserBrief,
};
use ulid::Ulid;

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

const SELECT: &str =
    "SELECT q.id, q.thread_id, q.author_id, q.text, q.status, q.position, q.superseded,
            q.created_at, u.username, u.display_name, u.avatar_url
     FROM queued_inputs q JOIN users u ON u.id = q.author_id";

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<QueueItemDto>, ServerError> {
    let rows = sqlx::query_as::<_, QueueRow>(&format!(
        "{SELECT} WHERE q.thread_id = ? AND q.status IN ('queued','sending','superseded')
         ORDER BY q.position ASC"
    ))
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_dto).collect())
}

pub struct NewQueuedInput<'a> {
    pub thread_id: &'a str,
    pub author_id: &'a str,
    pub text: &'a str,
    pub authors_json: Option<&'a str>,
    pub created_at: i64,
}

/// Append a queued input and return its display position.
pub async fn enqueue(pool: &SqlitePool, input: NewQueuedInput<'_>) -> Result<u32, ServerError> {
    let position: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM queued_inputs WHERE thread_id = ?",
    )
    .bind(input.thread_id)
    .fetch_one(pool)
    .await?;
    let status = QueueMachine::transition(QueueStatus::Queued, &QueueAction::Enqueue)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "INSERT INTO queued_inputs(id, thread_id, author_id, text, authors_json, status,
            position, created_at, updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(input.thread_id)
    .bind(input.author_id)
    .bind(input.text)
    .bind(input.authors_json)
    .bind(queue_status_str(status))
    .bind(position)
    .bind(input.created_at)
    .bind(input.created_at)
    .execute(pool)
    .await?;
    Ok(u32::try_from(position).unwrap_or(0))
}

pub async fn status_of(pool: &SqlitePool, item_id: &str) -> Result<QueueStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT status FROM queued_inputs WHERE id = ?")
        .bind(item_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("queue item not found"))?;
    parse_queue_status(&value)
        .ok_or_else(|| ServerError::internal(format!("unknown queue status: {value}")))
}

/// The next plain queued item for a thread, if any.
pub async fn head(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Option<(String, String, String, Option<String>)>, ServerError> {
    let row = sqlx::query_as(
        "SELECT id, author_id, text, authors_json FROM queued_inputs
         WHERE thread_id = ? AND status = 'queued' AND superseded = 0
         ORDER BY position ASC LIMIT 1",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn dispatch(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
) -> Result<QueueStatus, ServerError> {
    transition(
        pool,
        projection,
        thread_id,
        item_id,
        QueueAction::Dispatch,
        None,
    )
    .await
}

pub async fn complete(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
    turn_id: Option<&str>,
) -> Result<QueueStatus, ServerError> {
    transition(
        pool,
        projection,
        thread_id,
        item_id,
        QueueAction::PromptSucceeded,
        turn_id,
    )
    .await
}

pub async fn fail(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
) -> Result<QueueStatus, ServerError> {
    transition(
        pool,
        projection,
        thread_id,
        item_id,
        QueueAction::PromptFailed,
        None,
    )
    .await
}

pub async fn remove(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
) -> Result<QueueStatus, ServerError> {
    if status_of(pool, item_id).await? != QueueStatus::Queued {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    transition(
        pool,
        projection,
        thread_id,
        item_id,
        QueueAction::Delete,
        None,
    )
    .await
}

pub async fn retry(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
) -> Result<QueueStatus, ServerError> {
    let current = status_of(pool, item_id).await?;
    if QueueMachine::is_terminal(current) {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    let next = QueueMachine::transition(current, &QueueAction::Retry)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE queued_inputs SET status = ?, superseded = 0, updated_at = ?
         WHERE id = ? AND status IN ('queued','superseded')",
    )
    .bind(queue_status_str(next))
    .bind(now_ms())
    .bind(item_id)
    .execute(pool)
    .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

/// Mark every still-queued item superseded when a turn is cancelled or a new
/// prompt takes over.
pub async fn supersede(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    turn_id: Option<&str>,
) -> Result<u32, ServerError> {
    let superseded = QueueMachine::transition(QueueStatus::Queued, &QueueAction::Supersede)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let result = sqlx::query(
        "UPDATE queued_inputs SET status = ?, superseded = 1, turn_id = ?, updated_at = ?
         WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(queue_status_str(superseded))
    .bind(turn_id)
    .bind(now_ms())
    .bind(thread_id)
    .execute(pool)
    .await?;
    projection.publish(thread_id).await;
    Ok(u32::try_from(result.rows_affected()).unwrap_or(0))
}

async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
    action: QueueAction,
    turn_id: Option<&str>,
) -> Result<QueueStatus, ServerError> {
    let current = status_of(pool, item_id).await?;
    let next = QueueMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    let updated = sqlx::query(
        "UPDATE queued_inputs SET status = ?, turn_id = ?, updated_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(queue_status_str(next))
    .bind(turn_id)
    .bind(now_ms())
    .bind(item_id)
    .bind(queue_status_str(current))
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ServerError::internal("queue item changed concurrently"));
    }
    projection.publish(thread_id).await;
    Ok(next)
}

/// Append a `QueueUpdated` event so every subscriber sees the same snapshot.
pub async fn publish(
    pool: &SqlitePool,
    events: &EventStore,
    thread_id: &str,
) -> Result<(), ServerError> {
    let items = list_for_thread(pool, thread_id).await?;
    events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::system(),
            ThreadEvent::QueueUpdated { items },
            Default::default(),
        )
        .await
        .map_err(|error| ServerError::internal(error.to_string()))?;
    Ok(())
}

fn to_dto(row: QueueRow) -> QueueItemDto {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::events::test_support::seed_thread;

    fn projection(pool: &SqlitePool) -> Projection {
        let (tx, _) = tokio::sync::broadcast::channel(16);
        Projection::new(pool.clone(), tx)
    }

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
        .bind(now_ms())
        .bind(now_ms())
        .execute(&pool)
        .await
        .unwrap();
        supersede(&pool, &projection(&pool), thread.as_str(), Some("turn-1"))
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
            .bind(now_ms())
            .bind(now_ms())
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
