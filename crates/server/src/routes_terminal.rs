//! Terminal endpoints: sessions, attach tickets and the data socket.
//!
//! Workbench shells are admin-driven and member-watchable. Host tmux sessions
//! are a separate opt-in. A WebSocket upgrade cannot send the CSRF header, so the
//! socket authenticates with a single-use ticket from a CSRF-guarded POST.

use crate::app::AppState;
use crate::auth::{load_user_briefs, load_user_by_id, CurrentUser};
use crate::authz;
use crate::error::ServerError;
use crate::terminal::{self, Access, Attachment, Role, SessionRef, TerminalError, TicketGrant};
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
    HostTerminalAttachResponse, HostTerminalServerDto, HostTerminalSessionDto,
    HostTerminalSessionsResponse, TerminalSessionDto, TerminalSessionResponse,
    TerminalSessionsResponse, UserAccess, UserBrief, UserId,
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;
/// Coalesce pty writes so a chatty command does not send one frame per chunk.
const FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const FLUSH_BYTES: usize = 64 * 1024;
/// A socket quiet this long went away without a close frame (suspended tab,
/// dead network); drop it so its room slot and pty can be reused.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

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
        .route("/api/tmux/sessions", get(list_host_sessions))
        .route(
            "/api/tmux/sessions/{socket}/{session_id}/attach",
            post(attach_host_session),
        )
        .route("/ws/tmux", get(host_socket))
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

/// Host tmux is a second opt-in: every member gets a writable shell as the
/// service account, so it is not implied by `THREADBASE_TERMINAL=1`.
pub(crate) fn host_gate(state: &AppState, user: &CurrentUser) -> Result<(), ServerError> {
    if !state.config.terminal || !state.config.terminal_host {
        return Err(ServerError(ApiError::not_found("unknown endpoint")));
    }
    if user.is_admin() || matches!(user.user.access, UserAccess::Member) {
        Ok(())
    } else {
        Err(ServerError(ApiError::forbidden(
            "the terminal is not available for this account",
        )))
    }
}

fn origin_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    headers.get(ORIGIN).is_none_or(|origin| {
        origin
            .to_str()
            .ok()
            .is_some_and(|origin| crate::middleware::origin_allowed(&state.config, origin))
    })
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

/// The directory a thread's shell starts in. It follows the agent: a thread
/// with its own worktree opens there, and a thread created without one
/// (`worktree: false`) opens at the project checkout the agent also runs in.
async fn thread_cwd(state: &AppState, thread_id: &str) -> Result<PathBuf, ServerError> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT t.worktree_path, p.path
           FROM threads t
           JOIN projects p ON p.id = t.project_id
          WHERE t.id = ?",
    )
    .bind(thread_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((worktree, project)) = row else {
        return Err(ServerError(ApiError::not_found("thread not found")));
    };
    // Deliberately no fallback past this point: a worktree that went missing is
    // an error, not an invitation to type into the shared checkout.
    let cwd = worktree
        .filter(|path| !path.trim().is_empty())
        .unwrap_or(project);
    let path = PathBuf::from(cwd);
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
            .current_path(&SessionRef::workbench(id))
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

fn host_session_dto(
    state: &AppState,
    socket: &str,
    session: terminal::HostSession,
) -> HostTerminalSessionDto {
    let key = SessionRef::host(socket, &session.id).key();
    let viewers = state.terminal.room(&key).map_or(0, |room| room.viewers());
    HostTerminalSessionDto {
        id: session.id,
        name: session.name,
        windows: session.windows,
        attached: session.attached,
        created_at: session.created_at,
        activity_at: session.activity_at,
        cwd: session.cwd,
        command: session.command,
        viewers,
    }
}

pub(crate) fn host_socket_allowed(state: &AppState, socket: &str) -> bool {
    state
        .config
        .terminal_host_sockets
        .iter()
        .any(|allowed| allowed == socket)
        && socket != state.config.terminal_socket
        && terminal::is_socket_name(socket)
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
    let ticket = state.terminal.ticket(
        SessionRef::workbench(&id),
        user.user.id.as_str(),
        watch_only,
    );
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
    let target = SessionRef::workbench(&session_id);
    let room = match state.terminal.attach_room(&target).await {
        Ok(room) => room,
        Err(_) => {
            // A tmux session can outlive the server process; adopt it again
            // from the shell's current directory instead of 404-ing.
            let Some(cwd) = state.terminal.current_path(&target).await else {
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
    let ticket = state.terminal.ticket(
        SessionRef::workbench(&session_id),
        user.user.id.as_str(),
        watch_only,
    );
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
        .kill(&SessionRef::workbench(&session_id))
        .await
        .map_err(map_terminal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Ask for the keyboard in a host room. Workbench rooms ignore it.
    Take,
    /// Liveness probe; the browser reconnects when the answer stops arriving.
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    #[serde(rename_all = "camelCase")]
    Ready {
        role: &'static str,
        session: String,
        cwd: String,
        controller: Option<String>,
        viewers: u32,
        /// Visible viewer profiles; hidden-presence accounts are omitted.
        viewer_users: Vec<UserBrief>,
        /// True when this socket may send `take`.
        can_take_over: bool,
    },
    Output {
        data: String,
    },
    Control {
        controller: Option<String>,
        /// Present for takeover rooms; the receiver adopts this role.
        role: Option<&'static str>,
    },
    #[serde(rename_all = "camelCase")]
    Viewers {
        viewer_users: Vec<UserBrief>,
    },
    Exit {
        code: i32,
    },
    Pong,
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
    if !origin_allowed(&state, &headers) {
        return Err(ServerError(ApiError::forbidden("origin is not allowed")));
    }
    if !terminal::is_session_id(&session_id) || !owns_session(&user, &session_id) {
        return Err(ServerError(ApiError::not_found(
            "terminal session not found",
        )));
    }
    let target = SessionRef::workbench(&session_id);
    // The ticket proves this socket was authorised by a CSRF-guarded POST, and
    // carries whether the caller is allowed to type at all.
    let Some(TicketGrant {
        target: granted,
        watch_only,
    }) = state
        .terminal
        .take_ticket(&query.ticket, user.user.id.as_str())
    else {
        return Err(ServerError(ApiError::forbidden(
            "terminal ticket is invalid or expired",
        )));
    };
    if granted != target {
        return Err(ServerError(ApiError::forbidden(
            "terminal ticket is invalid or expired",
        )));
    }
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
        .attach_room(&target)
        .await
        .map_err(map_terminal_error)?;
    let role = room
        .claim(
            user.user.id.as_str(),
            access == Access::Drive && !watch_only,
        )
        .map_err(map_terminal_error)?;
    let attachment = match terminal::attach(&room, DEFAULT_COLS, DEFAULT_ROWS, role) {
        Ok(attachment) => attachment,
        Err(error) => {
            room.release(user.user.id.as_str());
            return Err(map_terminal_error(error));
        }
    };
    Ok(ws
        .on_upgrade(move |socket| run(socket, state, room, attachment, role, false, user))
        .into_response())
}

async fn list_host_sessions(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<HostTerminalSessionsResponse>, ServerError> {
    host_gate(&state, &user)?;
    let servers = state
        .terminal
        .host_sessions(&state.config.terminal_host_sockets)
        .await;
    let servers = servers
        .into_iter()
        .map(|server| {
            let sessions = server
                .sessions
                .into_iter()
                .map(|session| host_session_dto(&state, &server.socket, session))
                .collect();
            HostTerminalServerDto {
                socket: server.socket,
                sessions,
                error: server.error,
            }
        })
        .collect();
    Ok(Json(HostTerminalSessionsResponse { servers }))
}

async fn attach_host_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((socket, session_id)): Path<(String, String)>,
    body: Option<Json<AttachTerminalSessionRequest>>,
) -> Result<Json<HostTerminalAttachResponse>, ServerError> {
    host_gate(&state, &user)?;
    if !host_socket_allowed(&state, &socket) || !terminal::is_host_session_id(&session_id) {
        return Err(ServerError(ApiError::not_found("tmux session not found")));
    }
    state
        .terminal
        .ensure_host(&socket, &session_id)
        .await
        .map_err(map_terminal_error)?;
    let Some(session) = state.terminal.host_session(&socket, &session_id).await else {
        return Err(ServerError(ApiError::not_found("tmux session not found")));
    };
    let watch_only = body.and_then(|Json(body)| body.watch_only).unwrap_or(false);
    let ticket = state.terminal.ticket(
        SessionRef::host(&socket, &session_id),
        user.user.id.as_str(),
        watch_only,
    );
    let session = host_session_dto(&state, &socket, session);
    Ok(Json(HostTerminalAttachResponse { session, ticket }))
}

async fn host_socket(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<SocketQuery>,
    user: CurrentUser,
) -> Response {
    match host_upgrade(ws, headers, state, query, user).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn host_upgrade(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    state: AppState,
    query: SocketQuery,
    user: CurrentUser,
) -> Result<Response, ServerError> {
    host_gate(&state, &user)?;
    if !origin_allowed(&state, &headers) {
        return Err(ServerError(ApiError::forbidden("origin is not allowed")));
    }
    let Some(TicketGrant { target, watch_only }) = state
        .terminal
        .take_ticket(&query.ticket, user.user.id.as_str())
    else {
        return Err(ServerError(ApiError::forbidden(
            "terminal ticket is invalid or expired",
        )));
    };
    let SessionRef::Host { socket, id } = target else {
        return Err(ServerError(ApiError::forbidden(
            "terminal ticket is invalid or expired",
        )));
    };
    if !host_socket_allowed(&state, &socket) || !terminal::is_host_session_id(&id) {
        return Err(ServerError(ApiError::not_found("tmux session not found")));
    }
    // The host gate is repeated against the current row: an account downgraded
    // after the ticket was minted must not attach.
    let fresh = load_user_by_id(&state.pool, user.user.id.as_str())
        .await?
        .ok_or_else(|| ServerError(ApiError::unauthorized("sign in again")))?;
    if !(fresh.is_admin || matches!(fresh.access, UserAccess::Member)) {
        return Err(ServerError(ApiError::forbidden(
            "the terminal is not available for this account",
        )));
    }
    let target = SessionRef::host(&socket, &id);
    let room = match state.terminal.room(&target.key()) {
        Some(room) => room,
        None => {
            let Some(cwd) = state.terminal.host_current_path(&socket, &id).await else {
                return Err(ServerError(ApiError::not_found("tmux session not found")));
            };
            state.terminal.adopt_host(&socket, &id, FsPath::new(&cwd))
        }
    };
    let Some(room) = (room.target == target).then_some(room) else {
        return Err(ServerError(ApiError::not_found("tmux session not found")));
    };
    let role = room
        .claim(user.user.id.as_str(), !watch_only)
        .map_err(map_terminal_error)?;
    let can_take_over = room.allows_takeover() && !watch_only;
    let attachment = match terminal::attach(&room, DEFAULT_COLS, DEFAULT_ROWS, role) {
        Ok(attachment) => attachment,
        Err(error) => {
            room.release(user.user.id.as_str());
            return Err(map_terminal_error(error));
        }
    };
    Ok(ws
        .on_upgrade(move |socket| run(socket, state, room, attachment, role, can_take_over, user))
        .into_response())
}

async fn run(
    socket: WebSocket,
    state: AppState,
    room: std::sync::Arc<terminal::Room>,
    mut attachment: Attachment,
    mut role: Role,
    can_take_over: bool,
    user: CurrentUser,
) {
    let user_id = user.user.id.as_str().to_owned();
    let Some(mut exit) = attachment.exit.take() else {
        room.release(&user_id);
        return;
    };
    let (mut sender, mut receiver) = socket.split();
    let mut events = room.subscribe();
    let mut output_open = true;
    let viewer_users = load_user_briefs(&state.pool, &room.viewer_ids())
        .await
        .unwrap_or_default();

    let ready = ServerMessage::Ready {
        role: role.as_str(),
        session: room.key.clone(),
        cwd: room.cwd.to_string_lossy().into_owned(),
        controller: room.controller(),
        viewers: room.viewers(),
        viewer_users,
        can_take_over,
    };
    if send(&mut sender, &ready).await.is_err() {
        room.release(&user_id);
        return;
    }

    let mut pending: Vec<u8> = Vec::new();
    let mut last_seen = tokio::time::Instant::now();
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
                match event {
                    Ok(terminal::RoomEvent::Control(controller)) => {
                        if room.allows_takeover() {
                            role = if controller.as_deref() == Some(user_id.as_str()) {
                                Role::Controller
                            } else {
                                Role::Watcher
                            };
                        }
                        let next = room.allows_takeover().then_some(role.as_str());
                        let _ = send(
                            &mut sender,
                            &ServerMessage::Control { controller, role: next },
                        )
                        .await;
                    }
                    Ok(terminal::RoomEvent::Viewers) => {
                        let viewer_users =
                            load_user_briefs(&state.pool, &room.viewer_ids())
                                .await
                                .unwrap_or_default();
                        let _ = send(
                            &mut sender,
                            &ServerMessage::Viewers { viewer_users },
                        )
                        .await;
                    }
                    Err(_) => {}
                }
            }
            incoming = receiver.next() => {
                let Some(Ok(message)) = incoming else { break };
                last_seen = tokio::time::Instant::now();
                if let Some(reply) = handle_client(
                    &message,
                    &attachment,
                    &mut role,
                    &state,
                    &room,
                    &user_id,
                    can_take_over,
                )
                .await
                {
                    if send(&mut sender, &reply).await.is_err() {
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                // Browsers ping every few seconds; quiet past that is a
                // socket nobody is driving anymore.
                if last_seen.elapsed() > IDLE_TIMEOUT {
                    break;
                }
                flush(&mut sender, &mut pending, &attachment).await;
            }
        }
    }

    drop(attachment);
    room.release(&user_id);
}

/// Handles one client frame and returns the reply it owes, if any. Anything
/// unparseable is ignored on purpose: a terminal socket that closes on a bad
/// frame is worse than dropping it.
async fn handle_client(
    message: &Message,
    attachment: &Attachment,
    role: &mut Role,
    state: &AppState,
    room: &terminal::Room,
    user_id: &str,
    can_take_over: bool,
) -> Option<ServerMessage> {
    let Message::Text(text) = message else {
        return None;
    };
    let Ok(client) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
        return None;
    };
    match client {
        ClientMessage::Input { data } => {
            // The room controller is authoritative, not this socket's cached
            // role: a takeover must stop the previous driver's bytes on the
            // very next frame.
            if room.controller().as_deref() == Some(user_id) && !data.is_empty() {
                attachment.write(data.into_bytes());
            }
        }
        ClientMessage::Resize { cols, rows } => {
            // Every client sizes its own pty so tmux crops instead of wrapping;
            // only the driver owns the shared window geometry. Host rooms are
            // left to tmux's own window-size policy.
            attachment.resize(cols, rows);
            if room.controller().as_deref() != Some(user_id) || room.allows_takeover() {
                return None;
            }
            let state = state.clone();
            let socket = room.socket.clone();
            let session_id = room.tmux_id.clone();
            tokio::spawn(async move {
                state
                    .terminal
                    .resize_window(&socket, &session_id, cols, rows)
                    .await;
            });
        }
        ClientMessage::Take => {
            if !can_take_over {
                return None;
            }
            // Re-check the live account row: a member downgraded after the
            // ticket was minted must not take the keyboard on an open socket.
            let allowed = load_user_by_id(&state.pool, user_id)
                .await
                .ok()
                .flatten()
                .is_some_and(|user| user.is_admin || matches!(user.access, UserAccess::Member));
            if allowed && room.takeover(user_id) {
                *role = Role::Controller;
            }
        }
        ClientMessage::Ping => return Some(ServerMessage::Pong),
    }
    None
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
