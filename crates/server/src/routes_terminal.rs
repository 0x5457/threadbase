//! Browser terminal endpoints: sessions, attach tickets and the data socket.
//!
//! Every shell is admin-driven and member-watchable. The POST endpoints exist
//! for the same reason the rest of the API has them: a WebSocket upgrade cannot
//! carry the `X-Requested-With` CSRF header, so a short-lived single-use ticket
//! minted by a CSRF-guarded POST is the credential the socket presents.

use crate::app::AppState;
use crate::auth::{load_user_by_id, CurrentUser};
use crate::authz;
use crate::error::ServerError;
use crate::terminal::{self, Access, Attachment, Role, TerminalError};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::header::ORIGIN;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;
use threadbase_shared::{
    ApiError, AttachTerminalSessionRequest, CreateTerminalSessionRequest, ErrorCode,
    TerminalSessionDto, TerminalSessionResponse, TerminalSessionsResponse, UserId,
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;
/// Coalesce pty writes so a chatty command does not send one frame per chunk.
const FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const FLUSH_BYTES: usize = 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/terminal/sessions",
            get(list_sessions).post(create_session),
        )
        .route(
            "/api/terminal/sessions/{session_id}",
            axum::routing::delete(kill_session),
        )
        .route(
            "/api/terminal/sessions/{session_id}/attach",
            post(attach_ticket),
        )
        .route("/ws/terminal/{session_id}", get(socket))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionsQuery {
    thread_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SocketQuery {
    ticket: String,
}

/// Gate every terminal endpoint: the feature is opt-in, read-only accounts and
/// pending invitations never get a shell. Administrators and logins listed in
/// `THREADBASE_TERMINAL_DRIVERS` drive; other members watch.
fn gate(state: &AppState, user: &CurrentUser) -> Result<Access, ServerError> {
    if !state.config.terminal {
        // 404 rather than 403: a disabled feature should not advertise itself.
        return Err(ServerError(ApiError::not_found("unknown endpoint")));
    }
    let is_driver = terminal::is_driver(&state.config.terminal_drivers, &user.user.username);
    match terminal::access_for(user.user.access, user.is_admin() || is_driver) {
        Access::Denied => Err(ServerError(ApiError::forbidden(
            "the terminal is not available for this account",
        ))),
        allowed => Ok(allowed),
    }
}

fn map_terminal_error(error: TerminalError) -> ServerError {
    match error {
        TerminalError::NotFound => ServerError(ApiError::not_found("terminal session not found")),
        TerminalError::NoWorkspace => {
            ServerError(ApiError::conflict("this thread has no workspace yet"))
        }
        TerminalError::TooManyViewers => ServerError(ApiError::conflict(
            "too many people are watching this terminal",
        )),
        TerminalError::TmuxMissing => ServerError(ApiError::new(
            ErrorCode::AgentUnavailable,
            "tmux was not found on this machine",
        )),
        other => ServerError(ApiError::new(ErrorCode::Internal, other.to_string())),
    }
}

async fn thread_cwd(state: &AppState, thread_id: &str) -> Result<PathBuf, ServerError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT worktree_path FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((worktree,)) = row else {
        return Err(ServerError(ApiError::not_found("thread not found")));
    };
    let Some(worktree) = worktree.filter(|path| !path.trim().is_empty()) else {
        return Err(ServerError(ApiError::conflict(
            "this thread has no workspace yet",
        )));
    };
    let path = PathBuf::from(worktree);
    if !path.is_dir() {
        return Err(ServerError(ApiError::conflict(
            "this thread's workspace directory is gone",
        )));
    }
    Ok(path)
}

fn home_dir() -> Result<PathBuf, ServerError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .ok_or_else(|| ServerError(ApiError::internal("no home directory for the shell")))
}

async fn session_dto(
    state: &AppState,
    id: &str,
    thread_id: Option<String>,
    fallback_cwd: &str,
    alive: bool,
) -> TerminalSessionDto {
    let room = state.terminal.room(id);
    let cwd = match alive {
        true => state
            .terminal
            .current_path(id)
            .await
            .unwrap_or_else(|| fallback_cwd.to_owned()),
        false => fallback_cwd.to_owned(),
    };
    TerminalSessionDto {
        id: id.to_owned(),
        thread_id: thread_id.map(Into::into),
        cwd,
        alive,
        controller: room
            .as_ref()
            .and_then(|room| room.controller())
            .map(UserId::from),
        viewers: room.as_ref().map_or(0, |room| room.viewers()),
    }
}

fn thread_of(id: &str) -> Option<String> {
    id.strip_prefix("t-").map(str::to_owned)
}

/// A home shell belongs to exactly one account. User ids are visible in the UI,
/// so `h-<someone else>` must look like it does not exist.
fn owns_session(user: &CurrentUser, id: &str) -> bool {
    match id.strip_prefix("h-") {
        Some(owner) => owner == user.user.id.as_str(),
        None => true,
    }
}

async fn list_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<SessionsQuery>,
) -> Result<Json<TerminalSessionsResponse>, ServerError> {
    gate(&state, &user)?;
    if let Some(thread_id) = query.thread_id.as_deref() {
        authz::require_read(&state.pool, thread_id, &user).await?;
    }
    let live = state.terminal.sessions().await;
    let mut sessions = Vec::new();
    for id in live {
        if !owns_session(&user, &id) {
            continue;
        }
        let thread_id = thread_of(&id);
        if query
            .thread_id
            .as_deref()
            .is_some_and(|wanted| thread_id.as_deref() != Some(wanted))
        {
            continue;
        }
        sessions.push(session_dto(&state, &id, thread_id, "", true).await);
    }
    Ok(Json(TerminalSessionsResponse { sessions }))
}

async fn create_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateTerminalSessionRequest>,
) -> Result<Json<TerminalSessionResponse>, ServerError> {
    if gate(&state, &user)? != Access::Drive {
        return Err(ServerError(ApiError::forbidden(
            "only administrators can open a terminal",
        )));
    }
    let (id, thread_id, cwd) = match request.thread_id.as_ref() {
        Some(thread_id) => {
            let thread = thread_id.as_str().to_owned();
            authz::require_read(&state.pool, &thread, &user).await?;
            (
                format!("t-{thread}"),
                Some(thread),
                thread_cwd(&state, thread_id.as_str()).await?,
            )
        }
        None => (format!("h-{}", user.user.id.as_str()), None, home_dir()?),
    };
    let cols = request.cols.unwrap_or(DEFAULT_COLS);
    let rows = request.rows.unwrap_or(DEFAULT_ROWS);
    state
        .terminal
        .ensure(&id, thread_id.as_deref(), &cwd, cols, rows)
        .await
        .map_err(map_terminal_error)?;
    let watch_only = request.watch_only.unwrap_or(false);
    let ticket = state
        .terminal
        .ticket(&id, user.user.id.as_str(), watch_only);
    let session = session_dto(&state, &id, thread_id, &cwd.to_string_lossy(), true).await;
    Ok(Json(TerminalSessionResponse { session, ticket }))
}

async fn attach_ticket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
    body: Option<Json<AttachTerminalSessionRequest>>,
) -> Result<Json<TerminalSessionResponse>, ServerError> {
    gate(&state, &user)?;
    if !terminal::is_session_id(&session_id) || !owns_session(&user, &session_id) {
        return Err(ServerError(ApiError::not_found(
            "terminal session not found",
        )));
    }
    if let Some(thread_id) = thread_of(&session_id) {
        authz::require_read(&state.pool, &thread_id, &user).await?;
    }
    let room = match state.terminal.attach_room(&session_id).await {
        Ok(room) => room,
        Err(_) => {
            // A tmux session can outlive the server process; adopt it again
            // from the shell's current directory instead of 404-ing.
            let Some(cwd) = state.terminal.current_path(&session_id).await else {
                return Err(ServerError(ApiError::not_found(
                    "terminal session not found",
                )));
            };
            let thread_id = thread_of(&session_id);
            state
                .terminal
                .adopt(&session_id, thread_id.as_deref(), FsPath::new(&cwd))
        }
    };
    let watch_only = body.and_then(|Json(body)| body.watch_only).unwrap_or(false);
    let ticket = state
        .terminal
        .ticket(&session_id, user.user.id.as_str(), watch_only);
    let session = session_dto(
        &state,
        &session_id,
        thread_of(&session_id),
        &room.cwd.to_string_lossy(),
        true,
    )
    .await;
    Ok(Json(TerminalSessionResponse { session, ticket }))
}

async fn kill_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(session_id): Path<String>,
) -> Result<StatusCode, ServerError> {
    if gate(&state, &user)? != Access::Drive {
        return Err(ServerError(ApiError::forbidden(
            "only administrators can close a terminal",
        )));
    }
    if !terminal::is_session_id(&session_id) || !owns_session(&user, &session_id) {
        return Err(ServerError(ApiError::not_found(
            "terminal session not found",
        )));
    }
    if let Some(thread_id) = thread_of(&session_id) {
        authz::require_read(&state.pool, &thread_id, &user).await?;
    }
    state
        .terminal
        .kill(&session_id)
        .await
        .map_err(map_terminal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    Ready {
        role: &'static str,
        session: String,
        cwd: String,
        controller: Option<String>,
        viewers: u32,
    },
    Output {
        data: String,
    },
    Control {
        controller: Option<String>,
    },
    Exit {
        code: i32,
    },
}

async fn socket(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<SocketQuery>,
    user: CurrentUser,
) -> Response {
    match upgrade(ws, headers, state, session_id, query, user).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn upgrade(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    state: AppState,
    session_id: String,
    query: SocketQuery,
    user: CurrentUser,
) -> Result<Response, ServerError> {
    let _ = gate(&state, &user)?;
    let origin_ok = headers.get(ORIGIN).is_none_or(|origin| {
        origin
            .to_str()
            .ok()
            .is_some_and(|origin| crate::middleware::origin_allowed(&state.config, origin))
    });
    if !origin_ok {
        return Err(ServerError(ApiError::forbidden("origin is not allowed")));
    }
    if !terminal::is_session_id(&session_id) || !owns_session(&user, &session_id) {
        return Err(ServerError(ApiError::not_found(
            "terminal session not found",
        )));
    }
    // The ticket proves this socket was authorised by a CSRF-guarded POST, and
    // carries whether the caller is allowed to type at all.
    let Some(watch_only) =
        state
            .terminal
            .take_ticket(&query.ticket, &session_id, user.user.id.as_str())
    else {
        return Err(ServerError(ApiError::forbidden(
            "terminal ticket is invalid or expired",
        )));
    };
    // A revoked or downgraded account must not ride an old ticket, so the
    // decision is taken from the row as it is now, not from the cookie.
    let fresh = load_user_by_id(&state.pool, user.user.id.as_str())
        .await?
        .ok_or_else(|| ServerError(ApiError::unauthorized("sign in again")))?;
    let access = terminal::access_for(fresh.access, fresh.is_admin);
    if access == Access::Denied {
        return Err(ServerError(ApiError::forbidden(
            "the terminal is not available for this account",
        )));
    }
    if let Some(thread_id) = thread_of(&session_id) {
        authz::require_read(&state.pool, &thread_id, &user).await?;
    }
    let room = state
        .terminal
        .attach_room(&session_id)
        .await
        .map_err(map_terminal_error)?;
    let role = room
        .claim(
            user.user.id.as_str(),
            access == Access::Drive && !watch_only,
        )
        .map_err(map_terminal_error)?;
    let attachment = match terminal::attach(
        &session_id,
        &state.config.terminal_socket,
        &room.cwd,
        DEFAULT_COLS,
        DEFAULT_ROWS,
        role,
    ) {
        Ok(attachment) => attachment,
        Err(error) => {
            room.release(user.user.id.as_str(), role);
            return Err(map_terminal_error(error));
        }
    };
    Ok(ws
        .on_upgrade(move |socket| run(socket, state, room, attachment, role, user))
        .into_response())
}

async fn run(
    socket: WebSocket,
    state: AppState,
    room: std::sync::Arc<terminal::Room>,
    mut attachment: Attachment,
    role: Role,
    user: CurrentUser,
) {
    let user_id = user.user.id.as_str().to_owned();
    let Some(mut exit) = attachment.exit.take() else {
        room.release(&user_id, role);
        return;
    };
    let (mut sender, mut receiver) = socket.split();
    let mut events = room.subscribe();
    let mut output_open = true;

    let ready = ServerMessage::Ready {
        role: role.as_str(),
        session: room.id.clone(),
        cwd: room.cwd.to_string_lossy().into_owned(),
        controller: room.controller(),
        viewers: room.viewers(),
    };
    if send(&mut sender, &ready).await.is_err() {
        room.release(&user_id, role);
        return;
    }

    let mut pending: Vec<u8> = Vec::new();
    let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            chunk = attachment.output.recv(), if output_open => {
                match chunk {
                    Some(bytes) => {
                        pending.extend_from_slice(&bytes);
                        if pending.len() >= FLUSH_BYTES {
                            flush(&mut sender, &mut pending, &attachment).await;
                        }
                    }
                    None => output_open = false,
                }
            }
            code = &mut exit => {
                flush(&mut sender, &mut pending, &attachment).await;
                let code = code.unwrap_or(-1);
                let _ = send(&mut sender, &ServerMessage::Exit { code }).await;
                break;
            }
            event = events.recv() => {
                if let Ok(terminal::RoomEvent::Control(controller)) = event {
                    let _ = send(&mut sender, &ServerMessage::Control { controller }).await;
                }
            }
            incoming = receiver.next() => {
                let Some(Ok(message)) = incoming else { break };
                handle_client(&message, &attachment, role, &state, &room.id).await;
            }
            _ = ticker.tick() => {
                flush(&mut sender, &mut pending, &attachment).await;
            }
        }
    }

    drop(attachment);
    room.release(&user_id, role);
}

/// Handles one client frame. Anything unparseable is ignored on purpose: a
/// terminal socket that closes on a bad frame is worse than dropping it.
async fn handle_client(
    message: &Message,
    attachment: &Attachment,
    role: Role,
    state: &AppState,
    session_id: &str,
) {
    let Message::Text(text) = message else {
        return;
    };
    let Ok(client) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
        return;
    };
    match client {
        ClientMessage::Input { data } => {
            // tmux already drops keystrokes from a read-only client; this keeps
            // the bytes off the wire as well.
            if role == Role::Controller && !data.is_empty() {
                attachment.write(data.into_bytes());
            }
        }
        ClientMessage::Resize { cols, rows } => {
            // Every client sizes its own pty so tmux crops instead of wrapping;
            // only the driver owns the shared window geometry.
            attachment.resize(cols, rows);
            if role != Role::Controller {
                return;
            }
            let state = state.clone();
            let session_id = session_id.to_owned();
            tokio::spawn(async move {
                state.terminal.resize_window(&session_id, cols, rows).await;
            });
        }
    }
}

async fn flush(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    pending: &mut Vec<u8>,
    attachment: &Attachment,
) {
    let dropped = attachment
        .dropped
        .swap(0, std::sync::atomic::Ordering::Relaxed);
    if dropped > 0 {
        pending.extend_from_slice(
            format!("\r\n[threadbase: {dropped} output frames dropped]\r\n").as_bytes(),
        );
    }
    if pending.is_empty() {
        return;
    }
    // Never split a multi-byte character across two frames: keep the trailing
    // incomplete sequence for the next flush and send only the valid prefix.
    let data = match std::str::from_utf8(pending) {
        Ok(text) => {
            let text = text.to_owned();
            pending.clear();
            text
        }
        Err(error) => {
            let valid = error.valid_up_to();
            if valid == 0 {
                return;
            }
            let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
            pending.drain(..valid);
            text
        }
    };
    let _ = send(sender, &ServerMessage::Output { data }).await;
}

async fn send(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &ServerMessage,
) -> Result<(), ()> {
    let Ok(text) = serde_json::to_string(message) else {
        return Err(());
    };
    tokio::time::timeout(
        Duration::from_secs(5),
        sender.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}
