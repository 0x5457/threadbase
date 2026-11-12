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
        "{SELECT} WHERE q.thread_id = ? AND q.status IN (?, ?, ?)
         ORDER BY q.position ASC"
    ))
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
    .bind(queue_status_str(QueueStatus::Sending))
    .bind(queue_status_str(QueueStatus::Superseded))
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
    let status = queue_status_str(QueueStatus::Queued);
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
    .bind(status)
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
         WHERE thread_id = ? AND status = ? AND superseded = 0
         ORDER BY position ASC LIMIT 1",
    )
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
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
    let current = status_of(pool, item_id).await?;
    if current != QueueStatus::Queued {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    transition_from(
        pool,
        projection,
        thread_id,
        item_id,
        current,
        &QueueAction::Delete,
        None,
    )
    .await
}

/// Mark every still-queued item superseded when a turn is cancelled or a new
/// prompt takes over.
pub async fn supersede(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    turn_id: Option<&str>,
) -> Result<u32, ServerError> {
    let superseded = queue_status_str(QueueStatus::Superseded);
    let result = sqlx::query(
        "UPDATE queued_inputs SET status = ?, superseded = 1, turn_id = ?, updated_at = ?
         WHERE thread_id = ? AND status = ?",
    )
    .bind(superseded)
    .bind(turn_id)
    .bind(now_ms())
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
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
    transition_from(
        pool, projection, thread_id, item_id, current, &action, turn_id,
    )
    .await
}

async fn transition_from(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
    current: QueueStatus,
    action: &QueueAction,
    turn_id: Option<&str>,
) -> Result<QueueStatus, ServerError> {
    let next = QueueMachine::transition(current, action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
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

    async fn seed_item(
        pool: &SqlitePool,
        thread_id: &str,
        author_id: &str,
        id: &str,
        status: QueueStatus,
        position: i64,
    ) {
        sqlx::query(
            "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position, created_at, updated_at)
             VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(thread_id)
        .bind(author_id)
        .bind(format!("text {id}"))
        .bind(queue_status_str(status))
        .bind(position)
        .bind(now_ms())
        .bind(now_ms())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cancelling_marks_queued_inputs_superseded() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q1",
            QueueStatus::Queued,
            1,
        )
        .await;
        supersede(&pool, &projection(&pool), thread.as_str(), Some("turn-1"))
            .await
            .unwrap();
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].superseded);
        assert_eq!(items[0].status, QueueStatus::Superseded);
    }

    #[tokio::test]
    async fn enqueue_assigns_positions_and_keeps_order() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        for (expected, text) in [(1, "first"), (2, "second")] {
            let position = enqueue(
                &pool,
                NewQueuedInput {
                    thread_id: thread.as_str(),
                    author_id: user.as_str(),
                    text,
                    authors_json: None,
                    created_at: now_ms(),
                },
            )
            .await
            .unwrap();
            assert_eq!(position, expected);
        }
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "first");
        assert_eq!(items[1].text, "second");
        assert!(items.iter().all(|item| item.status == QueueStatus::Queued));
    }

    #[tokio::test]
    async fn head_returns_the_next_queued_item() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q1",
            QueueStatus::Queued,
            1,
        )
        .await;
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q2",
            QueueStatus::Queued,
            2,
        )
        .await;
        let head_item = head(&pool, thread.as_str()).await.unwrap().unwrap();
        assert_eq!(head_item.0, "q1");
        dispatch(&pool, &projection(&pool), thread.as_str(), "q1")
            .await
            .unwrap();
        let next = head(&pool, thread.as_str()).await.unwrap().unwrap();
        assert_eq!(next.0, "q2");
    }

    #[tokio::test]
    async fn dispatch_then_complete_marks_sent() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q1",
            QueueStatus::Queued,
            1,
        )
        .await;
        let projection = projection(&pool);
        assert_eq!(
            dispatch(&pool, &projection, thread.as_str(), "q1")
                .await
                .unwrap(),
            QueueStatus::Sending
        );
        assert_eq!(
            complete(&pool, &projection, thread.as_str(), "q1", Some("turn-1"))
                .await
                .unwrap(),
            QueueStatus::Sent
        );
        assert_eq!(status_of(&pool, "q1").await.unwrap(), QueueStatus::Sent);
        let turn_id: Option<String> =
            sqlx::query_scalar("SELECT turn_id FROM queued_inputs WHERE id = 'q1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(turn_id.as_deref(), Some("turn-1"));
    }

    #[tokio::test]
    async fn dispatch_then_fail_marks_dropped() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q1",
            QueueStatus::Queued,
            1,
        )
        .await;
        let projection = projection(&pool);
        dispatch(&pool, &projection, thread.as_str(), "q1")
            .await
            .unwrap();
        assert_eq!(
            fail(&pool, &projection, thread.as_str(), "q1")
                .await
                .unwrap(),
            QueueStatus::Dropped
        );
        assert_eq!(status_of(&pool, "q1").await.unwrap(), QueueStatus::Dropped);
    }

    #[tokio::test]
    async fn remove_rejects_items_that_already_left_the_queue() {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q1",
            QueueStatus::Sending,
            1,
        )
        .await;
        let error = remove(&pool, &projection(&pool), thread.as_str(), "q1")
            .await
            .unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::NotFound);
        assert_eq!(status_of(&pool, "q1").await.unwrap(), QueueStatus::Sending);
    }
}
