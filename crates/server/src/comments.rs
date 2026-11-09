//! Thread comments and their semantic anchors.

use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::{user_color, CommentAnchor, CommentDto, CommentStatus, UserBrief};

#[derive(Debug, FromRow)]
struct CommentRow {
    id: String,
    thread_id: String,
    parent_id: Option<String>,
    author_id: String,
    body: String,
    anchor_json: String,
    anchor_changed: i64,
    status: String,
    delivered_seq: Option<i64>,
    created_at: i64,
    updated_at: i64,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
}

pub async fn list_for_thread(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Vec<CommentDto>, ServerError> {
    let rows = sqlx::query_as::<_, CommentRow>(
        "SELECT c.id, c.thread_id, c.parent_id, c.author_id, c.body, c.anchor_json,
                c.anchor_changed, c.status, c.delivered_seq, c.created_at, c.updated_at,
                u.username, u.display_name, u.avatar_url
         FROM comments c JOIN users u ON u.id = c.author_id
         WHERE c.thread_id = ? AND c.status != 'draft'
         ORDER BY c.created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(dto).collect())
}

fn dto(row: CommentRow) -> CommentDto {
    let anchor =
        serde_json::from_str::<CommentAnchor>(&row.anchor_json).unwrap_or(CommentAnchor::Thread);
    CommentDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        parent_id: row.parent_id.map(Into::into),
        author: UserBrief {
            id: row.author_id.clone().into(),
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            color: user_color(&row.author_id).to_owned(),
        },
        body: row.body,
        anchor,
        anchor_changed: row.anchor_changed != 0,
        status: match row.status.as_str() {
            "draft" => CommentStatus::Draft,
            "delivered" => CommentStatus::Delivered,
            "resolved" => CommentStatus::Resolved,
            _ => CommentStatus::Pending,
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
        delivered_seq: row
            .delivered_seq
            .and_then(|value| u64::try_from(value).ok()),
    }
}
