//! The Sessions read model composes threads and host tmux sessions without
//! collapsing their permissions or lifecycles.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use tempfile::TempDir;
use threadbase_server::app::AppState;
use threadbase_server::auth_session;
use threadbase_server::config::Config;
use threadbase_server::{db, routes};
use threadbase_shared::now_ms;
use tower::ServiceExt;

struct Fixture {
    app: axum::Router,
    pool: SqlitePool,
    _data: TempDir,
    worktree: TempDir,
    socket: String,
    host_socket: String,
    pinned: String,
    running: String,
    archived: String,
    admin: String,
    member: String,
    read_only: String,
}

async fn fixture(host: bool) -> Fixture {
    let worktree = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let pool = db::connect_memory().await.unwrap();
    let socket = format!("tb-test-{}", ulid::Ulid::new());
    let host_socket = format!("tb-host-test-{}", ulid::Ulid::new());
    let config = Config {
        data_dir: data.path().to_path_buf(),
        db_path: data.path().join("threadbase.db"),
        bind: "127.0.0.1:0".parse().unwrap(),
        terminal: true,
        terminal_host: host,
        terminal_host_sockets: host.then(|| host_socket.clone()).into_iter().collect(),
        terminal_shell: "/bin/bash".to_owned(),
        terminal_socket: socket.clone(),
        ..Config::default()
    };
    let app = routes::router(AppState::new(config, pool.clone()));

    let admin = user(&pool, "ada", true, "member").await;
    let member = user(&pool, "bo", false, "member").await;
    let read_only = user(&pool, "cy", false, "read_only").await;
    let project = ulid::Ulid::new().to_string();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at)
         VALUES(?, 'repo', ?, 'main', ?, ?, ?)",
    )
    .bind(&project)
    .bind(worktree.path().to_string_lossy().to_string())
    .bind(&admin)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    init_repo(worktree.path());

    let pinned = thread(&pool, &project, "Pinned", "idle", 1, now - 300, now - 300).await;
    let running = thread(
        &pool,
        &project,
        "Running",
        "running",
        0,
        now - 200,
        now - 200,
    )
    .await;
    let archived = thread_archived(&pool, &project, "Archived", "idle", now - 100).await;
    sqlx::query(
        "INSERT INTO approvals(id, thread_id, agent, title, kind, options_json, status, created_at)
         VALUES('approval-1', ?, 'codex', 'Run tests', 'command', '[]', 'pending', ?)",
    )
    .bind(&running)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    Fixture {
        app,
        pool,
        _data: data,
        worktree,
        socket,
        host_socket,
        pinned,
        running,
        archived,
        admin,
        member,
        read_only,
    }
}

fn init_repo(path: &Path) {
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(path.join("README.md"), "fixture\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "init"]);
}

async fn user(pool: &SqlitePool, name: &str, admin: bool, access: &str) -> String {
    let id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO users(id, username, display_name, is_admin, access, created_at)
         VALUES(?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(name)
    .bind(name)
    .bind(i64::from(admin))
    .bind(access)
    .bind(now_ms())
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn thread(
    pool: &SqlitePool,
    project: &str,
    title: &str,
    status: &str,
    pinned: i64,
    activity: i64,
    created: i64,
) -> String {
    let id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch,
           worktree_path, created_by, created_at, updated_at, pinned, last_activity_at)
         VALUES(?,?,?,?, 'codex', 'thread/x', 'main', NULL, (SELECT created_by FROM projects WHERE id = ?), ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(project)
    .bind(title)
    .bind(status)
    .bind(project)
    .bind(created)
    .bind(activity)
    .bind(pinned)
    .bind(activity)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn thread_archived(
    pool: &SqlitePool,
    project: &str,
    title: &str,
    status: &str,
    now: i64,
) -> String {
    let id = thread(pool, project, title, status, 0, now, now).await;
    sqlx::query("UPDATE threads SET archived_at = ? WHERE id = ?")
        .bind(now)
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    id
}

impl Fixture {
    async fn token(&self, user: &str) -> String {
        auth_session::create(&self.pool, user, "github", None, None, 3600)
            .await
            .unwrap()
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        user: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let token = self.token(user).await;
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "127.0.0.1:7777")
            .header("origin", "http://127.0.0.1:7777")
            .header("x-requested-with", "threadbase")
            .header("cookie", format!("{}={token}", auth_session::COOKIE));
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        let mut request = request
            .body(Body::from(
                body.map_or_else(String::new, |value| value.to_string()),
            ))
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(SocketAddr::new(
                IpAddr::from([127, 0, 0, 1]),
                1234,
            )));
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }
}

#[tokio::test]
async fn threads_and_tmux_share_one_ordered_list() {
    if which::which("tmux").is_err() {
        return;
    }
    let fixture = fixture(true).await;
    let created = std::process::Command::new("tmux")
        .args([
            "-L",
            &fixture.host_socket,
            "new-session",
            "-d",
            "-s",
            "api",
            "-c",
            fixture.worktree.path().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(created.success());

    // One live workbench shell so the thread row can report it.
    let (status, body) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({ "threadId": fixture.running })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = fixture
        .call(Method::GET, "/api/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["warnings"], json!([]));
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 3, "{body}");
    assert_eq!(sessions[0]["kind"], "thread");
    assert_eq!(sessions[0]["id"], fixture.pinned.as_str());
    assert_eq!(sessions[0]["title"], "Pinned");
    assert_eq!(sessions[0]["projectName"], "repo");
    assert_eq!(sessions[1]["title"], "Running");
    assert_eq!(sessions[1]["pendingApprovals"], 1);
    assert!(sessions[1]["terminal"].is_object(), "{body}");
    assert_eq!(sessions[1]["terminal"]["canDrive"], false);
    assert_eq!(sessions[2]["kind"], "tmux");
    assert_eq!(sessions[2]["socket"], fixture.host_socket.as_str());
    assert_eq!(sessions[2]["name"], "api");
    assert!(sessions
        .iter()
        .all(|session| session["id"] != fixture.archived.as_str()));

    let (status, body) = fixture
        .call(Method::GET, "/api/sessions", &fixture.admin, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let running = body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["title"] == "Running")
        .unwrap();
    assert_eq!(running["terminal"]["canDrive"], true);

    // Read-only accounts see the threads but never the shell metadata.
    let (status, body) = fixture
        .call(Method::GET, "/api/sessions", &fixture.read_only, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|session| session["kind"] == "thread")
        .all(|session| session["terminal"].is_null()));

    let _ = fixture
        .call(
            Method::DELETE,
            &format!("/api/terminal/sessions/t-{}", fixture.running),
            &fixture.admin,
            None,
        )
        .await;
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.host_socket, "kill-server"])
        .status();
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn read_only_accounts_get_threads_but_not_host_shells() {
    let fixture = fixture(true).await;
    let (status, body) = fixture
        .call(Method::GET, "/api/sessions", &fixture.read_only, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|session| session["kind"] == "thread"));
}

#[tokio::test]
async fn host_tmux_is_omitted_when_its_feature_is_off() {
    let fixture = fixture(false).await;
    let (status, body) = fixture
        .call(Method::GET, "/api/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|session| session["kind"] == "thread"));
    // Clean up the private tmux server created by the fixture's terminal test
    // setup, if any.
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn a_tmux_session_can_become_a_thread() {
    if which::which("tmux").is_err() {
        return;
    }
    let fixture = fixture(true).await;
    let created = std::process::Command::new("tmux")
        .args([
            "-L",
            &fixture.host_socket,
            "new-session",
            "-d",
            "-s",
            "api",
            "-c",
            fixture.worktree.path().to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(created.success());

    let (status, body) = fixture
        .call(
            Method::POST,
            &format!("/api/tmux/sessions/{}/%240/thread", fixture.host_socket),
            &fixture.member,
            Some(json!({
                "title": "From tmux",
                "agent": "codex",
                "worktree": false
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let thread_id = body["thread"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["thread"]["title"], "From tmux");
    assert_eq!(body["thread"]["status"], "idle");

    let (status, body) = fixture
        .call(Method::GET, "/api/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tmux = body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["kind"] == "tmux" && session["name"] == "api")
        .unwrap();
    assert_eq!(tmux["projectName"], "repo");
    assert_eq!(tmux["link"]["threadId"], thread_id.as_str());
    assert_eq!(tmux["link"]["threadTitle"], "From tmux");

    // A second click on the same session is rejected rather than creating a
    // duplicate thread.
    let (status, _) = fixture
        .call(
            Method::POST,
            &format!("/api/tmux/sessions/{}/%240/thread", fixture.host_socket),
            &fixture.member,
            Some(json!({ "worktree": false })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.host_socket, "kill-server"])
        .status();
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}
