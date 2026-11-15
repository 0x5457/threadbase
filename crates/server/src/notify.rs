//! Outbound notifications: inbox rows and the optional webhook.

use crate::error::ServerError;
use sqlx::SqlitePool;
use std::net::{IpAddr, Ipv4Addr};
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

/// Notify every unmuted watcher of a thread. A legacy thread without watcher
/// rows falls back to its creator. Completion notifications skip users whose
/// current focus is already this thread.
#[allow(clippy::too_many_arguments)]
pub async fn notify_watchers(
    pool: &SqlitePool,
    projection: &crate::projection::Projection,
    thread_id: &str,
    kind: &str,
    approval_id: Option<&str>,
    elicitation_id: Option<&str>,
    summary: &str,
    presence: Option<&crate::presence::PresenceHub>,
) -> Result<(), ServerError> {
    let mut recipients = crate::watchers::recipients(pool, thread_id).await?;
    if let Some(presence) = presence {
        let focused = presence.focused_user_ids(thread_id).await;
        recipients.retain(|user_id| !focused.contains(user_id));
    }
    for user_id in recipients {
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
        crate::inbox::publish_count(pool, projection, &user_id).await;
    }
    Ok(())
}

/// Insert one notification and refresh that user's badge.
pub async fn notify_user(
    pool: &SqlitePool,
    projection: &crate::projection::Projection,
    item: InboxItem<'_>,
) -> Result<(), ServerError> {
    let user_id = item.user_id.to_owned();
    insert(pool, item).await?;
    crate::inbox::publish_count(pool, projection, &user_id).await;
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
        if !allowed_webhook_url(&url).await {
            tracing::warn!(event, "webhook URL is not allowed; skipping delivery");
            return;
        }
        let Ok(client) = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            tracing::warn!(
                event,
                "webhook client could not be created; skipping delivery"
            );
            return;
        };
        let result = client
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

/// Webhook URLs are user supplied, so they must not be usable as an SSRF
/// proxy into loopback or private networks. Hostnames are resolved here, and
/// every returned address must be public.
pub async fn allowed_webhook_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let Some(port) = parsed.port_or_known_default() else {
        return false;
    };
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) => is_public_ip(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => is_public_ip(IpAddr::V6(ip)),
        Some(url::Host::Domain(domain)) => {
            let Ok(addresses) = tokio::net::lookup_host((domain, port)).await else {
                return false;
            };
            let mut resolved = false;
            for address in addresses {
                resolved = true;
                if !is_public_ip(address.ip()) {
                    return false;
                }
            }
            resolved
        }
        None => false,
    }
}

// std's `IpAddr::is_global` is still unstable (tracking issue #27709) and,
// once enabled, treats multicast and IPv4-mapped addresses as global. Both
// would weaken this SSRF guard, so the address classes stay explicit here.
fn is_public_ip(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => {
            // IPv4-mapped (and IPv4-compatible) forms must be judged as IPv4.
            if let Some(mapped) = ip.to_ipv4_mapped().or_else(|| ip.to_ipv4()) {
                return is_public_ipv4(mapped);
            }
            let first = ip.segments()[0];
            // fc00::/7 unique-local and fe80::/10 link-local; ff00::/8
            // multicast was rejected above.
            first & 0xfe00 != 0xfc00 && first & 0xffc0 != 0xfe80
        }
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
    {
        return false;
    }
    let octets = ip.octets();
    // 100.64.0.0/10 carrier-grade NAT.
    !(octets[0] == 100 && (octets[1] & 0xc0) == 64)
}

#[must_use]
pub fn approval_summary(title: &str) -> String {
    format!("Approval requested: {title}")
}

#[must_use]
pub fn elicitation_summary(agent: &str, message: &str) -> String {
    format!("Question from {agent}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_internal_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fc00::1",
            "fdff::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:0.0.0.0",
            "::ffff:224.0.0.1",
            "::ffff:255.255.255.255",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} must be rejected");
        }
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
        assert!(is_public_ip("::ffff:8.8.8.8".parse().unwrap()));
    }

    #[tokio::test]
    async fn webhook_url_requires_https_and_a_public_host() {
        assert!(!allowed_webhook_url("http://1.1.1.1/hook").await);
        assert!(!allowed_webhook_url("https://127.0.0.1/hook").await);
        assert!(!allowed_webhook_url("https://10.0.0.1/hook").await);
        assert!(!allowed_webhook_url("https://[::1]/hook").await);
        assert!(!allowed_webhook_url("not a url").await);
        assert!(allowed_webhook_url("https://1.1.1.1/hook").await);
        assert!(allowed_webhook_url("https://[2606:4700:4700::1111]/hook").await);
    }
}
