//! Outbound notifications: inbox rows and the optional webhook.

use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_shared::{now_ms, SettingsDto};
use ulid::Ulid;

/// One inbox row, owned by the server and surfaced through `/api/inbox`.
pub struct InboxItem<'a> {
    pub user_id: &'a str,
    pub kind: &'a str,
    pub thread_id: &'a str,
    pub actor_id: Option<&'a str>,
    pub approval_id: Option<&'a str>,
    pub elicitation_id: Option<&'a str>,
    pub comment_id: Option<&'a str>,
    pub summary: &'a str,
}

pub async fn insert(pool: &SqlitePool, item: InboxItem<'_>) -> Result<(), ServerError> {
    sqlx::query(
        "INSERT INTO inbox(id, user_id, kind, thread_id, actor_id, approval_id, elicitation_id,
            comment_id, summary, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(item.user_id)
    .bind(item.kind)
    .bind(item.thread_id)
    .bind(item.actor_id)
    .bind(item.approval_id)
    .bind(item.elicitation_id)
    .bind(item.comment_id)
    .bind(item.summary)
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

/// Notify every thread member. A failure to write one inbox row must not lose
/// the originating event, so callers use the result only for logging.
pub async fn notify_members(
    pool: &SqlitePool,
    thread_id: &str,
    kind: &str,
    approval_id: Option<&str>,
    elicitation_id: Option<&str>,
    summary: &str,
) -> Result<(), ServerError> {
    let members: Vec<String> =
        sqlx::query_scalar("SELECT user_id FROM thread_members WHERE thread_id = ?")
            .bind(thread_id)
            .fetch_all(pool)
            .await?;
    for user_id in members {
        insert(
            pool,
            InboxItem {
                user_id: &user_id,
                kind,
                thread_id,
                actor_id: None,
                approval_id,
                elicitation_id,
                comment_id: None,
                summary,
            },
        )
        .await?;
    }
    Ok(())
}

/// Minimal webhook contract: POST JSON once, no retries, never blocks a thread.
/// Reads the thread creator's saved setting, falling back to the environment.
pub async fn webhook(pool: &SqlitePool, event: &str, thread_id: &str, summary: &str) {
    let mut url = std::env::var("THREADBASE_WEBHOOK_URL").unwrap_or_default();
    if url.trim().is_empty() {
        let settings: Option<String> = sqlx::query_scalar(
            "SELECT u.settings_json FROM threads t JOIN users u ON u.id = t.created_by WHERE t.id = ?",
        )
        .bind(thread_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some(settings) = settings
            .as_deref()
            .and_then(|json| serde_json::from_str::<SettingsDto>(json).ok())
        {
            url = settings.webhook_url.unwrap_or_default();
        }
    }
    if url.trim().is_empty() {
        return;
    }
    let payload = serde_json::json!({
        "event": event,
        "threadId": thread_id,
        "summary": summary,
        "createdAt": now_ms(),
    });
    let event = event.to_owned();
    tokio::spawn(async move {
        let result = reqwest::Client::new()
            .post(url)
            .timeout(std::time::Duration::from_secs(5))
            .json(&payload)
            .send()
            .await;
        if let Err(error) = result {
            tracing::warn!(%error, event, "webhook delivery failed");
        }
    });
}

#[must_use]
pub fn approval_summary(title: &str) -> String {
    format!("Approval requested: {title}")
}

#[must_use]
pub fn elicitation_summary(agent: &str, message: &str) -> String {
    format!("Question from {agent}: {message}")
}
