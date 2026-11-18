//! HTTP tests for the agent catalogue: listing, adding, editing and removing
//! workspace-configured agents, and the admin gate around configuration.

use axum::body::Body;
use axum::http::StatusCode;
use serde_json::Value;
use threadbase_server::auth::Identity;
use threadbase_server::auth_session;
use tower::ServiceExt;

mod support;

use support::{fixture, request};

struct Test {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    cookie: String,
    _data: tempfile::TempDir,
}

async fn setup() -> Test {
    let (fixture, app) = fixture(threadbase_server::config::Config::default()).await;
    let pool = fixture.pool.clone();
    let session = auth_session::create(
        &pool,
        fixture.current.user.id.as_str(),
        "tailscale",
        None,
        None,
        3600,
    )
    .await
    .unwrap();
    Test {
        app,
        pool,
        cookie: format!("threadbase_session={session}"),
        _data: fixture._data,
    }
}

/// A second, non-admin member with a session of their own.
async fn member_session(pool: &sqlx::SqlitePool) -> String {
    let member = threadbase_server::auth::ensure_identity_user(
        pool,
        &Identity::tailscale("bob@example.com", Some("Bob"), None),
        &[],
    )
    .await
    .unwrap();
    assert!(!member.is_admin);
    sqlx::query("UPDATE users SET access = 'member' WHERE id = ?")
        .bind(member.id.as_str())
        .execute(pool)
        .await
        .unwrap();
    auth_session::create(pool, member.id.as_str(), "tailscale", None, None, 3600)
        .await
        .unwrap()
}

async fn body(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn agent_input(label: &str, command: &str) -> String {
    serde_json::json!({
        "label": label,
        "command": command,
        "args": ["--profile", "acp"],
        "env": [{ "key": "DEEPSEEK_API_KEY", "value": "sk-test" }],
    })
    .to_string()
}

async fn agent_ids(test: &Test) -> Vec<String> {
    let response = test
        .app
        .clone()
        .oneshot(request("GET", "/api/agents", Some(&test.cookie), ""))
        .await
        .unwrap();
    body(response)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| agent["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn listing_shows_the_built_ins_to_every_member() {
    let test = setup().await;
    let app = test.app.clone();
    let response = app
        .oneshot(request("GET", "/api/agents", Some(&test.cookie), ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let agents = body(response).await;
    let ids = agent_ids(&test).await;
    for expected in ["codex", "claude", "opencode", "pi"] {
        assert!(
            ids.contains(&expected.to_owned()),
            "missing built-in {expected}"
        );
    }
    assert!(!agents[0]["custom"].as_bool().unwrap());
    assert!(agents[0]["env"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn listing_requires_a_session() {
    let test = setup().await;
    let response = test
        .app
        .oneshot(request("GET", "/api/agents", None, ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn members_cannot_configure_agents() {
    let test = setup().await;
    let member = member_session(&test.pool).await;
    let response = test
        .app
        .oneshot(request(
            "POST",
            "/api/agents",
            Some(&format!("threadbase_session={member}")),
            &agent_input("DeepSeek", "dsh"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_admin_can_add_edit_and_remove_a_custom_agent() {
    let test = setup().await;
    let response = test
        .app
        .clone()
        .oneshot(request(
            "POST",
            "/api/agents",
            Some(&test.cookie),
            &agent_input("DeepSeek Harness", "definitely-not-installed-dsh"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let agent = body(response).await;
    assert_eq!(agent["id"], "deepseek-harness");
    assert_eq!(agent["custom"], true);
    assert_eq!(agent["installed"], false);
    assert_eq!(agent["env"][0]["key"], "DEEPSEEK_API_KEY");

    // The id is fixed to the original slug, so threads keep addressing it.
    let response = test
        .app
        .clone()
        .oneshot(request(
            "PATCH",
            "/api/agents/deepseek-harness",
            Some(&test.cookie),
            &agent_input("DeepSeek (renamed)", "definitely-not-installed-dsh"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let agent = body(response).await;
    assert_eq!(agent["id"], "deepseek-harness");
    assert_eq!(agent["label"], "DeepSeek (renamed)");

    assert!(agent_ids(&test)
        .await
        .contains(&"deepseek-harness".to_owned()));
    let response = test
        .app
        .clone()
        .oneshot(request(
            "DELETE",
            "/api/agents/deepseek-harness",
            Some(&test.cookie),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!agent_ids(&test)
        .await
        .contains(&"deepseek-harness".to_owned()));
}

#[tokio::test]
async fn a_name_that_collides_is_rejected() {
    let test = setup().await;

    // A built-in name cannot be shadowed by a custom agent.
    let response = test
        .app
        .clone()
        .oneshot(request(
            "POST",
            "/api/agents",
            Some(&test.cookie),
            &agent_input("Codex", "other-command"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Nor can the same name be used twice.
    let response = test
        .app
        .clone()
        .oneshot(request(
            "POST",
            "/api/agents",
            Some(&test.cookie),
            &agent_input("Duo", "duo-command"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = test
        .app
        .clone()
        .oneshot(request(
            "POST",
            "/api/agents",
            Some(&test.cookie),
            &agent_input("Duo", "duo-command"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_pasted_shell_command_is_rejected() {
    let test = setup().await;
    let response = test
        .app
        .oneshot(request(
            "POST",
            "/api/agents",
            Some(&test.cookie),
            &agent_input("Shell paste", "npx -y dsh"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
