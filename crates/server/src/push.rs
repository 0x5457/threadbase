//! Web Push subscriptions and delivery.
//!
//! A background worker drains inbox rows that have not been pushed yet, so the
//! request paths that create approvals, mentions, completions and errors stay
//! independent of push transport failures. VAPID keys are generated once and
//! persisted in `push_config`.

use crate::error::ServerError;
use base64::Engine;
use sqlx::{FromRow, SqlitePool};
use std::sync::Arc;
use std::time::Duration;
use threadbase_core::state::{PushAction, PushMachine};
use threadbase_shared::{
    now_ms, ApiError, PushConfigResponse, PushStatus, PushSubscriptionRequest,
    PushUnsubscribeRequest,
};
use ulid::Ulid;
use web_push::{ContentEncoding, SubscriptionInfo, VapidSignatureBuilder, WebPushMessageBuilder};

const PUSH_INTERVAL: Duration = Duration::from_secs(5);
const MAX_PAYLOAD: usize = 3052;
/// Attempts before a notification is declared undeliverable.
const MAX_ATTEMPTS: i64 = 5;
/// First retry delay; doubles per attempt up to one hour.
const BACKOFF_BASE_MS: i64 = 30_000;

#[derive(Clone)]
struct Keys {
    private: String,
    public: String,
}

pub struct PushHub {
    pool: SqlitePool,
    client: reqwest::Client,
    keys: tokio::sync::OnceCell<Option<Keys>>,
}

#[derive(FromRow)]
struct PendingRow {
    id: String,
    user_id: String,
    thread_id: String,
    thread_title: String,
    project_id: String,
    summary: String,
    read_at: Option<i64>,
    push_attempts: i64,
}

#[derive(FromRow)]
struct SubscriptionRow {
    id: String,
    endpoint: String,
    p256dh: String,
    auth: String,
}

impl PushHub {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            pool,
            client,
            keys: tokio::sync::OnceCell::new(),
        }
    }

    pub fn spawn(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PUSH_INTERVAL).await;
                if let Err(error) = hub.deliver_pending().await {
                    tracing::warn!(%error, "push delivery failed");
                }
            }
        });
    }

    pub async fn config(&self) -> PushConfigResponse {
        match self.keys().await {
            Some(keys) => PushConfigResponse {
                enabled: true,
                public_key: Some(keys.public.clone()),
            },
            None => PushConfigResponse {
                enabled: false,
                public_key: None,
            },
        }
    }

    async fn keys(&self) -> Option<&Keys> {
        self.keys
            .get_or_init(|| async { self.load_or_generate().await })
            .await
            .as_ref()
    }

    async fn load_or_generate(&self) -> Option<Keys> {
        if let Ok(Some((private, public))) = sqlx::query_as::<_, (String, String)>(
            "SELECT private_key, public_key FROM push_config WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        {
            return Some(Keys { private, public });
        }
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        for _ in 0..16 {
            let bytes: [u8; 32] = rand::random();
            let private = engine.encode(bytes);
            let Ok(builder) = VapidSignatureBuilder::from_base64_no_sub(&private) else {
                continue;
            };
            let public = engine.encode(builder.get_public_key());
            if sqlx::query(
                "INSERT INTO push_config(id, private_key, public_key, created_at)
                 VALUES(1, ?, ?, ?)
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&private)
            .bind(&public)
            .bind(now_ms())
            .execute(&self.pool)
            .await
            .is_err()
            {
                return None;
            }
            // Another thread may have won the race; read the persisted pair.
            return sqlx::query_as::<_, (String, String)>(
                "SELECT private_key, public_key FROM push_config WHERE id = 1",
            )
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .map(|(private, public)| Keys { private, public });
        }
        None
    }

    pub async fn subscribe(
        &self,
        user_id: &str,
        request: &PushSubscriptionRequest,
    ) -> Result<(), ServerError> {
        if request.endpoint.trim().is_empty()
            || request.keys.p256dh.is_empty()
            || request.keys.auth.is_empty()
        {
            return Err(ServerError(ApiError::validation(
                "push subscription is incomplete",
            )));
        }
        // Endpoints are user supplied; never let one turn the push worker into
        // an SSRF proxy.
        if !crate::notify::allowed_webhook_url(&request.endpoint).await {
            return Err(ServerError(ApiError::validation(
                "push endpoint must be a public HTTPS URL",
            )));
        }
        sqlx::query(
            "INSERT INTO push_subscriptions(id, user_id, endpoint, p256dh, auth, created_at)
             VALUES(?, ?, ?, ?, ?, ?)
             ON CONFLICT(endpoint) DO UPDATE SET
                user_id = excluded.user_id,
                p256dh = excluded.p256dh,
                auth = excluded.auth",
        )
        .bind(Ulid::new().to_string())
        .bind(user_id)
        .bind(request.endpoint.trim())
        .bind(&request.keys.p256dh)
        .bind(&request.keys.auth)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unsubscribe(
        &self,
        user_id: &str,
        request: &PushUnsubscribeRequest,
    ) -> Result<(), ServerError> {
        sqlx::query("DELETE FROM push_subscriptions WHERE user_id = ? AND endpoint = ?")
            .bind(user_id)
            .bind(request.endpoint.trim())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn deliver_pending(&self) -> Result<(), ServerError> {
        let Some(keys) = self.keys().await else {
            return Ok(());
        };
        let now = now_ms();
        let rows: Vec<PendingRow> = sqlx::query_as(
            "SELECT i.id, i.user_id, i.thread_id, t.title AS thread_title,
                    t.project_id, i.summary, i.read_at, i.push_attempts
             FROM inbox i
             JOIN threads t ON t.id = i.thread_id
             WHERE i.push_status = 'pending'
               AND i.push_next_at <= ?
               AND i.push_attempts < ?
             ORDER BY i.created_at ASC
             LIMIT 50",
        )
        .bind(now)
        .bind(MAX_ATTEMPTS)
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let attempts = row.push_attempts + 1;
            // A notification read before delivery is not worth a phone buzz.
            if row.read_at.is_some() {
                self.record(&row.id, request(PushAction::Skip, attempts, None))
                    .await;
                continue;
            }
            let subs: Vec<SubscriptionRow> = sqlx::query_as(
                "SELECT id, endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
            )
            .bind(&row.user_id)
            .fetch_all(&self.pool)
            .await?;
            if subs.is_empty() {
                self.record(&row.id, request(PushAction::Skip, attempts, None))
                    .await;
                continue;
            }
            let payload = serde_json::json!({
                "title": row.thread_title,
                "body": row.summary,
                "url": format!("/p/{}/t/{}", row.project_id, row.thread_id),
                "tag": row.thread_id,
            })
            .to_string();
            let payload = truncate(payload, MAX_PAYLOAD);
            let mut delivered = false;
            let mut last_error: Option<String> = None;
            for sub in subs {
                match self.send_one(keys, &sub, &payload).await {
                    Ok(Outcome::Sent) => delivered = true,
                    Ok(Outcome::Dropped) => {}
                    Err(error) => {
                        tracing::warn!(endpoint = %sub.endpoint, %error, "push send failed");
                        last_error = Some(error.0.message);
                    }
                }
            }
            let action = next_action(delivered, last_error.is_some(), attempts);
            self.record(&row.id, request(action, attempts, last_error.as_deref()))
                .await;
        }
        Ok(())
    }

    /// Persist one push transition. Retries stay `pending` with a backoff so
    /// the worker can pick them up on a later tick.
    async fn record(&self, id: &str, request: PushUpdate) {
        let now = now_ms();
        let result = sqlx::query(
            "UPDATE inbox
                SET push_status = ?, push_attempts = ?, push_next_at = ?, push_error = ?
              WHERE id = ?",
        )
        .bind(request.status.as_str())
        .bind(request.attempts)
        .bind(request.next_at(now))
        .bind(request.error.as_deref())
        .bind(id)
        .execute(&self.pool)
        .await;
        if let Err(error) = result {
            tracing::warn!(%error, "could not persist push state");
        }
    }

    async fn send_one(
        &self,
        keys: &Keys,
        sub: &SubscriptionRow,
        payload: &str,
    ) -> Result<Outcome, ServerError> {
        if !crate::notify::allowed_webhook_url(&sub.endpoint).await {
            tracing::warn!(endpoint = %sub.endpoint, "push endpoint is not a public HTTPS URL; skipping");
            return Ok(Outcome::Dropped);
        }
        let info = SubscriptionInfo::new(
            sub.endpoint.as_str(),
            sub.p256dh.as_str(),
            sub.auth.as_str(),
        );
        let partial = VapidSignatureBuilder::from_base64_no_sub(&keys.private)
            .map_err(|error| ServerError::internal(format!("vapid key: {error}")))?;
        let signature = partial
            .add_sub_info(&info)
            .build()
            .map_err(|error| ServerError::internal(format!("vapid signature: {error}")))?;
        let mut builder = WebPushMessageBuilder::new(&info);
        builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
        builder.set_vapid_signature(signature);
        builder.set_ttl(3600);
        let message = builder
            .build()
            .map_err(|error| ServerError::internal(format!("push message: {error}")))?;
        let request = web_push::request_builder::build_request::<Vec<u8>>(message);
        let (parts, body) = request.into_parts();
        let mut outgoing = self
            .client
            .request(reqwest::Method::POST, parts.uri.to_string())
            .body(body)
            .timeout(Duration::from_secs(10));
        for (name, value) in parts.headers {
            if let Some(name) = name {
                outgoing = outgoing.header(name.as_str(), value.as_bytes());
            }
        }
        let response = outgoing
            .send()
            .await
            .map_err(|error| ServerError::internal(format!("push request: {error}")))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND
            || response.status() == reqwest::StatusCode::GONE
        {
            let _ = sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
                .bind(&sub.id)
                .execute(&self.pool)
                .await;
            return Ok(Outcome::Dropped);
        }
        if !response.status().is_success() {
            return Err(ServerError::internal(format!(
                "push service returned {}",
                response.status()
            )));
        }
        Ok(Outcome::Sent)
    }
}

/// What happened to one subscription attempt.
enum Outcome {
    Sent,
    /// The subscription is gone (404/410) or the endpoint is not allowed.
    Dropped,
}

/// A computed push transition, ready to persist.
struct PushUpdate {
    status: PushStatus,
    attempts: i64,
    error: Option<String>,
}

impl PushUpdate {
    fn next_at(&self, now: i64) -> i64 {
        if self.status != PushStatus::Pending {
            return 0;
        }
        // Exponential backoff, capped at one hour.
        let exponent = self.attempts.clamp(1, 7);
        let delay = BACKOFF_BASE_MS.saturating_mul(1_i64 << exponent);
        now.saturating_add(delay.min(3_600_000))
    }
}

/// Choose the transition for one delivery round.
fn next_action(delivered: bool, failed: bool, attempts: i64) -> PushAction {
    if delivered {
        PushAction::Deliver
    } else if failed && attempts >= MAX_ATTEMPTS {
        PushAction::GiveUp
    } else if failed {
        PushAction::Fail
    } else {
        // Every subscription was gone (410/404 or a non-public URL).
        PushAction::Skip
    }
}

/// Ask the machine for the next state, then attach the attempt bookkeeping.
fn request(action: PushAction, attempts: i64, error: Option<&str>) -> PushUpdate {
    let status = PushMachine::transition(PushStatus::Pending, &action).unwrap_or_else(|error| {
        // The row was `pending` when selected; an illegal action is a bug.
        debug_assert!(false, "illegal push transition: {error}");
        PushStatus::Failed
    });
    PushUpdate {
        status,
        attempts,
        error: error.map(str::to_owned),
    }
}

fn truncate(value: String, limit: usize) -> String {
    if value.len() <= limit {
        return value;
    }
    let mut cut = limit;
    while cut > 0 && !value.is_char_boundary(cut) {
        cut -= 1;
    }
    value[..cut].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn transport_failures_retry_until_the_budget_runs_out() {
        assert_eq!(next_action(false, true, 1), PushAction::Fail);
        assert_eq!(next_action(false, true, MAX_ATTEMPTS), PushAction::GiveUp);
        assert_eq!(next_action(true, true, 1), PushAction::Deliver);
        assert_eq!(next_action(false, false, 3), PushAction::Skip);
    }

    #[test]
    fn a_failed_attempt_schedules_the_next_one_with_backoff() {
        let update = request(PushAction::Fail, 1, Some("boom"));
        assert_eq!(update.status, PushStatus::Pending);
        assert_eq!(update.next_at(1_000), 1_000 + BACKOFF_BASE_MS * 2);
        let later = request(PushAction::Fail, 3, Some("boom"));
        assert!(later.next_at(1_000) > update.next_at(1_000));
        let sent = request(PushAction::Deliver, 1, None);
        assert_eq!(sent.status, PushStatus::Sent);
        assert_eq!(sent.next_at(1_000), 0, "terminal rows are never polled");
    }

    #[tokio::test]
    async fn generates_a_stable_public_key_and_rejects_private_endpoints() {
        let pool = db::connect_memory().await.unwrap();
        let hub = PushHub::new(pool.clone());
        let first = hub.config().await;
        assert!(first.enabled);
        assert!(first
            .public_key
            .as_deref()
            .is_some_and(|key| !key.is_empty()));
        let second = hub.config().await;
        assert_eq!(first.public_key, second.public_key);
        let rejected = hub
            .subscribe(
                "user",
                &PushSubscriptionRequest {
                    endpoint: "https://127.0.0.1/push".into(),
                    keys: threadbase_shared::api::PushSubscriptionKeys {
                        p256dh: "key".into(),
                        auth: "auth".into(),
                    },
                },
            )
            .await;
        assert!(rejected.is_err());
    }
}
