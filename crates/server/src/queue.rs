//! Thread-level queued input listing.

use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
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
        status: match row.status.as_str() {
            "sending" => QueueStatus::Sending,
            "sent" => QueueStatus::Sent,
            "superseded" => QueueStatus::Superseded,
            "dropped" => QueueStatus::Dropped,
            _ => QueueStatus::Queued,
        },
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
