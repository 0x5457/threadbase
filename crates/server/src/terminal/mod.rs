//! Web terminal plumbing: tmux sessions, pty clients and attach tickets.
//!
//! Workbench sessions live on a private socket, not as children of the server.
//! That buys three things for free: the shell survives a page reload or a
//! server restart, scrollback lives in tmux (nothing is written to disk), and
//! several browsers can attach to the same shell.
//!
//! A workbench room has exactly one driving browser: it gets a read/write tmux
//! client, everyone else attaches with `tmux attach -r`, so read-only is
//! enforced by tmux itself rather than by the frontend. Host rooms point at the
//! machine user's own tmux servers and are shared, like several local `tmux
//! attach` clients. The tmux client in turn runs inside a pty, because `tmux
//! attach` insists on a real terminal.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use rand::TryRng;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use threadbase_shared::now_ms;
use tokio::sync::{broadcast, mpsc, OnceCell};

/// Default private socket, so threadbase sessions never collide with the user's
/// own tmux sessions.
pub const SOCKET: &str = "threadbase";
/// How long a signed attach ticket stays valid.
const TICKET_TTL_MS: i64 = 30_000;
/// Watchers per session. Each one is a tmux client process plus two threads.
const MAX_VIEWERS: u32 = 4;
/// Output queue between the pty reader and the WebSocket writer.
const OUTPUT_QUEUE: usize = 256;
/// Terminal geometry accepted from the browser.
const MIN_COLS: u16 = 20;
const MAX_COLS: u16 = 500;
const MIN_ROWS: u16 = 5;
const MAX_ROWS: u16 = 200;

/// Who is allowed to type in a session that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Read-only accounts and pending invitations never reach a shell.
    Denied,
    /// Members may watch, but only administrators and allowed drivers type.
    Watch,
    /// Administrators and `THREADBASE_TERMINAL_DRIVERS` own the keyboard.
    Drive,
}

/// Case-insensitive match against `THREADBASE_TERMINAL_DRIVERS` (GitHub logins).
#[must_use]
pub fn is_driver(logins: &[String], username: &str) -> bool {
    logins
        .iter()
        .any(|login| login.eq_ignore_ascii_case(username))
}

#[must_use]
pub fn access_for(access: threadbase_shared::UserAccess, is_admin: bool) -> Access {
    if is_admin {
        return Access::Drive;
    }
    match access {
        threadbase_shared::UserAccess::Member => Access::Watch,
        _ => Access::Denied,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Controller,
    Watcher,
}

impl Role {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Watcher => "watcher",
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TerminalError {
    #[error("tmux is not available")]
    TmuxMissing,
    #[error("terminal session not found")]
    NotFound,
    #[error("terminal session has no workspace directory yet")]
    NoWorkspace,
    #[error("too many viewers are attached to this terminal")]
    TooManyViewers,
    #[error("{0}")]
    Tmux(String),
    #[error("{0}")]
    Pty(String),
}

#[derive(Debug, Clone)]
pub enum RoomEvent {
    Control(Option<String>),
    /// Somebody attached or detached; the receiver reloads the visible
    /// viewer profiles. The event carries no payload so hidden-presence
    /// filtering stays in one place.
    Viewers,
}

/// Which tmux server a room belongs to. Workbench sessions live on the private
/// socket and keep their one-driver rule; host sessions are ordinary tmux
/// sessions and may be driven by every member at once, like `tmux attach` from
/// several terminals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRef {
    Workbench(String),
    Host { socket: String, id: String },
}

impl SessionRef {
    #[must_use]
    pub fn workbench(id: impl Into<String>) -> Self {
        Self::Workbench(id.into())
    }

    #[must_use]
    pub fn host(socket: impl Into<String>, id: impl Into<String>) -> Self {
        Self::Host {
            socket: socket.into(),
            id: id.into(),
        }
    }

    /// Registry key. Workbench keys stay the tmux session name; host keys add
    /// the socket because `$0` is only unique within one tmux server.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Workbench(id) => id.clone(),
            Self::Host { socket, id } => format!("host:{socket}:{id}"),
        }
    }

    #[must_use]
    pub fn is_host(&self) -> bool {
        matches!(self, Self::Host { .. })
    }
}

/// One attach ticket. It names the exact tmux session, so the WebSocket route
/// does not have to repeat a user-controlled path segment.
struct Ticket {
    target: SessionRef,
    user: String,
    expires_at: i64,
    /// Set when the caller may not take the keyboard, e.g. a phone that can
    /// only watch.
    watch_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketGrant {
    pub target: SessionRef,
    pub watch_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSession {
    pub id: String,
    pub name: String,
    pub windows: u32,
    pub attached: u32,
    pub created_at: i64,
    pub activity_at: i64,
    pub cwd: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostServer {
    pub socket: String,
    pub sessions: Vec<HostSession>,
    pub error: Option<String>,
}

/// Workbench rooms are private to a thread and keep the classic one-driver
/// rule with read-only tmux clients for watchers. Host rooms are exclusive too,
/// but every attached browser gets a writable tmux client so a watcher can take
/// the keyboard without reconnecting; the server still forwards input only for
/// the current controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomMode {
    Workbench,
    Host,
}

impl RoomMode {
    #[must_use]
    pub fn allows_takeover(self) -> bool {
        matches!(self, Self::Host)
    }

    /// Workbench watchers attach with `tmux attach -r`; host watchers need a
    /// writable client so an explicit takeover can promote them in place.
    #[must_use]
    pub fn read_only_watchers(self) -> bool {
        matches!(self, Self::Workbench)
    }
}

#[derive(Debug, Default)]
struct RoomState {
    controller: Option<String>,
    /// User id to number of attached browsers. A user with two tabs counts
    /// twice in the cap but once in the viewer list.
    viewers: HashMap<String, u32>,
}

/// One shared terminal: a tmux session plus the browsers attached to it.
pub struct Room {
    /// Registry key and the id sent to the browser.
    pub key: String,
    pub target: SessionRef,
    /// Socket and session id used for every tmux command on this room.
    pub socket: String,
    pub tmux_id: String,
    pub thread_id: Option<String>,
    pub cwd: PathBuf,
    mode: RoomMode,
    state: Mutex<RoomState>,
    events: broadcast::Sender<RoomEvent>,
}

impl Room {
    fn new(
        target: SessionRef,
        socket: String,
        tmux_id: String,
        thread_id: Option<String>,
        cwd: PathBuf,
        mode: RoomMode,
    ) -> Self {
        let (events, _) = broadcast::channel(16);
        Self {
            key: target.key(),
            target,
            socket,
            tmux_id,
            thread_id,
            cwd,
            mode,
            state: Mutex::new(RoomState::default()),
            events,
        }
    }

    #[must_use]
    pub fn mode(&self) -> RoomMode {
        self.mode
    }

    #[must_use]
    pub fn allows_takeover(&self) -> bool {
        self.mode.allows_takeover()
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, RoomState>, TerminalError> {
        self.state
            .lock()
            .map_err(|_| TerminalError::Pty("terminal state poisoned".into()))
    }

    #[must_use]
    pub fn controller(&self) -> Option<String> {
        self.lock_state()
            .ok()
            .and_then(|state| state.controller.clone())
    }

    #[must_use]
    pub fn viewers(&self) -> u32 {
        self.lock_state()
            .map(|state| state.viewers.values().sum())
            .unwrap_or(0)
    }

    /// Unique attached users, sorted for a stable avatar order.
    #[must_use]
    pub fn viewer_ids(&self) -> Vec<String> {
        let Ok(state) = self.lock_state() else {
            return Vec::new();
        };
        let mut ids: Vec<String> = state.viewers.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.events.subscribe()
    }

    /// Claim the keyboard when nobody holds it; otherwise watch. Host rooms use
    /// the same rule, then let a watcher call [`Room::takeover`].
    pub fn claim(&self, user: &str, may_drive: bool) -> Result<Role, TerminalError> {
        let mut state = self.lock_state()?;
        let viewers: u32 = state.viewers.values().sum();
        if viewers > MAX_VIEWERS {
            return Err(TerminalError::TooManyViewers);
        }
        *state.viewers.entry(user.to_owned()).or_insert(0) += 1;
        if may_drive && state.controller.is_none() {
            state.controller = Some(user.to_owned());
            let _ = self.events.send(RoomEvent::Viewers);
            let _ = self.events.send(RoomEvent::Control(Some(user.to_owned())));
            return Ok(Role::Controller);
        }
        let _ = self.events.send(RoomEvent::Viewers);
        Ok(Role::Watcher)
    }

    /// Move the keyboard to a watcher without reconnecting. Only host rooms
    /// allow this; workbench viewers keep their read-only tmux client.
    pub fn takeover(&self, user: &str) -> bool {
        if !self.mode.allows_takeover() {
            return false;
        }
        let Ok(mut state) = self.lock_state() else {
            return false;
        };
        if !state.viewers.contains_key(user) {
            return false;
        }
        if state.controller.as_deref() == Some(user) {
            return true;
        }
        state.controller = Some(user.to_owned());
        let _ = self.events.send(RoomEvent::Control(Some(user.to_owned())));
        true
    }

    pub fn release(&self, user: &str, role: Role) {
        let Ok(mut state) = self.lock_state() else {
            return;
        };
        let remove = match state.viewers.get_mut(user) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => true,
            None => false,
        };
        if remove {
            state.viewers.remove(user);
        }
        let _ = self.events.send(RoomEvent::Viewers);
        if role == Role::Controller && state.controller.as_deref() == Some(user) {
            state.controller = None;
            let _ = self.events.send(RoomEvent::Control(None));
        }
    }
}

/// Process-wide registry of rooms, tickets and the tmux socket configuration.
pub struct TerminalHub {
    shell: String,
    socket: String,
    conf: Option<PathBuf>,
    configured: OnceCell<()>,
    rooms: Mutex<HashMap<String, Arc<Room>>>,
    tickets: Mutex<HashMap<String, Ticket>>,
}

impl TerminalHub {
    #[must_use]
    pub fn new(data_dir: &Path, shell: &str, socket: &str) -> Self {
        let shell = safe_shell(shell);
        let conf = data_dir.join("terminal-tmux.conf");
        let conf = match std::fs::write(&conf, tmux_config(&shell)) {
            Ok(()) => Some(conf),
            Err(error) => {
                tracing::warn!(%error, "could not write the terminal tmux config");
                None
            }
        };
        Self {
            shell,
            socket: socket.to_owned(),
            conf,
            configured: OnceCell::new(),
            rooms: Mutex::new(HashMap::new()),
            tickets: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn shell(&self) -> &str {
        &self.shell
    }

    #[must_use]
    pub fn room(&self, id: &str) -> Option<Arc<Room>> {
        self.rooms.lock().ok()?.get(id).cloned()
    }

    fn room_or_create(
        &self,
        target: SessionRef,
        socket: &str,
        tmux_id: &str,
        thread_id: Option<&str>,
        cwd: &Path,
        mode: RoomMode,
    ) -> Arc<Room> {
        let key = target.key();
        let mut rooms = self.rooms.lock().expect("terminal registry poisoned");
        rooms
            .entry(key)
            .or_insert_with(|| {
                Arc::new(Room::new(
                    target,
                    socket.to_owned(),
                    tmux_id.to_owned(),
                    thread_id.map(str::to_owned),
                    cwd.to_path_buf(),
                    mode,
                ))
            })
            .clone()
    }

    fn forget(&self, key: &str) {
        if let Ok(mut rooms) = self.rooms.lock() {
            rooms.remove(key);
        }
    }

    /// Mint a single-use ticket bound to one user and one session.
    pub fn ticket(&self, target: SessionRef, user: &str, watch_only: bool) -> String {
        let token = random_token();
        if let Ok(mut tickets) = self.tickets.lock() {
            let now = now_ms();
            // Tickets live for half a minute; sweeping on mint keeps the map
            // bounded without a background task.
            tickets.retain(|_, ticket| ticket.expires_at > now);
            tickets.insert(
                token.clone(),
                Ticket {
                    target,
                    user: user.to_owned(),
                    expires_at: now + TICKET_TTL_MS,
                    watch_only,
                },
            );
        }
        token
    }

    /// Consume a ticket. It is removed even when the user does not match, so a
    /// wrong guess cannot be retried against the same token.
    pub fn take_ticket(&self, token: &str, user: &str) -> Option<TicketGrant> {
        let mut tickets = self.tickets.lock().ok()?;
        let ticket = tickets.remove(token)?;
        if ticket.user != user || ticket.expires_at <= now_ms() {
            return None;
        }
        Some(TicketGrant {
            target: ticket.target,
            watch_only: ticket.watch_only,
        })
    }

    /// Apply the options to a server this process did not start.
    async fn configure(&self) -> Result<(), TerminalError> {
        self.configured
            .get_or_try_init(|| async {
                let mut args: Vec<String> = Vec::new();
                for (key, _) in OPTIONS {
                    args.extend([
                        "set".to_owned(),
                        "-g".to_owned(),
                        (*key).to_owned(),
                        option_value(key, &self.shell).to_owned(),
                        ";".to_owned(),
                    ]);
                }
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                self.tmux(&refs).await?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn tmux(&self, args: &[&str]) -> Result<String, TerminalError> {
        self.tmux_command(&self.socket, args, true).await
    }

    /// Run against a host server without the workbench config file: those
    /// options (status off, window-size manual, ...) belong only to the private
    /// socket and must never leak into the user's own tmux server.
    async fn host_tmux(&self, socket: &str, args: &[&str]) -> Result<String, TerminalError> {
        self.tmux_command(socket, args, false).await
    }

    async fn tmux_command(
        &self,
        socket: &str,
        args: &[&str],
        with_conf: bool,
    ) -> Result<String, TerminalError> {
        let mut command = tokio::process::Command::new("tmux");
        command.arg("-L").arg(socket);
        if with_conf {
            if let Some(conf) = &self.conf {
                command.arg("-f").arg(conf);
            }
        }
        command.args(args);
        command.stdin(std::process::Stdio::null());
        command.kill_on_drop(true);
        let output = match command.output().await {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(TerminalError::TmuxMissing)
            }
            Err(error) => return Err(TerminalError::Tmux(error.to_string())),
        };
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_owned());
        }
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(TerminalError::Tmux(if message.is_empty() {
            "tmux command failed".to_owned()
        } else {
            message
        }))
    }

    /// Live sessions on the private socket, newest name last.
    pub async fn sessions(&self) -> Vec<String> {
        match self.tmux(&["list-sessions", "-F", "#{session_name}"]).await {
            Ok(listed) => listed
                .lines()
                .map(str::trim)
                .filter(|name| is_session_id(name))
                .map(str::to_owned)
                .collect(),
            // No server yet means no sessions, not an error.
            Err(_) => Vec::new(),
        }
    }

    /// Sessions on the configured host sockets. The private workbench socket is
    /// skipped even if it was configured: its one-driver rule must not be
    /// bypassed by the host route.
    pub async fn host_sessions(&self, sockets: &[String]) -> Vec<HostServer> {
        let mut servers = Vec::new();
        for socket in sockets {
            if socket == &self.socket || !is_socket_name(socket) {
                continue;
            }
            let server = match self.list_host(socket).await {
                Ok(sessions) => HostServer {
                    socket: socket.clone(),
                    sessions,
                    error: None,
                },
                // A socket with no server is an empty server, not an error.
                Err(error) if no_server(&error) => HostServer {
                    socket: socket.clone(),
                    sessions: Vec::new(),
                    error: None,
                },
                Err(error) => HostServer {
                    socket: socket.clone(),
                    sessions: Vec::new(),
                    error: Some(error.to_string()),
                },
            };
            servers.push(server);
        }
        servers
    }

    pub async fn host_session(&self, socket: &str, id: &str) -> Option<HostSession> {
        self.list_host(socket)
            .await
            .ok()?
            .into_iter()
            .find(|session| session.id == id)
    }

    async fn list_host(&self, socket: &str) -> Result<Vec<HostSession>, TerminalError> {
        let listed = self
            .host_tmux(socket, &["list-sessions", "-F", HOST_SESSION_FORMAT])
            .await?;
        let mut sessions = parse_host_sessions(&listed);
        if let Ok(panes) = self
            .host_tmux(socket, &["list-panes", "-a", "-F", HOST_PANE_FORMAT])
            .await
        {
            apply_host_panes(&mut sessions, &panes);
        }
        Ok(sessions)
    }

    async fn alive(&self, id: &str) -> bool {
        self.tmux(&["has-session", "-t", id]).await.is_ok()
    }

    async fn host_alive(&self, socket: &str, id: &str) -> bool {
        self.host_tmux(socket, &["has-session", "-t", id])
            .await
            .is_ok()
    }

    /// Create the tmux session when it does not exist yet.
    pub async fn ensure(
        &self,
        id: &str,
        thread_id: Option<&str>,
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<Room>, TerminalError> {
        if !cwd.is_dir() {
            return Err(TerminalError::NoWorkspace);
        }
        // A tmux server with no sessions exits immediately, so the options can
        // only be applied once one exists.
        if !self.alive(id).await {
            let cwd = cwd.to_string_lossy().into_owned();
            let cols = cols.clamp(MIN_COLS, MAX_COLS).to_string();
            let rows = rows.clamp(MIN_ROWS, MAX_ROWS).to_string();
            // A concurrent create wins the race; a real failure surfaces from
            // the follow-up check.
            let error = self
                .tmux(&[
                    "new-session",
                    "-d",
                    "-s",
                    id,
                    "-c",
                    &cwd,
                    "-x",
                    &cols,
                    "-y",
                    &rows,
                ])
                .await
                .err();
            if !self.alive(id).await {
                return Err(error.unwrap_or(TerminalError::Tmux("could not start tmux".into())));
            }
        }
        self.configure().await?;
        Ok(self.room_or_create(
            SessionRef::workbench(id),
            &self.socket,
            id,
            thread_id,
            cwd,
            RoomMode::Workbench,
        ))
    }

    /// Adopt an existing host session. The room allows an explicit takeover,
    /// so watchers get a writable tmux client and the server gates input.
    pub async fn ensure_host(&self, socket: &str, id: &str) -> Result<Arc<Room>, TerminalError> {
        if socket == self.socket || !is_socket_name(socket) || !is_host_session_id(id) {
            return Err(TerminalError::NotFound);
        }
        let Some(cwd) = self.host_current_path(socket, id).await else {
            return Err(TerminalError::NotFound);
        };
        if !self.host_alive(socket, id).await {
            return Err(TerminalError::NotFound);
        }
        Ok(self.room_or_create(
            SessionRef::host(socket, id),
            socket,
            id,
            None,
            Path::new(&cwd),
            RoomMode::Host,
        ))
    }

    pub async fn attach_room(&self, target: &SessionRef) -> Result<Arc<Room>, TerminalError> {
        let room = self.room(&target.key()).ok_or(TerminalError::NotFound)?;
        if !target.is_host() {
            self.configure().await?;
        }
        Ok(room)
    }

    /// Re-register a tmux session that this process did not create, which is
    /// what happens after a server restart while the shell kept running.
    pub fn adopt(&self, id: &str, thread_id: Option<&str>, cwd: &Path) -> Arc<Room> {
        self.room_or_create(
            SessionRef::workbench(id),
            &self.socket,
            id,
            thread_id,
            cwd,
            RoomMode::Workbench,
        )
    }

    pub fn adopt_host(&self, socket: &str, id: &str, cwd: &Path) -> Arc<Room> {
        self.room_or_create(
            SessionRef::host(socket, id),
            socket,
            id,
            None,
            cwd,
            RoomMode::Host,
        )
    }

    /// Where the shell currently is, for the drawer header.
    pub async fn current_path(&self, target: &SessionRef) -> Option<String> {
        match target {
            SessionRef::Workbench(id) => self
                .tmux(&["display-message", "-p", "-t", id, "#{pane_current_path}"])
                .await
                .ok()
                .filter(|path| !path.is_empty()),
            SessionRef::Host { socket, id } => self.host_current_path(socket, id).await,
        }
    }

    pub async fn host_current_path(&self, socket: &str, id: &str) -> Option<String> {
        self.host_tmux(
            socket,
            &["display-message", "-p", "-t", id, "#{pane_current_path}"],
        )
        .await
        .ok()
        .filter(|path| !path.is_empty())
    }

    pub async fn kill(&self, target: &SessionRef) -> Result<(), TerminalError> {
        let SessionRef::Workbench(id) = target else {
            // Host sessions belong to the user, not to threadbase. Closing the
            // browser must never kill their shell.
            return Err(TerminalError::NotFound);
        };
        let _ = self.tmux(&["kill-session", "-t", id]).await;
        self.forget(&target.key());
        Ok(())
    }

    /// Keep the tmux window at the driver's size. Watchers render a crop of it,
    /// so a smaller or larger watcher cannot reflow the driver's terminal. Host
    /// rooms are shared and left to tmux's own window-size policy.
    pub async fn resize_window(&self, socket: &str, id: &str, cols: u16, rows: u16) {
        let cols = cols.clamp(MIN_COLS, MAX_COLS).to_string();
        let rows = rows.clamp(MIN_ROWS, MAX_ROWS).to_string();
        let _ = self
            .tmux_command(
                socket,
                &["resize-window", "-t", id, "-x", &cols, "-y", &rows],
                true,
            )
            .await;
    }
}

/// Session fields, with the free-form name last so `splitn` can keep tabs in
/// it. Timestamps are Unix seconds; the wire format uses milliseconds.
const HOST_SESSION_FORMAT: &str = "#{session_id}\t#{session_windows}\t#{session_attached}\t#{session_created}\t#{session_activity}\t#{session_name}";
/// Only the active pane of the active window describes a session's current
/// command and directory. The path is last for the same reason as the name.
const HOST_PANE_FORMAT: &str = "#{session_id}\t#{window_active}\t#{pane_active}\t#{pane_current_command}\t#{pane_current_path}";

fn parse_host_sessions(listed: &str) -> Vec<HostSession> {
    listed
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(6, '\t');
            let id = fields.next()?.trim();
            if !is_host_session_id(id) {
                return None;
            }
            let windows = fields.next()?.trim().parse().unwrap_or(0);
            let attached = fields.next()?.trim().parse().unwrap_or(0);
            let created = fields.next()?.trim().parse::<i64>().unwrap_or(0);
            let activity = fields.next()?.trim().parse::<i64>().unwrap_or(0);
            let name = fields.next()?.to_owned();
            Some(HostSession {
                id: id.to_owned(),
                name,
                windows,
                attached,
                created_at: created.saturating_mul(1000),
                activity_at: activity.saturating_mul(1000),
                cwd: None,
                command: None,
            })
        })
        .collect()
}

fn apply_host_panes(sessions: &mut [HostSession], listed: &str) {
    let mut active: HashMap<String, (String, String)> = HashMap::new();
    for line in listed.lines() {
        let mut fields = line.splitn(5, '\t');
        let Some(id) = fields.next() else { continue };
        if !is_host_session_id(id) {
            continue;
        }
        let Some(window_active) = fields.next() else {
            continue;
        };
        let Some(pane_active) = fields.next() else {
            continue;
        };
        if window_active != "1" || pane_active != "1" {
            continue;
        }
        let Some(command) = fields.next() else {
            continue;
        };
        let Some(path) = fields.next() else {
            continue;
        };
        active.insert(id.to_owned(), (command.to_owned(), path.to_owned()));
    }
    for session in sessions {
        if let Some((command, path)) = active.remove(&session.id) {
            session.command = Some(command);
            session.cwd = Some(path);
        }
    }
}

fn no_server(error: &TerminalError) -> bool {
    let TerminalError::Tmux(message) = error else {
        return false;
    };
    message.contains("no server running")
        || message.contains("error connecting to")
        || message.contains("No such file or directory")
}

/// The shell path lands in a tmux config file, so anything exotic falls back
/// to bash instead of being interpolated.
fn safe_shell(shell: &str) -> String {
    let looks_safe = shell.starts_with('/')
        && shell.len() < 128
        && shell.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-' | b'+')
        });
    if looks_safe {
        shell.to_owned()
    } else {
        "/bin/bash".to_owned()
    }
}

/// The tmux options a threadbase shell needs. They are written to the config
/// file for a fresh server, and applied again at runtime because `tmux -f` is
/// ignored once a server for the socket is already up.
const OPTIONS: &[(&str, &str)] = &[
    ("default-shell", ""),
    ("default-terminal", "tmux-256color"),
    // Our drawer is the status bar; tmux must not eat a row for its own.
    ("status", "off"),
    ("mouse", "on"),
    ("escape-time", "10"),
    ("focus-events", "on"),
    // tmux keeps scrollback in memory: nothing is written to disk.
    ("history-limit", "10000"),
    // The driver owns the window size; watchers render a crop of it.
    ("window-size", "manual"),
    // When the shell exits the client must see EOF, so the drawer can offer a
    // fresh session instead of showing a dead pane forever.
    ("remain-on-exit", "off"),
    ("destroy-unattached", "off"),
    ("set-clipboard", "off"),
    // Rows outside a smaller-than-client window are padded with this; tmux's
    // default dot pattern reads as corruption in the drawer.
    ("fill-character", " "),
];

fn option_value<'a>(key: &str, shell: &'a str) -> &'a str {
    if key == "default-shell" {
        shell
    } else {
        OPTIONS
            .iter()
            .find(|(name, _)| *name == key)
            .map_or("", |(_, value)| value)
    }
}

fn tmux_config(shell: &str) -> String {
    let mut config = String::new();
    for (key, _) in OPTIONS {
        let value = option_value(key, shell);
        if value.contains(' ') || value.contains('"') || value.is_empty() {
            config.push_str(&format!("set -g {key} \"{value}\"\n"));
        } else {
            config.push_str(&format!("set -g {key} {value}\n"));
        }
    }
    config
}

#[must_use]
pub fn is_session_id(id: &str) -> bool {
    (id.starts_with("t-") || id.starts_with("h-"))
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// tmux's stable session id (`$0`, `$12`, ...). A name would also match tmux's
/// target grammar, so host sessions are always addressed by id.
#[must_use]
pub fn is_host_session_id(id: &str) -> bool {
    id.len() > 1
        && id.len() <= 16
        && id.starts_with('$')
        && id[1..].bytes().all(|byte| byte.is_ascii_digit())
}

/// `-L` names become file names in the tmux socket directory; keeping them to
/// a small alphabet also keeps the room key unambiguous.
#[must_use]
pub fn is_socket_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn random_token() -> String {
    let mut bytes = [0u8; 24];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("operating system random source must be available");
    hex::encode(bytes)
}

/// One browser attached to one tmux session.
///
/// Output arrives on a bounded channel so a browser that stops reading cannot
/// grow memory: the reader drops frames and counts them instead. Input and
/// resizes go through the driver thread that owns the pty handles.
pub struct Attachment {
    pub output: mpsc::Receiver<Vec<u8>>,
    pub exit: Option<tokio::sync::oneshot::Receiver<i32>>,
    pub dropped: Arc<AtomicU64>,
    input: std::sync::mpsc::Sender<DriverMessage>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

enum DriverMessage {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

impl Attachment {
    pub fn write(&self, bytes: Vec<u8>) {
        let _ = self.input.send(DriverMessage::Data(bytes));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.input.send(DriverMessage::Resize { cols, rows });
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        // Detach the tmux client; the session keeps running.
        let _ = self.killer.kill();
    }
}

/// Spawn a tmux client inside a fresh pty.
pub fn attach(room: &Room, cols: u16, rows: u16, role: Role) -> Result<Attachment, TerminalError> {
    let cols = cols.clamp(MIN_COLS, MAX_COLS);
    let rows = rows.clamp(MIN_ROWS, MAX_ROWS);
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| TerminalError::Pty(error.to_string()))?;

    let mut command = CommandBuilder::new("tmux");
    command.args(["-L", &room.socket, "attach"]);
    if role == Role::Watcher && room.mode().read_only_watchers() {
        // Workbench watchers are read-only at the tmux layer. Host watchers
        // keep a writable client so a takeover can promote them in place; the
        // server still drops their input until they hold the keyboard.
        command.arg("-r");
    }
    command.args(["-t", &room.tmux_id]);
    // A pane's directory can disappear while the shell keeps running; the
    // client must still attach, so fall back instead of failing to spawn.
    let cwd = if room.cwd.is_dir() {
        room.cwd.clone()
    } else {
        std::env::temp_dir()
    };
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env_remove("TMUX");

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| TerminalError::Pty(error.to_string()))?;
    // The slave side must go, or the master never sees EOF.
    drop(pair.slave);

    let master = pair.master;
    let mut reader = master
        .try_clone_reader()
        .map_err(|error| TerminalError::Pty(error.to_string()))?;
    let writer = master
        .take_writer()
        .map_err(|error| TerminalError::Pty(error.to_string()))?;
    let killer = child.clone_killer();

    let (out_tx, output) = mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE);
    let (exit_tx, exit) = tokio::sync::oneshot::channel::<i32>();
    let dropped = Arc::new(AtomicU64::new(0));

    let reader_dropped = Arc::clone(&dropped);
    std::thread::spawn(move || {
        read_output(&mut reader, &out_tx, &reader_dropped);
        let code = child.wait().map_or(-1, |status| status.exit_code() as i32);
        let _ = exit_tx.send(code);
    });

    let (input, messages) = std::sync::mpsc::channel::<DriverMessage>();
    std::thread::spawn(move || drive(master, writer, &messages));

    Ok(Attachment {
        output,
        exit: Some(exit),
        dropped,
        input,
        killer,
    })
}

fn read_output(
    reader: &mut Box<dyn Read + Send>,
    out: &mpsc::Sender<Vec<u8>>,
    dropped: &AtomicU64,
) {
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(size) => {
                match out.try_send(buffer[..size].to_vec()) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        // The browser stopped reading; drop frames rather than
                        // growing the queue without bound.
                        dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => return,
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn drive(
    master: Box<dyn MasterPty + Send>,
    mut writer: Box<dyn Write + Send>,
    messages: &std::sync::mpsc::Receiver<DriverMessage>,
) {
    while let Ok(message) = messages.recv() {
        match message {
            DriverMessage::Data(bytes) => {
                if writer.write_all(&bytes).is_err() {
                    return;
                }
                let _ = writer.flush();
            }
            DriverMessage::Resize { cols, rows } => {
                let _ = master.resize(PtySize {
                    rows: rows.clamp(MIN_ROWS, MAX_ROWS),
                    cols: cols.clamp(MIN_COLS, MAX_COLS),
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_shared::UserAccess;

    #[test]
    fn session_ids_are_t_and_h_scoped() {
        assert!(is_session_id("t-01M789SXFBEPWYPK77J4YXXK9A"));
        assert!(is_session_id("h-01M789SXFBEPWYPK77J4YXXK9A"));
        assert!(!is_session_id("thread/1"));
        assert!(!is_session_id(";rm -rf /"));
        assert!(!is_session_id(""));
        assert!(!is_session_id("t-a b"));
    }

    #[test]
    fn administrators_drive_and_members_watch() {
        assert_eq!(access_for(UserAccess::Member, true), Access::Drive);
        assert_eq!(access_for(UserAccess::Member, false), Access::Watch);
        assert_eq!(access_for(UserAccess::ReadOnly, false), Access::Denied);
        assert_eq!(access_for(UserAccess::Pending, false), Access::Denied);
    }

    #[test]
    fn terminal_driver_logins_match_case_insensitively() {
        let drivers = vec!["0x5459".to_owned(), "Alice".to_owned()];
        assert!(is_driver(&drivers, "0x5459"));
        assert!(is_driver(&drivers, "alice"));
        assert!(!is_driver(&drivers, "0x5457"));
        assert!(!is_driver(&[], "0x5459"));
    }

    fn room(mode: RoomMode) -> Room {
        let target = SessionRef::workbench("t-1");
        Room::new(
            target,
            "tb-test".to_owned(),
            "t-1".to_owned(),
            None,
            PathBuf::from("/tmp"),
            mode,
        )
    }

    #[test]
    fn host_session_ids_are_strict_and_socket_names_are_safe() {
        assert!(is_host_session_id("$0"));
        assert!(is_host_session_id("$123"));
        assert!(!is_host_session_id("$x"));
        assert!(!is_host_session_id("$1:2"));
        assert!(!is_host_session_id("0"));
        assert!(!is_host_session_id(""));
        assert!(is_socket_name("default"));
        assert!(is_socket_name("work-1.2"));
        assert!(!is_socket_name("../evil"));
        assert!(!is_socket_name("a:b"));
        assert!(!is_socket_name(""));
        assert!(!is_socket_name("."));
        assert!(!is_socket_name(".."));
    }

    #[test]
    fn host_sessions_parse_by_id_and_keep_the_name_last() {
        let sessions = parse_host_sessions(
            "$0\t2\t1\t1700000000\t1700000010\tapi server\n$4\t1\t0\t1700000005\t1700000006\twith\ttab",
        );
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, "$0");
        assert_eq!(sessions[0].name, "api server");
        assert_eq!(sessions[0].windows, 2);
        assert_eq!(sessions[0].attached, 1);
        assert_eq!(sessions[0].created_at, 1_700_000_000_000);
        assert_eq!(sessions[1].name, "with\ttab");

        let mut sessions = sessions;
        apply_host_panes(
            &mut sessions,
            "$0\t1\t0\tbash\t/tmp/inactive\n$0\t1\t1\tnode\t/tmp/active\n$4\t0\t1\tvim\t/tmp/other",
        );
        assert_eq!(sessions[0].command.as_deref(), Some("node"));
        assert_eq!(sessions[0].cwd.as_deref(), Some("/tmp/active"));
        assert_eq!(sessions[1].command, None);
    }

    #[test]
    fn tickets_are_single_use_and_bound_to_user_and_target() {
        let dir = tempfile::tempdir().unwrap();
        let hub = TerminalHub::new(dir.path(), "/bin/bash", "tb-test");
        let target = SessionRef::workbench("t-1");

        let token = hub.ticket(target.clone(), "u1", false);
        assert_eq!(hub.take_ticket(&token, "u2"), None);
        assert_eq!(hub.take_ticket(&token, "u1"), None);

        let token = hub.ticket(target.clone(), "u1", false);
        assert_eq!(
            hub.take_ticket(&token, "u1"),
            Some(TicketGrant {
                target: target.clone(),
                watch_only: false,
            })
        );

        let token = hub.ticket(target.clone(), "u1", true);
        assert_eq!(
            hub.take_ticket(&token, "u1"),
            Some(TicketGrant {
                target: target.clone(),
                watch_only: true,
            })
        );

        let host = SessionRef::host("default", "$3");
        let token = hub.ticket(host.clone(), "u1", false);
        assert_eq!(
            hub.take_ticket(&token, "u1"),
            Some(TicketGrant {
                target: host,
                watch_only: false,
            })
        );
    }

    #[test]
    fn the_first_driver_keeps_the_keyboard() {
        let room = room(RoomMode::Workbench);
        assert_eq!(room.claim("u1", true), Ok(Role::Controller));
        assert_eq!(room.claim("u2", true), Ok(Role::Watcher));
        assert_eq!(room.controller().as_deref(), Some("u1"));
        room.release("u1", Role::Controller);
        assert_eq!(room.controller(), None);
        assert_eq!(room.claim("u2", true), Ok(Role::Controller));
    }

    #[test]
    fn host_rooms_promote_a_watcher_without_reattaching() {
        let room = room(RoomMode::Host);
        assert_eq!(room.claim("u1", true), Ok(Role::Controller));
        assert_eq!(room.claim("u2", true), Ok(Role::Watcher));
        assert_eq!(room.controller().as_deref(), Some("u1"));
        assert!(room.takeover("u2"));
        assert_eq!(room.controller().as_deref(), Some("u2"));
        // Taking over twice is a no-op, not an error.
        assert!(room.takeover("u2"));
        // A user who is not attached cannot take the keyboard.
        assert!(!room.takeover("u3"));
        room.release("u2", Role::Controller);
        assert_eq!(room.controller(), None);
        assert!(room.takeover("u1"));
    }

    #[test]
    fn workbench_rooms_never_take_over() {
        let room = room(RoomMode::Workbench);
        assert_eq!(room.claim("u1", true), Ok(Role::Controller));
        assert_eq!(room.claim("u2", true), Ok(Role::Watcher));
        assert!(!room.takeover("u2"));
        assert_eq!(room.controller().as_deref(), Some("u1"));
    }

    #[test]
    fn viewer_ids_are_unique_and_drop_with_the_last_tab() {
        let room = room(RoomMode::Host);
        assert_eq!(room.claim("u1", true), Ok(Role::Controller));
        assert_eq!(room.claim("u1", true), Ok(Role::Watcher));
        assert_eq!(room.claim("u2", false), Ok(Role::Watcher));
        assert_eq!(room.viewers(), 3);
        assert_eq!(room.viewer_ids(), ["u1", "u2"]);
        room.release("u1", Role::Watcher);
        assert_eq!(room.viewers(), 2);
        assert_eq!(room.viewer_ids(), ["u1", "u2"]);
        room.release("u1", Role::Controller);
        assert_eq!(room.viewers(), 1);
        assert_eq!(room.viewer_ids(), ["u2"]);
    }

    #[test]
    fn watchers_are_capped() {
        let room = room(RoomMode::Workbench);
        assert_eq!(room.claim("driver", true), Ok(Role::Controller));
        for index in 0..MAX_VIEWERS {
            assert_eq!(room.claim(&format!("w{index}"), true), Ok(Role::Watcher));
        }
        assert_eq!(
            room.claim("extra", true),
            Err(TerminalError::TooManyViewers)
        );
    }
}
