//! Live WebSocket tests: event replay, fan-out, presence and draft sync.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::Duration;
use tempfile::TempDir;
use threadbase_draft::DraftDoc;
use threadbase_server::app::AppState;
use threadbase_server::config::Config;
use threadbase_server::{db, routes};
use threadbase_shared::{now_ms, EventEnvelope, ThreadEvent, ThreadId};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use y_sync::sync::{Message as SyncEnvelope, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

struct Server {
    addr: String,
    token: String,
    state: AppState,
    user_id: String,
    project_id: String,
    thread_id: String,
    _dir: TempDir,
}

async fn start() -> Server {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: dir.path().to_path_buf(),
        db_path: dir.path().join("threadbase.db"),
        bind: "127.0.0.1:0".parse().unwrap(),
        ..Config::default()
    };
    let pool = db::connect_memory().await.unwrap();
    let state = AppState::new(config, pool);
    let app = routes::router(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let user = threadbase_server::auth::ensure_identity_user(
        &state.pool,
        &threadbase_server::auth::Identity::tailscale("ada@example.com", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let user_id = user.id.to_string();
    let token = threadbase_server::auth_session::create(
        &state.pool,
        &user_id,
        "tailscale",
        None,
        None,
        3600,
    )
    .await
    .unwrap();
    let project_id = ulid::Ulid::new().to_string();
    let thread_id = ulid::Ulid::new().to_string();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at)
         VALUES(?, 'repo', ?, 'main', ?, ?, ?)",
    )
    .bind(&project_id)
    .bind(dir.path().join("repo").to_string_lossy().to_string())
    .bind(&user_id)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, created_by, created_at, updated_at)
         VALUES(?, ?, 'Test', 'idle', 'codex', 'thread/test', 'main', ?, ?, ?)",
    )
    .bind(&thread_id)
    .bind(&project_id)
    .bind(&user_id)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await
    .unwrap();
    Server {
        addr: addr.to_string(),
        token,
        state,
        user_id,
        project_id,
        thread_id,
        _dir: dir,
    }
}

async fn connect(
    server: &Server,
    path: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!("ws://{}{path}", server.addr)
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "cookie",
        format!("threadbase_session={}", server.token)
            .parse()
            .unwrap(),
    );
    connect_async(request).await.unwrap().0
}

async fn next_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match message {
            WsMessage::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected websocket message: {other:?}"),
        }
    }
}

async fn next_binary(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Vec<u8> {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match message {
            WsMessage::Binary(bytes) => return bytes.to_vec(),
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("unexpected websocket message: {other:?}"),
        }
    }
}

async fn append_notice(server: &Server, label: &str) -> EventEnvelope {
    server
        .state
        .events
        .append(
            &ThreadId::from(server.thread_id.clone()),
            threadbase_shared::Actor::system(),
            ThreadEvent::SystemNotice {
                level: "info".into(),
                message: label.into(),
            },
            Default::default(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn subscribes_replays_and_receives_live_events() {
    let server = start().await;
    let mut socket = connect(&server, "/ws").await;
    let inbox = next_json(&mut socket).await;
    assert_eq!(inbox["type"], "inbox_count");

    socket
        .send(WsMessage::Text(
            json!({"type":"subscribe","threadId":server.thread_id,"afterSeq":0})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let page = next_json(&mut socket).await;
    assert_eq!(page["type"], "events_page");
    assert_eq!(page["page"]["events"].as_array().unwrap().len(), 0);
    let state = next_json(&mut socket).await;
    assert_eq!(state["type"], "thread_state");

    let event = append_notice(&server, "live").await;
    let message = next_json(&mut socket).await;
    assert_eq!(message["type"], "event");
    assert_eq!(message["event"]["seq"], event.seq);
    assert_eq!(message["event"]["payload"]["message"], "live");
}

#[tokio::test]
async fn reconnect_with_after_seq_replays_missed_events() {
    let server = start().await;
    let first = append_notice(&server, "one").await;
    let mut socket = connect(&server, "/ws").await;
    let _ = next_json(&mut socket).await;
    socket
        .send(WsMessage::Text(
            json!({"type":"subscribe","threadId":server.thread_id,"afterSeq":0})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let page = next_json(&mut socket).await;
    assert_eq!(page["page"]["events"][0]["seq"], first.seq);
    drop(socket);

    let second = append_notice(&server, "two").await;
    let mut socket = connect(&server, "/ws").await;
    let _ = next_json(&mut socket).await;
    socket
        .send(WsMessage::Text(
            json!({"type":"subscribe","threadId":server.thread_id,"afterSeq":first.seq})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let page = next_json(&mut socket).await;
    assert_eq!(page["page"]["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["page"]["events"][0]["seq"], second.seq);
}

#[tokio::test]
async fn presence_is_broadcast_with_semantic_anchor() {
    let server = start().await;
    let mut socket = connect(&server, "/ws").await;
    let _ = next_json(&mut socket).await;
    socket
        .send(WsMessage::Text(
            json!({
                "type":"presence",
                "projectId": server.project_id,
                "target": {"kind":"message","messageId":"m1"},
                "threadId": server.thread_id,
                "panel": "transcript",
                "typing": true,
                "hidden": false
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let message = next_json(&mut socket).await;
    assert_eq!(message["type"], "presence");
    assert_eq!(message["onlineUserIds"][0], server.user_id);
    assert_eq!(message["focuses"][0]["typing"], true);
    assert_eq!(message["focuses"][0]["target"]["kind"], "message");
}

#[tokio::test]
async fn draft_websocket_syncs_updates() {
    let server = start().await;
    let mut socket = connect(&server, &format!("/ws/drafts/{}", server.thread_id)).await;
    let initial = next_binary(&mut socket).await;
    assert!(!initial.is_empty());

    let mut client = DraftDoc::new();
    threadbase_draft::seed_text(&mut client, "u1", &["shared text"]).unwrap();
    let update = SyncEnvelope::Sync(SyncMessage::Update(client.encode_full())).encode_v1();
    socket.send(WsMessage::Binary(update.into())).await.unwrap();
    let broadcast = next_binary(&mut socket).await;
    let mut decoder =
        yrs::updates::decoder::DecoderV1::new(yrs::encoding::read::Cursor::new(&broadcast));
    let message = SyncEnvelope::decode(&mut decoder).unwrap();
    let SyncEnvelope::Sync(SyncMessage::Update(update)) = message else {
        panic!("expected update");
    };
    let mut server_doc = DraftDoc::new();
    server_doc.apply_update(&update).unwrap();
    assert_eq!(server_doc.text(), "shared text");
}

#[tokio::test]
async fn heartbeat_pings_idle_connections() {
    let server = start().await;
    let mut socket = connect(&server, "/ws").await;
    let _ = next_json(&mut socket).await;
    let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(message, WsMessage::Ping(_)));
}

#[tokio::test]
async fn a_new_notification_pushes_the_owner_unread_count() {
    let server = start().await;
    let mut socket = connect(&server, "/ws").await;
    let initial = next_json(&mut socket).await;
    assert_eq!(initial["type"], "inbox_count");
    assert_eq!(initial["unread"], 0);
    threadbase_server::notify::notify_user(
        &server.state.pool,
        &server.state.projection,
        threadbase_server::notify::InboxItem {
            user_id: &server.user_id,
            kind: "mention",
            thread_id: &server.thread_id,
            actor_id: None,
            approval_id: None,
            elicitation_id: None,
            comment_id: None,
            summary: "Ada mentioned you",
        },
    )
    .await
    .unwrap();
    let pushed = next_json(&mut socket).await;
    assert_eq!(pushed["type"], "inbox_count");
    assert_eq!(pushed["unread"], 1);
    assert_eq!(pushed["userId"], server.user_id);
}
