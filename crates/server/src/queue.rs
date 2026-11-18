//! Thread-level input queue: persistence, lifecycle transitions and events.

use crate::error::ServerError;
use crate::events::EventStore;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;
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
    authors_json: Option<String>,
    created_at: i64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}

const SELECT: &str = "SELECT q.id, q.thread_id, q.author_id, q.text, q.status, q.position,
            q.authors_json, q.created_at, u.username, u.display_name, u.avatar_url
     FROM queued_inputs q JOIN users u ON u.id = q.author_id";

/// The synthetic author of the continuation prompt a restart queues. Its row
/// exists so the work is durable, not to be read or acted on by a person.
pub(crate) fn restart_author(authors_json: Option<&str>) -> Option<UserBrief> {
    let authors: Vec<UserBrief> = serde_json::from_str(authors_json?).ok()?;
    match authors.as_slice() {
        [author] if author.id.as_str() == "threadbase" => Some(author.clone()),
        _ => None,
    }
}

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<QueueItemDto>, ServerError> {
    let rows = sqlx::query_as::<_, QueueRow>(&format!(
        "{SELECT} WHERE q.thread_id = ? AND q.status IN (?, ?) AND q.starting = 0
         ORDER BY q.position ASC"
    ))
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
    .bind(queue_status_str(QueueStatus::Superseded))
    .fetch_all(pool)
    .await?;
    // `sending` rows are already in the transcript: dispatch happens before the
    // turn records its prompt, so keeping the row in the bar would show the same
    // message twice until the adapter answers.
    // The restart continuation is internal plumbing: showing it in the queue
    // bar invites a user to steer or drop a prompt that is already being sent.
    Ok(rows
        .into_iter()
        .filter(|row| restart_author(row.authors_json.as_deref()).is_none())
        .map(to_dto)
        .collect())
}

pub struct NewQueuedInput<'a> {
    pub thread_id: &'a str,
    pub author_id: &'a str,
    pub text: &'a str,
    pub authors_json: Option<&'a str>,
    pub attachments_json: Option<&'a str>,
    /// This message is starting a cold thread; it is not waiting behind
    /// anything, so it stays out of the queue bar until the worker takes it.
    pub starting: bool,
    pub created_at: i64,
}

/// Append a queued input and return its id and display position.
pub async fn enqueue(
    pool: &SqlitePool,
    input: NewQueuedInput<'_>,
) -> Result<(String, u32), ServerError> {
    let position: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM queued_inputs WHERE thread_id = ?",
    )
    .bind(input.thread_id)
    .fetch_one(pool)
    .await?;
    let id = Ulid::new().to_string();
    let status = queue_status_str(QueueStatus::Queued);
    sqlx::query(
        "INSERT INTO queued_inputs(id, thread_id, author_id, text, authors_json, attachments_json,
            status, starting, position, created_at, updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(input.thread_id)
    .bind(input.author_id)
    .bind(input.text)
    .bind(input.authors_json)
    .bind(input.attachments_json)
    .bind(status)
    .bind(input.starting)
    .bind(position)
    .bind(input.created_at)
    .bind(input.created_at)
    .execute(pool)
    .await?;
    Ok((id, u32::try_from(position).unwrap_or(0)))
}

/// Whether anything is still waiting to be delivered. A hidden cold-start row
/// counts: the adapter has not taken it yet.
pub async fn pending(pool: &SqlitePool, thread_id: &str) -> Result<bool, ServerError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM queued_inputs
          WHERE thread_id = ? AND status IN ('queued', 'sending'))",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Put a cold-start message back in the bar: the adapter never took it, so it
/// is waiting like any other input.
pub async fn reveal(
    pool: &SqlitePool,
    projection: &Projection,
    events: &EventStore,
    thread_id: &str,
    item_id: &str,
) -> Result<(), ServerError> {
    let updated = sqlx::query(
        "UPDATE queued_inputs SET starting = 0, updated_at = ?
          WHERE id = ? AND thread_id = ? AND starting = 1",
    )
    .bind(now_ms())
    .bind(item_id)
    .bind(thread_id)
    .execute(pool)
    .await?;
    if updated.rows_affected() > 0 {
        projection.publish(thread_id).await;
        publish(pool, events, thread_id).await?;
    }
    Ok(())
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
) -> Result<Option<(String, String, String, Option<String>, Option<String>)>, ServerError> {
    let row = sqlx::query_as(
        "SELECT id, author_id, text, authors_json, attachments_json FROM queued_inputs
         WHERE thread_id = ? AND status = ?
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

/// Undo a drop: put a dropped queued message back in its original slot.
pub async fn restore(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
) -> Result<QueueStatus, ServerError> {
    let current = status_of(pool, item_id).await?;
    if current != QueueStatus::Dropped {
        return Err(ServerError(ApiError::not_found(
            "dropped message not found",
        )));
    }
    transition_from(
        pool,
        projection,
        thread_id,
        item_id,
        current,
        &QueueAction::Restore,
        None,
    )
    .await
}

/// One queued input plus the fields every delivery path needs.
pub struct QueuedInput {
    pub author_id: String,
    pub text: String,
    pub authors_json: Option<String>,
    pub attachments_json: Option<String>,
    pub status: QueueStatus,
}

#[derive(Debug, FromRow)]
struct ItemRow {
    author_id: String,
    text: String,
    authors_json: Option<String>,
    attachments_json: Option<String>,
    status: String,
}

/// Load one item, scoped to its thread so a mismatched id reads as missing.
pub async fn item(
    pool: &SqlitePool,
    thread_id: &str,
    item_id: &str,
) -> Result<QueuedInput, ServerError> {
    let row: Option<ItemRow> = sqlx::query_as(
        "SELECT author_id, text, authors_json, attachments_json, status FROM queued_inputs
         WHERE id = ? AND thread_id = ?",
    )
    .bind(item_id)
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(ServerError(ApiError::not_found("queued message not found")));
    };
    Ok(QueuedInput {
        author_id: row.author_id,
        text: row.text,
        authors_json: row.authors_json,
        attachments_json: row.attachments_json,
        status: parse_queue_status(&row.status).unwrap_or(QueueStatus::Queued),
    })
}

/// Rewrite a message that is still waiting; sent or dropped rows are immutable.
pub async fn update_text(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_id: &str,
    text: &str,
) -> Result<(), ServerError> {
    let updated = sqlx::query(
        "UPDATE queued_inputs SET text = ?, updated_at = ?
         WHERE id = ? AND thread_id = ? AND status = ?",
    )
    .bind(text)
    .bind(now_ms())
    .bind(item_id)
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found(
            "queued message not found or already sent",
        )));
    }
    projection.publish(thread_id).await;
    Ok(())
}

/// Hand a claimed item back to the queue: a steering attempt failed, so the
/// message keeps waiting for the next turn instead of being dropped.
pub async fn release(
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
        QueueAction::Release,
        None,
    )
    .await
}

/// Reorder the queued inputs. Sending/superseded rows keep their slots; the
/// queued rows are re-slotted in the requested order. Unknown or repeated ids
/// are ignored, so a stale drag cannot corrupt another thread's queue.
pub async fn reorder(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    item_ids: &[String],
) -> Result<(), ServerError> {
    let all: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM queued_inputs WHERE thread_id = ? AND status IN (?, ?, ?)
         ORDER BY position ASC, created_at ASC",
    )
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
    .bind(queue_status_str(QueueStatus::Sending))
    .bind(queue_status_str(QueueStatus::Superseded))
    .fetch_all(pool)
    .await?;
    let queued: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM queued_inputs WHERE thread_id = ? AND status = ?
         ORDER BY position ASC, created_at ASC",
    )
    .bind(thread_id)
    .bind(queue_status_str(QueueStatus::Queued))
    .fetch_all(pool)
    .await?;

    use std::collections::HashSet;
    let queued_set: HashSet<&str> = queued.iter().map(String::as_str).collect();
    let mut requested: Vec<String> = Vec::new();
    for id in item_ids {
        if queued_set.contains(id.as_str()) && !requested.iter().any(|x| x == id) {
            requested.push(id.clone());
        }
    }
    for id in &queued {
        if !requested.contains(id) {
            requested.push(id.clone());
        }
    }

    // Replace only the queued slots in the full ordered list, preserving the
    // positions of in-flight and superseded rows.
    let mut next = requested.into_iter();
    let mut order: Vec<String> = Vec::with_capacity(all.len());
    for id in &all {
        if queued_set.contains(id.as_str()) {
            order.push(next.next().unwrap_or_else(|| id.clone()));
        } else {
            order.push(id.clone());
        }
    }

    let mut tx = pool.begin().await?;
    let now = now_ms();
    for (index, id) in order.iter().enumerate() {
        sqlx::query(
            "UPDATE queued_inputs SET position = ?, updated_at = ?
             WHERE id = ? AND thread_id = ?",
        )
        .bind(i64::try_from(index).unwrap_or(0) + 1)
        .bind(now)
        .bind(id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    projection.publish(thread_id).await;
    Ok(())
}

/// Recover rows left `sending` when the previous process died mid-dispatch.
///
/// A row on a thread whose turn was cut off was already delivered (or is
/// covered by the continuation prompt), so it becomes `sent`; replaying it
/// would repeat side effects. Any other row never reached a turn, so it goes
/// back to `queued` and the worker runs it.
pub async fn recover_sending(
    pool: &SqlitePool,
    projection: &Projection,
    interrupted: &HashSet<String>,
) -> Result<(), ServerError> {
    // The task that hid a cold-start message died with the process; nothing is
    // starting anymore, so every row is honestly waiting.
    sqlx::query("UPDATE queued_inputs SET starting = 0 WHERE starting = 1")
        .execute(pool)
        .await?;
    let threads: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT thread_id FROM queued_inputs WHERE status = ?")
            .bind(queue_status_str(QueueStatus::Sending))
            .fetch_all(pool)
            .await?;
    let now = now_ms();
    for thread_id in threads {
        let status = if interrupted.contains(&thread_id) {
            QueueStatus::Sent
        } else {
            QueueStatus::Queued
        };
        sqlx::query(
            "UPDATE queued_inputs SET status = ?, updated_at = ?
              WHERE thread_id = ? AND status = ?",
        )
        .bind(queue_status_str(status))
        .bind(now)
        .bind(&thread_id)
        .bind(queue_status_str(QueueStatus::Sending))
        .execute(pool)
        .await?;
        projection.publish(&thread_id).await;
    }
    Ok(())
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
        "UPDATE queued_inputs SET status = ?, turn_id = ?, updated_at = ?
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
        "UPDATE queued_inputs SET status = ?, turn_id = ?, starting = 0, updated_at = ?
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

    /// A fresh database with one user and one thread.
    async fn seeded() -> (SqlitePool, String, String) {
        let pool = db::connect_memory().await.unwrap();
        let (user, _, thread) = seed_thread(&pool).await;
        (pool, user.to_string(), thread.to_string())
    }

    /// One queued row, which is what most of these tests need.
    async fn queued(pool: &SqlitePool, thread: &str, user: &str, id: &str, position: i64) {
        seed_item(pool, thread, user, id, QueueStatus::Queued, position).await;
    }

    #[tokio::test]
    async fn cancelling_marks_queued_inputs_superseded() {
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
        supersede(&pool, &projection(&pool), thread.as_str(), Some("turn-1"))
            .await
            .unwrap();
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, QueueStatus::Superseded);
    }

    #[tokio::test]
    async fn enqueue_assigns_positions_and_keeps_order() {
        let (pool, user, thread) = seeded().await;
        for (expected, text) in [(1, "first"), (2, "second")] {
            let (_, position) = enqueue(
                &pool,
                NewQueuedInput {
                    thread_id: thread.as_str(),
                    author_id: user.as_str(),
                    text,
                    authors_json: None,
                    attachments_json: None,
                    starting: false,
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
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
        queued(&pool, &thread, &user, "q2", 2).await;
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
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
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
    async fn claimed_items_leave_the_visible_queue() {
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
        let projection = projection(&pool);
        assert_eq!(
            list_for_thread(&pool, thread.as_str()).await.unwrap().len(),
            1
        );
        dispatch(&pool, &projection, thread.as_str(), "q1")
            .await
            .unwrap();
        // The claimed prompt is written into the transcript next; keeping the
        // row visible until the adapter answers shows the message twice.
        assert!(list_for_thread(&pool, thread.as_str())
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_starting_message_is_hidden_until_the_start_fails() {
        let (pool, user, thread) = seeded().await;
        let (id, position) = enqueue(
            &pool,
            NewQueuedInput {
                thread_id: thread.as_str(),
                author_id: user.as_str(),
                text: "starting the thread",
                authors_json: None,
                attachments_json: None,
                starting: true,
                created_at: now_ms(),
            },
        )
        .await
        .unwrap();
        assert_eq!(position, 1);
        // It is the message starting the thread, not something waiting behind it.
        assert!(list_for_thread(&pool, thread.as_str())
            .await
            .unwrap()
            .is_empty());
        assert!(pending(&pool, thread.as_str()).await.unwrap());

        // A failed adapter start returns it to the queue, where it can be seen.
        reveal(
            &pool,
            &projection(&pool),
            &EventStore::new(pool.clone()),
            thread.as_str(),
            &id,
        )
        .await
        .unwrap();
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.0, id);
    }

    #[tokio::test]
    async fn dispatch_then_fail_marks_dropped() {
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
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
        let (pool, user, thread) = seeded().await;
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

    #[tokio::test]
    async fn update_text_rewrites_only_waiting_items() {
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
        let projection = projection(&pool);
        update_text(&pool, &projection, thread.as_str(), "q1", "edited")
            .await
            .unwrap();
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items[0].text, "edited");

        dispatch(&pool, &projection, thread.as_str(), "q1")
            .await
            .unwrap();
        let error = update_text(&pool, &projection, thread.as_str(), "q1", "late")
            .await
            .unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn release_returns_a_claimed_item_to_the_queue() {
        let (pool, user, thread) = seeded().await;
        queued(&pool, &thread, &user, "q1", 1).await;
        let projection = projection(&pool);
        dispatch(&pool, &projection, thread.as_str(), "q1")
            .await
            .unwrap();
        assert_eq!(
            release(&pool, &projection, thread.as_str(), "q1")
                .await
                .unwrap(),
            QueueStatus::Queued
        );
        let head_item = head(&pool, thread.as_str()).await.unwrap().unwrap();
        assert_eq!(head_item.0, "q1");
    }

    #[tokio::test]
    async fn restart_recovers_rows_left_sending() {
        let pool = db::connect_memory().await.unwrap();
        let (user, project, thread) = seed_thread(&pool).await;
        let other = ThreadId::new();
        let now = now_ms();
        sqlx::query(
            "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch,
                created_by, created_at, updated_at)
             VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(other.as_str())
        .bind(project.as_str())
        .bind("Other thread")
        .bind("idle")
        .bind("codex")
        .bind("thread/other")
        .bind("main")
        .bind(user.as_str())
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        seed_item(
            &pool,
            thread.as_str(),
            user.as_str(),
            "q1",
            QueueStatus::Sending,
            1,
        )
        .await;
        seed_item(
            &pool,
            other.as_str(),
            user.as_str(),
            "q2",
            QueueStatus::Sending,
            1,
        )
        .await;
        let projection = projection(&pool);
        let mut interrupted = HashSet::new();
        interrupted.insert(thread.as_str().to_owned());
        recover_sending(&pool, &projection, &interrupted)
            .await
            .unwrap();
        // The row belonged to the cut-off turn: covered by the continuation.
        assert_eq!(status_of(&pool, "q1").await.unwrap(), QueueStatus::Sent);
        // No turn was recorded for this one: run it again.
        assert_eq!(status_of(&pool, "q2").await.unwrap(), QueueStatus::Queued);
    }

    #[tokio::test]
    async fn restart_continuation_is_hidden_from_the_queue() {
        let (pool, user, thread) = seeded().await;
        let system = UserBrief {
            id: "threadbase".into(),
            username: "threadbase".into(),
            display_name: "Threadbase".into(),
            avatar_url: None,
            color: "#000".into(),
        };
        let authors_json = serde_json::to_string(&[system]).unwrap();
        enqueue(
            &pool,
            NewQueuedInput {
                thread_id: thread.as_str(),
                author_id: user.as_str(),
                text: "The server restarted during the previous turn.",
                authors_json: Some(&authors_json),
                attachments_json: None,
                starting: false,
                created_at: now_ms(),
            },
        )
        .await
        .unwrap();
        enqueue(
            &pool,
            NewQueuedInput {
                thread_id: thread.as_str(),
                author_id: user.as_str(),
                text: "from a person",
                authors_json: None,
                attachments_json: None,
                starting: false,
                created_at: now_ms(),
            },
        )
        .await
        .unwrap();
        let items = list_for_thread(&pool, thread.as_str()).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "from a person");
    }
}
