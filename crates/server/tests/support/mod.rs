//! Shared setup for the integration tests.
//!
//! Each test binary compiles this module separately, so helpers used by one
//! binary are unused in the others.
#![allow(dead_code)]
//!
//! A git repo, a data directory, a database with one user, one project and one
//! thread, and the HTTP app.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::Router;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use threadbase_server::auth::CurrentUser;
use threadbase_server::config::Config;
use threadbase_server::{auth, db, projects, threads};
use threadbase_shared::api::{CreateProjectRequest, CreateThreadRequest};

pub struct Fixture {
    /// Kept alive so the worktree and data directory outlive the test.
    pub _repo: TempDir,
    pub _data: TempDir,
    pub pool: SqlitePool,
    pub current: CurrentUser,
    pub thread_id: String,
}

/// Builds the fixture. `config` carries the flags a test needs; the paths are
/// filled in here.
pub async fn fixture(config: Config) -> (Fixture, Router) {
    let repo = TempDir::new().unwrap();
    init_repo(repo.path()).await;
    let data = TempDir::new().unwrap();
    let pool = db::connect_memory().await.unwrap();
    let user = auth::ensure_identity_user(
        &pool,
        &auth::Identity::tailscale("ada@example.com", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let current = CurrentUser {
        user: user.clone(),
        method: "tailscale",
    };
    let config = Config {
        data_dir: data.path().to_path_buf(),
        db_path: data.path().join("threadbase.db"),
        ..config
    };
    let state = threadbase_server::app::AppState::new(config, pool.clone());
    let project = projects::create(
        &pool,
        &current,
        &CreateProjectRequest {
            path: repo.path().to_string_lossy().to_string(),
            name: None,
            create: false,
        },
    )
    .await
    .unwrap();
    let thread = threads::create(
        &pool,
        &state.projection,
        data.path(),
        &current,
        project.id.as_str(),
        &CreateThreadRequest {
            title: Some("Test thread".into()),
            agent: Some("codex".into()),
            model: None,
            effort: None,
            base_branch: Some("main".into()),
            initial_prompt: None,
            worktree: None,
        },
        &threadbase_core::permissions::PermissionPolicy::default(),
    )
    .await
    .unwrap();
    let app = threadbase_server::routes::router(state);
    (
        Fixture {
            _repo: repo,
            _data: data,
            pool,
            current,
            thread_id: thread.id.0.clone(),
        },
        app,
    )
}

pub async fn init_repo(path: &Path) {
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "test@example.com"]);
    git(path, &["config", "user.name", "Test"]);
    tokio::fs::write(path.join("README.md"), "one\ntwo\n")
        .await
        .unwrap();
    git(path, &["add", "-A"]);
    git(path, &["commit", "-m", "initial"]);
}

pub fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A request from 127.0.0.1 with the headers the CSRF guard expects.
pub fn request(method: &str, uri: &str, cookie: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-requested-with", "threadbase")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let mut request = builder.body(Body::from(body.to_owned())).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1234))));
    request
}

/// The `name=value` pair from a response's `Set-Cookie`.
pub fn cookie_of(response: &axum::http::Response<Body>) -> String {
    response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or_default()
        .to_owned()
}
