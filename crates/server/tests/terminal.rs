//! Integration tests for the browser terminal: the opt-in gate, who may drive
//! an existing shell, and one end-to-end tmux round trip.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tempfile::TempDir;
use threadbase_server::app::AppState;
use threadbase_server::auth_session;
use threadbase_server::config::Config;
use threadbase_server::{db, routes};
use threadbase_shared::now_ms;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tower::ServiceExt;

struct Fixture {
    socket: String,
    app: axum::Router,
    addr: String,
    pool: SqlitePool,
    _data: TempDir,
    /// The project checkout, shared by threads created without a worktree.
    project: TempDir,
    worktree: TempDir,
    thread_id: String,
    /// A thread created with `worktree: false`: its shell opens at the project.
    bare_thread_id: String,
    admin: String,
    member: String,
    read_only: String,
    /// Optional host tmux server, separate from the workbench socket.
    host_socket: Option<String>,
}

async fn fixture(terminal: bool) -> Fixture {
    fixture_with_host(terminal, None).await
}

async fn fixture_with_host(terminal: bool, host_socket: Option<&str>) -> Fixture {
    let project = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let pool = db::connect_memory().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket = format!("tb-test-{}", ulid::Ulid::new());
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        db_path: dir.path().join("threadbase.db"),
        bind: "127.0.0.1:0".parse().unwrap(),
        terminal,
        terminal_host: host_socket.is_some(),
        terminal_host_sockets: host_socket.map(str::to_owned).into_iter().collect(),
        terminal_shell: "/bin/bash".to_owned(),
        terminal_socket: socket.clone(),
        ..Config::default()
    };
    let state = AppState::new(config, pool.clone());
    let app = routes::router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let served = app.clone();
    tokio::spawn(async move {
        axum::serve(
            listener,
            served.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let admin = user(&pool, "ada", true, "member").await;
    let member = user(&pool, "bo", false, "member").await;
    let read_only = user(&pool, "cy", false, "read_only").await;

    let project_id = ulid::Ulid::new().to_string();
    let thread_id = ulid::Ulid::new().to_string();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at)
         VALUES(?, 'repo', ?, 'main', ?, ?, ?)",
    )
    .bind(&project_id)
    .bind(project.path().to_string_lossy().to_string())
    .bind(&admin)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, worktree_path, created_by, created_at, updated_at)
         VALUES(?, ?, 'Terminal', 'idle', 'codex', 'thread/terminal', 'main', ?, ?, ?, ?)",
    )
    .bind(&thread_id)
    .bind(&project_id)
    .bind(worktree.path().to_string_lossy().to_string())
    .bind(&admin)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Same project, no worktree: `worktree: false` threads run the agent at the
    // project checkout, so their shell has to start there too.
    let bare_thread_id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, worktree_path, created_by, created_at, updated_at)
         VALUES(?, ?, 'Shared checkout', 'idle', 'codex', 'main', 'main', NULL, ?, ?, ?)",
    )
    .bind(&bare_thread_id)
    .bind(&project_id)
    .bind(&admin)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    Fixture {
        socket,
        app,
        addr,
        pool,
        _data: dir,
        project,
        worktree,
        thread_id,
        bare_thread_id,
        admin,
        member,
        read_only,
        host_socket: host_socket.map(str::to_owned),
    }
}

impl Fixture {
    async fn token(&self, user: &str) -> String {
        auth_session::create(&self.pool, user, "tailscale", None, None, 3600)
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

    async fn connect(
        &self,
        user: &str,
        ticket: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let token = self.token(user).await;
        let mut request = format!(
            "ws://{}/ws/terminal/t-{}?ticket={ticket}",
            self.addr, self.thread_id
        )
        .into_client_request()
        .unwrap();
        request.headers_mut().insert(
            "cookie",
            format!("{}={token}", auth_session::COOKIE).parse().unwrap(),
        );
        connect_async(request).await.unwrap().0
    }

    async fn connect_host(
        &self,
        user: &str,
        ticket: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let token = self.token(user).await;
        let mut request = format!("ws://{}/ws/tmux?ticket={ticket}", self.addr)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "cookie",
            format!("{}={token}", auth_session::COOKIE).parse().unwrap(),
        );
        connect_async(request).await.unwrap().0
    }
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

async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("timed out waiting for the terminal socket")
            .unwrap()
            .unwrap();
        match message {
            WsMessage::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
            WsMessage::Ping(_) | WsMessage::Pong(_) => {}
            other => panic!("unexpected terminal message: {other:?}"),
        }
    }
}

fn tmux_available() -> bool {
    which::which("tmux").is_ok()
}

#[tokio::test]
async fn the_terminal_is_hidden_unless_enabled() {
    let fixture = fixture(false).await;
    let (status, _) = fixture
        .call(Method::GET, "/api/terminal/sessions", &fixture.admin, None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({ "threadId": fixture.thread_id })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn read_only_accounts_never_reach_a_shell() {
    let fixture = fixture(true).await;
    let (status, _) = fixture
        .call(
            Method::GET,
            "/api/terminal/sessions",
            &fixture.read_only,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.read_only,
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_home_shell_belongs_to_one_account() {
    if !tmux_available() {
        return;
    }
    let fixture = fixture(true).await;
    let home = format!("h-{}", fixture.admin);
    let (status, body) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["session"]["id"], home);

    // A member who knows the id still gets a 404, and the id never shows up in
    // their session list.
    let (status, _) = fixture
        .call(
            Method::POST,
            &format!("/api/terminal/sessions/{home}/attach"),
            &fixture.member,
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = fixture
        .call(Method::GET, "/api/terminal/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|session| session["id"] != home.as_str()));
    let _ = fixture
        .call(
            Method::DELETE,
            &format!("/api/terminal/sessions/{home}"),
            &fixture.admin,
            None,
        )
        .await;
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn a_watch_only_ticket_never_takes_the_keyboard() {
    if !tmux_available() {
        return;
    }
    let fixture = fixture(true).await;
    let session = format!("t-{}", fixture.thread_id);
    let (status, body) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({ "threadId": fixture.thread_id, "watchOnly": true })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ticket = body["ticket"].as_str().unwrap().to_owned();

    let mut socket = fixture.connect(&fixture.admin, &ticket).await;
    let ready = next_json(&mut socket).await;
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["role"], "watcher");

    let _ = fixture
        .call(
            Method::DELETE,
            &format!("/api/terminal/sessions/{session}"),
            &fixture.admin,
            None,
        )
        .await;
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn members_may_watch_but_not_open_a_shell() {
    let fixture = fixture(true).await;
    let (status, body) = fixture
        .call(
            Method::GET,
            &format!("/api/terminal/sessions?threadId={}", fixture.thread_id),
            &fixture.member,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sessions"].as_array().unwrap().len(), 0);

    let (status, _) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.member,
            Some(json!({ "threadId": fixture.thread_id })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_thread_without_a_worktree_opens_at_the_project_checkout() {
    if !tmux_available() {
        return;
    }
    let fixture = fixture(true).await;
    let session = format!("t-{}", fixture.bare_thread_id);
    let (status, body) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({ "threadId": fixture.bare_thread_id })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["session"]["id"], session);
    assert_eq!(
        body["session"]["cwd"],
        fixture.project.path().to_string_lossy().as_ref()
    );
    let _ = fixture
        .call(
            Method::DELETE,
            &format!("/api/terminal/sessions/{session}"),
            &fixture.admin,
            None,
        )
        .await;
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn every_keystroke_reaches_the_shell() {
    if !tmux_available() {
        return;
    }
    let fixture = fixture(true).await;
    let session = format!("t-{}", fixture.thread_id);
    let (status, body) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({ "threadId": fixture.thread_id })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ticket = body["ticket"].as_str().unwrap().to_owned();
    let mut admin = fixture.connect(&fixture.admin, &ticket).await;
    let ready = next_json(&mut admin).await;
    assert_eq!(ready["type"], "ready");
    admin
        .send(WsMessage::Text(
            json!({ "type": "resize", "cols": 100, "rows": 30 })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    for ch in "echo key-by-key\r".chars() {
        admin
            .send(WsMessage::Text(
                json!({ "type": "input", "data": ch.to_string() })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut echoed = String::new();
    for _ in 0..80 {
        let message = next_json(&mut admin).await;
        if message["type"] == "output" {
            echoed.push_str(message["data"].as_str().unwrap_or_default());
        }
        if echoed.matches("key-by-key").count() >= 2 {
            break;
        }
    }
    assert!(
        echoed.matches("key-by-key").count() >= 2,
        "keystrokes did not all arrive: {echoed:?}"
    );
    let _ = fixture
        .call(
            Method::DELETE,
            &format!("/api/terminal/sessions/{session}"),
            &fixture.admin,
            None,
        )
        .await;
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn an_admin_drives_and_a_member_watches() {
    if !tmux_available() {
        return;
    }
    let fixture = fixture(true).await;
    let session = format!("t-{}", fixture.thread_id);

    let (status, body) = fixture
        .call(
            Method::POST,
            "/api/terminal/sessions",
            &fixture.admin,
            Some(json!({ "threadId": fixture.thread_id, "cols": 100, "rows": 30 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["session"]["id"], session);
    assert_eq!(
        body["session"]["cwd"],
        fixture.worktree.path().to_string_lossy().as_ref()
    );
    let admin_ticket = body["ticket"].as_str().unwrap().to_owned();

    let mut admin = fixture.connect(&fixture.admin, &admin_ticket).await;
    let ready = next_json(&mut admin).await;
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["role"], "controller");

    // A ticket is single use: replaying it cannot open a second socket.
    let token = fixture.token(&fixture.admin).await;
    let mut request = format!(
        "ws://{}/ws/terminal/{session}?ticket={admin_ticket}",
        fixture.addr
    )
    .into_client_request()
    .unwrap();
    request.headers_mut().insert(
        "cookie",
        format!("{}={token}", auth_session::COOKIE).parse().unwrap(),
    );
    let replay = connect_async(request).await;
    assert!(
        replay.is_err() || !matches!(replay.unwrap().0.next().await, Some(Ok(WsMessage::Text(_))))
    );

    admin
        .send(WsMessage::Text(
            json!({ "type": "resize", "cols": 100, "rows": 30 })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    admin
        .send(WsMessage::Text(
            json!({ "type": "input", "data": "echo threadbase-terminal\r" })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let mut echoed = String::new();
    for _ in 0..40 {
        let message = next_json(&mut admin).await;
        if message["type"] == "output" {
            echoed.push_str(message["data"].as_str().unwrap_or_default());
        }
        if echoed.contains("threadbase-terminal")
            && echoed.matches("threadbase-terminal").count() > 1
        {
            break;
        }
    }
    assert!(
        echoed.contains("threadbase-terminal"),
        "no echo: {echoed:?}"
    );

    // A member attaches read-only to the shell that already exists.
    let (status, body) = fixture
        .call(
            Method::POST,
            &format!("/api/terminal/sessions/{session}/attach"),
            &fixture.member,
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let member_ticket = body["ticket"].as_str().unwrap().to_owned();
    let mut watcher = fixture.connect(&fixture.member, &member_ticket).await;
    let ready = next_json(&mut watcher).await;
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["role"], "watcher");
    assert_eq!(ready["controller"], fixture.admin);

    // Closing the shell from an admin removes the tmux session.
    let (status, _) = fixture
        .call(
            Method::DELETE,
            &format!("/api/terminal/sessions/{session}"),
            &fixture.admin,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = fixture
        .call(
            Method::GET,
            &format!("/api/terminal/sessions?threadId={}", fixture.thread_id),
            &fixture.admin,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sessions"].as_array().unwrap().len(), 0);

    // The test socket goes away with the test.
    let _ = std::process::Command::new("tmux")
        .args(["-L", &fixture.socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn host_tmux_is_hidden_unless_its_opt_in_is_on() {
    let fixture = fixture(true).await;
    assert!(fixture.host_socket.is_none());
    let (status, _) = fixture
        .call(Method::GET, "/api/tmux/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn read_only_accounts_cannot_see_host_tmux_sessions() {
    let socket = format!("tb-host-test-{}", ulid::Ulid::new());
    let fixture = fixture_with_host(true, Some(&socket)).await;
    assert_eq!(fixture.host_socket.as_deref(), Some(socket.as_str()));
    let (status, _) = fixture
        .call(Method::GET, "/api/tmux/sessions", &fixture.read_only, None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn members_list_and_drive_host_tmux_sessions() {
    if !tmux_available() {
        return;
    }
    let socket = format!("tb-host-test-{}", ulid::Ulid::new());
    let fixture = fixture_with_host(true, Some(&socket)).await;
    let project = fixture.project.path().to_string_lossy().into_owned();
    let created = std::process::Command::new("tmux")
        .args([
            "-L",
            &socket,
            "new-session",
            "-d",
            "-s",
            "work",
            "-c",
            &project,
        ])
        .status()
        .unwrap();
    assert!(created.success());

    let (status, body) = fixture
        .call(Method::GET, "/api/tmux/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let server = &body["servers"][0];
    assert_eq!(server["socket"], socket.as_str());
    let session = &server["sessions"][0];
    let id = session["id"].as_str().unwrap().to_owned();
    assert!(id.starts_with('$'), "{id}");
    assert_eq!(session["name"], "work");
    assert_eq!(session["cwd"], project.as_str());

    // Every member may drive a host session, but only one browser holds the
    // keyboard at a time; a watcher can take over explicitly.
    let (status, body) = fixture
        .call(
            Method::POST,
            &format!("/api/tmux/sessions/{socket}/{id}/attach"),
            &fixture.member,
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ticket = body["ticket"].as_str().unwrap().to_owned();
    let mut member = fixture.connect_host(&fixture.member, &ticket).await;
    let ready = next_json(&mut member).await;
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["role"], "controller");
    assert_eq!(ready["canTakeOver"], true);
    assert_eq!(ready["viewerUsers"].as_array().unwrap().len(), 1);

    for ch in "echo host-round-trip\r".chars() {
        member
            .send(WsMessage::Text(
                json!({ "type": "input", "data": ch.to_string() })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut echoed = String::new();
    for _ in 0..80 {
        let message = next_json(&mut member).await;
        if message["type"] == "output" {
            echoed.push_str(message["data"].as_str().unwrap_or_default());
        }
        if echoed.matches("host-round-trip").count() >= 2 {
            break;
        }
    }
    assert!(
        echoed.matches("host-round-trip").count() >= 2,
        "host keystrokes did not arrive: {echoed:?}"
    );

    let (status, body) = fixture
        .call(
            Method::POST,
            &format!("/api/tmux/sessions/{socket}/{id}/attach"),
            &fixture.admin,
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let admin_ticket = body["ticket"].as_str().unwrap().to_owned();
    let mut admin = fixture.connect_host(&fixture.admin, &admin_ticket).await;
    let ready = next_json(&mut admin).await;
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["role"], "watcher");
    assert_eq!(ready["canTakeOver"], true);
    assert_eq!(ready["viewerUsers"].as_array().unwrap().len(), 2);

    let mut viewer_event = Value::Null;
    for _ in 0..20 {
        let message = next_json(&mut member).await;
        if message["type"] == "viewers" {
            viewer_event = message;
            break;
        }
    }
    assert_eq!(
        viewer_event["viewerUsers"].as_array().unwrap().len(),
        2,
        "{viewer_event}"
    );

    admin
        .send(WsMessage::Text(
            json!({ "type": "take" }).to_string().into(),
        ))
        .await
        .unwrap();
    let mut admin_control = Value::Null;
    for _ in 0..30 {
        let message = next_json(&mut admin).await;
        if message["type"] == "control" {
            admin_control = message;
            break;
        }
    }
    assert_eq!(admin_control["role"], "controller", "{admin_control}");
    let mut member_control = Value::Null;
    for _ in 0..30 {
        let message = next_json(&mut member).await;
        if message["type"] == "control" {
            member_control = message;
            break;
        }
    }
    assert_eq!(member_control["role"], "watcher", "{member_control}");
    assert_eq!(member_control["controller"], fixture.admin.as_str());

    // The previous driver's bytes are dropped by the controller check, not
    // merely hidden by the UI.
    for ch in "echo old-driver\r".chars() {
        member
            .send(WsMessage::Text(
                json!({ "type": "input", "data": ch.to_string() })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    for ch in "echo new-driver\r".chars() {
        admin
            .send(WsMessage::Text(
                json!({ "type": "input", "data": ch.to_string() })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut echoed = String::new();
    for _ in 0..80 {
        let message = next_json(&mut admin).await;
        if message["type"] == "output" {
            echoed.push_str(message["data"].as_str().unwrap_or_default());
        }
        if echoed.matches("new-driver").count() >= 2 {
            break;
        }
    }
    assert!(
        echoed.matches("new-driver").count() >= 2,
        "new driver keystrokes did not arrive: {echoed:?}"
    );
    assert!(
        !echoed.contains("old-driver"),
        "the previous driver's input leaked: {echoed:?}"
    );

    drop(admin);
    drop(member);
    let _ = std::process::Command::new("tmux")
        .args(["-L", &socket, "kill-server"])
        .status();
}

#[tokio::test]
async fn a_missing_host_server_is_an_empty_group() {
    let socket = format!("tb-host-empty-{}", ulid::Ulid::new());
    let fixture = fixture_with_host(true, Some(&socket)).await;
    let (status, body) = fixture
        .call(Method::GET, "/api/tmux/sessions", &fixture.member, None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let server = &body["servers"][0];
    assert_eq!(server["socket"], socket.as_str());
    assert_eq!(server["sessions"].as_array().unwrap().len(), 0);
    assert!(server["error"].is_null());
}

#[tokio::test]
async fn host_attach_rejects_sockets_outside_the_allowlist() {
    let socket = format!("tb-host-test-{}", ulid::Ulid::new());
    let fixture = fixture_with_host(true, Some(&socket)).await;
    for target in ["other", fixture.socket.as_str()] {
        let (status, _) = fixture
            .call(
                Method::POST,
                &format!("/api/tmux/sessions/{target}/$0/attach"),
                &fixture.member,
                Some(json!({})),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "socket {target}");
    }
}
