//! The MCP endpoint: token auth, scopes and the JSON-RPC surface.

mod support;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use support::{fixture, Fixture};
use threadbase_server::auth;
use threadbase_server::auth::CurrentUser;
use threadbase_server::config::Config;
use threadbase_server::mcp;
use threadbase_shared::api::CreateMcpTokenRequest;
use threadbase_shared::{McpScope, UserAccess};
use tower::ServiceExt;

async fn rpc(app: &Router, token: Option<&str>, body: Value) -> (StatusCode, Option<Value>) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let mut request = builder.body(Body::from(body.to_string())).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            1234,
        ))));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).ok();
    (status, value)
}

async fn mint(fixture: &Fixture, scopes: &[McpScope]) -> String {
    mcp::mint(&fixture.pool, &fixture.current, "test client", scopes)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_endpoint_does_not_exist_when_disabled() {
    let (_fixture, app) = fixture(Config::default()).await;
    let (status, _) = rpc(&app, None, json!({"jsonrpc":"2.0","id":1,"method":"ping"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let request = Request::builder()
        .method("GET")
        .uri("/api/mcp/tokens")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_bearer_token_is_required_and_revocable() {
    let (fixture, app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    let (status, _) = rpc(&app, None, json!({"jsonrpc":"2.0","id":1,"method":"ping"})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = rpc(
        &app,
        Some("not-a-token"),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let token = mint(&fixture, &[McpScope::Read]).await;
    let (status, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.unwrap()["result"], json!({}));

    let tokens = mcp::list(&fixture.pool, fixture.current.user.id.as_str())
        .await
        .unwrap();
    assert_eq!(tokens.len(), 1);
    mcp::revoke(
        &fixture.pool,
        fixture.current.user.id.as_str(),
        &tokens[0].id,
    )
    .await
    .unwrap();
    let (status, _) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "revoked tokens stop working"
    );
}

#[tokio::test]
async fn initialize_and_tools_list_describe_the_surface() {
    let (fixture, app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    let token = mint(&fixture, &[McpScope::Read]).await;
    let (_, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}),
    )
    .await;
    let body = body.unwrap();
    assert!(body["result"]["protocolVersion"].is_string());
    assert_eq!(
        body["result"]["capabilities"]["tools"]["listChanged"],
        false
    );

    let (_, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let tools = body.unwrap()["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    for expected in [
        "threads_list",
        "thread_read",
        "thread_prompt",
        "thread_wait",
        "approvals_list",
        "approval_decide",
    ] {
        assert!(
            tools.contains(&expected.to_owned()),
            "missing {expected}: {tools:?}"
        );
    }

    // A notification has no id and must not produce a reply.
    let (status, _) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn read_tokens_can_list_and_read_but_not_prompt() {
    let (fixture, app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    let token = mint(&fixture, &[McpScope::Read]).await;

    let (_, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"threads_list","arguments":{}}}),
    )
    .await;
    let text = body.unwrap()["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text")
        .to_owned();
    let listed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(listed["threads"].as_array().unwrap().len(), 1);

    let (_, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"thread_read","arguments":{"thread_id": fixture.thread_id}}}),
    )
    .await;
    let text = body.unwrap()["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text")
        .to_owned();
    let read: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(read["thread"]["title"], "Test thread");

    // The write tool is refused before anything reaches an agent.
    let (_, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"thread_prompt","arguments":{"thread_id": fixture.thread_id, "text": "go"}}}),
    )
    .await;
    let body = body.unwrap();
    assert_eq!(body["error"]["code"], -32003, "{body}");
    let turns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ? AND kind = 'turn_started'",
    )
    .bind(&fixture.thread_id)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(turns, 0);
}

#[tokio::test]
async fn granting_write_needs_an_account_that_can_write() {
    let (fixture, _app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    sqlx::query("UPDATE users SET access = ? WHERE id = ?")
        .bind(UserAccess::ReadOnly.as_str())
        .bind(fixture.current.user.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let read_only = auth::load_user_by_id(&fixture.pool, fixture.current.user.id.as_str())
        .await
        .unwrap()
        .unwrap();
    let principal = CurrentUser {
        user: read_only,
        method: "tailscale",
    };
    let error = mcp::mint(
        &fixture.pool,
        &principal,
        "writer",
        &[McpScope::Read, McpScope::Write],
    )
    .await
    .expect_err("a read-only account cannot mint a write token");
    assert_eq!(
        error.0.code,
        threadbase_shared::ErrorCode::Forbidden,
        "{}",
        error.0.message
    );
}

#[tokio::test]
async fn approving_needs_the_approve_scope_and_an_admin_mint() {
    let (fixture, app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    // The fixture's first user is the instance admin; a second member is not.
    let member = auth::ensure_identity_user(
        &fixture.pool,
        &auth::Identity::tailscale("bob@example.com", Some("Bob"), None),
        &[],
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET access = ?, is_admin = 0 WHERE id = ?")
        .bind(UserAccess::Member.as_str())
        .bind(member.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let member = CurrentUser {
        user: auth::load_user_by_id(&fixture.pool, member.id.as_str())
            .await
            .unwrap()
            .unwrap(),
        method: "tailscale",
    };
    let error = mcp::mint(
        &fixture.pool,
        &member,
        "approver",
        &[McpScope::Read, McpScope::Approve],
    )
    .await
    .expect_err("members cannot mint approve tokens");
    assert_eq!(error.0.code, threadbase_shared::ErrorCode::Forbidden);

    // An admin may, and then the scope is what decides.
    let admin_token = mcp::mint(
        &fixture.pool,
        &fixture.current,
        "admin approver",
        &[McpScope::Read, McpScope::Approve],
    )
    .await
    .unwrap();
    assert!(!admin_token.is_empty());
    let read_only = mint(&fixture, &[McpScope::Read]).await;
    let (_, body) = rpc(
        &app,
        Some(&read_only),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"approval_decide","arguments":{"approval_id":"nope","decision":"allow_once"}}}),
    )
    .await;
    assert_eq!(body.unwrap()["error"]["code"], -32003);
}

#[tokio::test]
async fn waiting_returns_as_soon_as_the_thread_is_idle() {
    let (fixture, app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    let token = mint(&fixture, &[McpScope::Read]).await;
    let started = std::time::Instant::now();
    let (_, body) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"thread_wait","arguments":{"thread_id": fixture.thread_id, "timeout_seconds": 5}}}),
    )
    .await;
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    let text = body.unwrap()["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text")
        .to_owned();
    let waited: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(waited["reached"], true);
    assert_eq!(waited["state"]["status"], "idle");
}

/// A token is delegated authority, not stored authority: when the account stops
/// being able to drive threads, the token stops working too.
#[tokio::test]
async fn a_token_stops_working_when_its_account_is_suspended() {
    let (fixture, app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    let token = mint(&fixture, &[McpScope::Read]).await;
    sqlx::query("UPDATE users SET access = ? WHERE id = ?")
        .bind(UserAccess::Disabled.as_str())
        .bind(fixture.current.user.id.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let (status, _) = rpc(
        &app,
        Some(&token),
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a suspended account keeps nothing"
    );
}

/// The token DTO round-trips through the API the settings page uses.
#[tokio::test]
async fn tokens_are_listed_without_ever_showing_the_secret() {
    let (fixture, _app) = fixture(Config {
        mcp: true,
        ..Config::default()
    })
    .await;
    let request = CreateMcpTokenRequest {
        label: "laptop".into(),
        scopes: vec![McpScope::Read],
    };
    let token = mcp::mint(
        &fixture.pool,
        &fixture.current,
        &request.label,
        &request.scopes,
    )
    .await
    .unwrap();
    let listed = mcp::list(&fixture.pool, fixture.current.user.id.as_str())
        .await
        .unwrap();
    let serialized = serde_json::to_string(&listed).unwrap();
    assert!(
        !serialized.contains(&token),
        "the secret is never in the list"
    );
    assert_eq!(listed[0].label, "laptop");
    assert_eq!(listed[0].scopes, vec![McpScope::Read]);
}
