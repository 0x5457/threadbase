//! Guest links: what a guest can do, and the member API it must not open.

mod support;

use axum::http::StatusCode;
use support::{cookie_of, fixture, request};
use threadbase_server::config::Config;
use threadbase_server::guest;
use threadbase_shared::api::{GuestMessageRequest, GuestSessionRequest};
use threadbase_shared::{now_ms, ErrorCode};
use tower::ServiceExt;

#[tokio::test]
async fn guest_endpoints_do_not_exist_when_disabled() {
    let (fixture, app) = fixture(Config::default()).await;
    for (method, uri) in [
        ("POST", "/api/guest/session"),
        ("GET", "/api/guest/thread"),
        ("POST", "/api/guest/messages"),
        (
            "GET",
            format!("/api/threads/{}/guest-messages", fixture.thread_id).leak(),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(request(method, uri, None, "{}"))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} {uri} must not exist while the feature is off"
        );
    }
}

#[tokio::test]
async fn a_link_is_exchanged_for_a_cookie_that_reads_only_its_thread() {
    let (fixture, app) = fixture(Config {
        guest_links: true,
        ..Config::default()
    })
    .await;
    let (token, _) = guest::create_link(
        &fixture.pool,
        &fixture.thread_id,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();

    // Without a cookie the guest API says nothing.
    let anonymous = app
        .clone()
        .oneshot(request("GET", "/api/guest/thread", None, ""))
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::FORBIDDEN);

    let body = serde_json::to_string(&GuestSessionRequest {
        token: token.clone(),
    })
    .unwrap();
    let exchanged = app
        .clone()
        .oneshot(request("POST", "/api/guest/session", None, &body))
        .await
        .unwrap();
    assert_eq!(exchanged.status(), StatusCode::OK);
    let cookie = cookie_of(&exchanged);
    assert!(
        cookie.starts_with("threadbase_guest="),
        "the exchange sets the guest cookie: {cookie}"
    );

    let read = app
        .clone()
        .oneshot(request("GET", "/api/guest/thread", Some(&cookie), ""))
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(read.into_body(), 1 << 20)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["title"], "Test thread");
    assert!(
        json["page"].get("events").is_some(),
        "the transcript window is part of the response"
    );

    // A guest cookie is not a member session: the ordinary API stays closed.
    let member_api = app
        .clone()
        .oneshot(request("GET", "/api/threads", Some(&cookie), ""))
        .await
        .unwrap();
    assert!(
        !member_api.status().is_success(),
        "a guest must not reach the member API, got {}",
        member_api.status()
    );
}

#[tokio::test]
async fn revoking_a_link_ends_the_sessions_it_opened() {
    let (fixture, app) = fixture(Config {
        guest_links: true,
        ..Config::default()
    })
    .await;
    let (token, _) = guest::create_link(
        &fixture.pool,
        &fixture.thread_id,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    let body = serde_json::to_string(&GuestSessionRequest { token }).unwrap();
    let exchanged = app
        .clone()
        .oneshot(request("POST", "/api/guest/session", None, &body))
        .await
        .unwrap();
    let cookie = cookie_of(&exchanged);

    guest::revoke_thread_links(&fixture.pool, &fixture.thread_id)
        .await
        .unwrap();
    let read = app
        .clone()
        .oneshot(request("GET", "/api/guest/thread", Some(&cookie), ""))
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_expired_or_unknown_link_does_not_exchange() {
    let (fixture, app) = fixture(Config {
        guest_links: true,
        ..Config::default()
    })
    .await;
    let (token, _) = guest::create_link(
        &fixture.pool,
        &fixture.thread_id,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE guest_links SET expires_at = ?")
        .bind(now_ms() - 1_000)
        .execute(&fixture.pool)
        .await
        .unwrap();
    for candidate in [token, "0".repeat(64)] {
        let body = serde_json::to_string(&GuestSessionRequest { token: candidate }).unwrap();
        let response = app
            .clone()
            .oneshot(request("POST", "/api/guest/session", None, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn drafts_are_limited_and_can_only_be_claimed_once() {
    let (fixture, _) = fixture(Config {
        guest_links: true,
        ..Config::default()
    })
    .await;
    let (token, _) = guest::create_link(
        &fixture.pool,
        &fixture.thread_id,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    guest::exchange(&fixture.pool, &token, None).await.unwrap();
    let context = sqlx::query_as::<_, guest::GuestContext>(
        "SELECT s.link_id, l.thread_id, l.expires_at AS link_expires_at,
                s.expires_at AS session_expires_at
           FROM guest_sessions s JOIN guest_links l ON l.id = s.link_id",
    )
    .fetch_one(&fixture.pool)
    .await
    .unwrap();

    let empty = guest::draft(&fixture.pool, &context, "   ")
        .await
        .unwrap_err();
    assert_eq!(empty.0.code, ErrorCode::Validation);
    let long = guest::draft(&fixture.pool, &context, &"x".repeat(9_000))
        .await
        .unwrap_err();
    assert_eq!(long.0.code, ErrorCode::Validation);

    let first = guest::draft(&fixture.pool, &context, "Could you also add a test?")
        .await
        .unwrap();
    for index in 0..4 {
        guest::draft(&fixture.pool, &context, &format!("note {index}"))
            .await
            .unwrap();
    }
    let blocked = guest::draft(&fixture.pool, &context, "one more")
        .await
        .unwrap_err();
    assert_eq!(
        blocked.0.code,
        ErrorCode::Conflict,
        "a guest cannot queue unbounded drafts"
    );

    // Two people pressing Send must not both start a turn.
    let claimed = guest::claim(
        &fixture.pool,
        &fixture.thread_id,
        &first,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    assert_eq!(claimed, "Could you also add a test?");
    let again = guest::claim(
        &fixture.pool,
        &fixture.thread_id,
        &first,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap_err();
    assert_eq!(again.0.code, ErrorCode::Conflict);

    // A failed send puts the draft back instead of losing it.
    guest::release(&fixture.pool, &first).await;
    let pending = guest::list_for_thread(&fixture.pool, &fixture.thread_id)
        .await
        .unwrap();
    assert_eq!(pending.len(), 5);
    assert!(pending.iter().any(|message| message.id == first));

    guest::dismiss(
        &fixture.pool,
        &fixture.thread_id,
        &first,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    let after = guest::list_for_thread(&fixture.pool, &fixture.thread_id)
        .await
        .unwrap();
    assert_eq!(after.len(), 4);
}

#[tokio::test]
async fn a_guest_can_only_draft_text_through_its_link() {
    let (fixture, app) = fixture(Config {
        guest_links: true,
        ..Config::default()
    })
    .await;
    let (token, _) = guest::create_link(
        &fixture.pool,
        &fixture.thread_id,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    let body = serde_json::to_string(&GuestSessionRequest { token }).unwrap();
    let exchanged = app
        .clone()
        .oneshot(request("POST", "/api/guest/session", None, &body))
        .await
        .unwrap();
    let cookie = cookie_of(&exchanged);

    let draft = serde_json::to_string(&GuestMessageRequest {
        text: "What does this change do?".into(),
    })
    .unwrap();
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/guest/messages",
            Some(&cookie),
            &draft,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let pending = guest::list_for_thread(&fixture.pool, &fixture.thread_id)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, "What does this change do?");
    // The guest's draft is not a turn: nothing was sent to an agent.
    let turns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ? AND kind = 'turn_started'",
    )
    .bind(&fixture.thread_id)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(turns, 0, "a guest draft must not start a turn");
}

/// The guest payload is filtered on the server: a link reads the transcript,
/// not the workbench. Queued member drafts and approval requests stay out.
#[tokio::test]
async fn the_guest_payload_leaves_out_workbench_events() {
    let (fixture, app) = fixture(Config {
        guest_links: true,
        ..Config::default()
    })
    .await;
    let events = threadbase_server::events::EventStore::new(fixture.pool.clone());
    let actor = threadbase_shared::Actor::system();
    for payload in [
        threadbase_shared::ThreadEvent::QueueUpdated { items: Vec::new() },
        threadbase_shared::ThreadEvent::SystemNotice {
            level: "info".to_owned(),
            message: "visible to guests".to_owned(),
        },
    ] {
        events
            .append(
                &fixture.thread_id.clone().into(),
                actor.clone(),
                payload,
                threadbase_server::events::EventMeta::default(),
            )
            .await
            .unwrap();
    }

    let (token, _) = guest::create_link(
        &fixture.pool,
        &fixture.thread_id,
        fixture.current.user.id.as_str(),
    )
    .await
    .unwrap();
    let body = serde_json::to_string(&GuestSessionRequest { token }).unwrap();
    let exchanged = app
        .clone()
        .oneshot(request("POST", "/api/guest/session", None, &body))
        .await
        .unwrap();
    let cookie = cookie_of(&exchanged);
    let read = app
        .clone()
        .oneshot(request("GET", "/api/guest/thread", Some(&cookie), ""))
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(read.into_body(), 1 << 20)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let kinds: Vec<&str> = json["page"]["events"]
        .as_array()
        .expect("events array")
        .iter()
        .filter_map(|event| event.pointer("/payload/type").and_then(|v| v.as_str()))
        .collect();
    assert!(kinds.contains(&"system_notice"), "{kinds:?}");
    assert!(
        !kinds.contains(&"queue_updated"),
        "a guest must not see what members have queued: {kinds:?}"
    );
}
