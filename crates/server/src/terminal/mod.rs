//! Web terminal plumbing: tmux sessions, pty clients and attach tickets.
//!
//! A terminal session is a tmux session on a private socket, not a child of the
//! server. That buys three things for free: the shell survives a page reload or
//! a server restart, scrollback lives in tmux (nothing is written to disk), and
//! several browsers can attach to the same shell.
//!
//! Exactly one attached browser *drives*: it gets a read/write tmux client.
//! Everyone else attaches with `tmux attach -r`, so read-only is enforced by
//! tmux itself rather than by the frontend. The tmux client in turn runs inside
//! a pty, because `tmux attach` insists on a real terminal.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use rand::TryRng;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
    /// Members may watch, but only administrators drive.
    Watch,
    /// Administrators own the keyboard.
    Drive,
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
}

struct Ticket {
    session: String,
    user: String,
    expires_at: i64,
}

/// One shared terminal: a tmux session plus the browsers attached to it.
pub struct Room {
    pub id: String,
    pub thread_id: Option<String>,
    pub cwd: PathBuf,
    controller: Mutex<Option<String>>,
    viewers: AtomicU32,
    events: broadcast::Sender<RoomEvent>,
}

impl Room {
    fn new(id: String, thread_id: Option<String>, cwd: PathBuf) -> Self {
        let (events, _) = broadcast::channel(16);
        Self {
            id,
            thread_id,
            cwd,
            controller: Mutex::new(None),
            viewers: AtomicU32::new(0),
            events,
        }
    }

    #[must_use]
    pub fn controller(&self) -> Option<String> {
        self.controller.lock().ok().and_then(|slot| slot.clone())
    }

    #[must_use]
    pub fn viewers(&self) -> u32 {
        self.viewers.load(Ordering::Relaxed)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.events.subscribe()
    }

    /// Claim the keyboard when nobody holds it; otherwise watch.
    pub fn claim(&self, user: &str, may_drive: bool) -> Result<Role, TerminalError> {
        let mut controller = self
            .controller
            .lock()
            .map_err(|_| TerminalError::Pty("terminal state poisoned".into()))?;
        if may_drive && controller.is_none() {
            *controller = Some(user.to_owned());
            self.viewers.fetch_add(1, Ordering::Relaxed);
            let _ = self.events.send(RoomEvent::Control(Some(user.to_owned())));
            return Ok(Role::Controller);
        }
        let viewers = self.viewers.fetch_add(1, Ordering::Relaxed) + 1;
        if viewers > MAX_VIEWERS + 1 {
            self.viewers.fetch_sub(1, Ordering::Relaxed);
            return Err(TerminalError::TooManyViewers);
        }
        Ok(Role::Watcher)
    }

    pub fn release(&self, user: &str, role: Role) {
        self.viewers.fetch_sub(1, Ordering::Relaxed);
        if role != Role::Controller {
            return;
        }
        let Ok(mut controller) = self.controller.lock() else {
            return;
        };
        if controller.as_deref() == Some(user) {
            *controller = None;
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

    fn room_or_create(&self, id: &str, thread_id: Option<&str>, cwd: &Path) -> Arc<Room> {
        let mut rooms = self.rooms.lock().expect("terminal registry poisoned");
        rooms
            .entry(id.to_owned())
            .or_insert_with(|| {
                Arc::new(Room::new(
                    id.to_owned(),
                    thread_id.map(str::to_owned),
                    cwd.to_path_buf(),
                ))
            })
            .clone()
    }

    fn forget(&self, id: &str) {
        if let Ok(mut rooms) = self.rooms.lock() {
            rooms.remove(id);
        }
    }

    /// Mint a single-use ticket bound to one user and one session.
    pub fn ticket(&self, session: &str, user: &str) -> String {
        let token = random_token();
        if let Ok(mut tickets) = self.tickets.lock() {
            let now = now_ms();
            // Tickets live for half a minute; sweeping on mint keeps the map
            // bounded without a background task.
            tickets.retain(|_, ticket| ticket.expires_at > now);
            tickets.insert(
                token.clone(),
                Ticket {
                    session: session.to_owned(),
                    user: user.to_owned(),
                    expires_at: now + TICKET_TTL_MS,
                },
            );
        }
        token
    }

    /// Consume a ticket. It is removed even when it does not match, so a wrong
    /// guess cannot be retried against the same token.
    pub fn take_ticket(&self, token: &str, session: &str, user: &str) -> bool {
        let Ok(mut tickets) = self.tickets.lock() else {
            return false;
        };
        let Some(ticket) = tickets.remove(token) else {
            return false;
        };
        ticket.session == session && ticket.user == user && ticket.expires_at > now_ms()
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
        let mut command = tokio::process::Command::new("tmux");
        command.arg("-L").arg(&self.socket);
        if let Some(conf) = &self.conf {
            command.arg("-f").arg(conf);
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

    async fn alive(&self, id: &str) -> bool {
        self.tmux(&["has-session", "-t", id]).await.is_ok()
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
        Ok(self.room_or_create(id, thread_id, cwd))
    }

    pub async fn attach_room(&self, id: &str) -> Result<Arc<Room>, TerminalError> {
        let room = self.room(id).ok_or(TerminalError::NotFound)?;
        self.configure().await?;
        Ok(room)
    }

    /// Re-register a tmux session that this process did not create, which is
    /// what happens after a server restart while the shell kept running.
    pub fn adopt(&self, id: &str, thread_id: Option<&str>, cwd: &Path) -> Arc<Room> {
        self.room_or_create(id, thread_id, cwd)
    }

    /// Where the shell currently is, for the drawer header.
    pub async fn current_path(&self, id: &str) -> Option<String> {
        self.tmux(&["display-message", "-p", "-t", id, "#{pane_current_path}"])
            .await
            .ok()
            .filter(|path| !path.is_empty())
    }

    pub async fn kill(&self, id: &str) -> Result<(), TerminalError> {
        let _ = self.tmux(&["kill-session", "-t", id]).await;
        self.forget(id);
        Ok(())
    }

    /// Keep the tmux window at the driver's size. Watchers render a crop of it,
    /// so a smaller or larger watcher cannot reflow the driver's terminal.
    pub async fn resize_window(&self, id: &str, cols: u16, rows: u16) {
        let cols = cols.clamp(MIN_COLS, MAX_COLS).to_string();
        let rows = rows.clamp(MIN_ROWS, MAX_ROWS).to_string();
        let _ = self
            .tmux(&["resize-window", "-t", id, "-x", &cols, "-y", &rows])
            .await;
    }
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
    ("remain-on-exit", "on"),
    ("destroy-unattached", "off"),
    ("set-clipboard", "off"),
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
pub fn attach(
    id: &str,
    socket: &str,
    cwd: &Path,
    cols: u16,
    rows: u16,
    role: Role,
) -> Result<Attachment, TerminalError> {
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
    command.args(["-L", socket, "attach"]);
    if role == Role::Watcher {
        // tmux drops keystrokes from a read-only client, which is the entire
        // server-side enforcement of "everyone else only watches".
        command.arg("-r");
    }
    command.args(["-t", id]);
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
    fn tickets_are_single_use_and_bound_to_user_and_session() {
        let dir = tempfile::tempdir().unwrap();
        let hub = TerminalHub::new(dir.path(), "/bin/bash", "tb-test");

        let token = hub.ticket("t-1", "u1");
        assert!(!hub.take_ticket(&token, "t-1", "u2"));
        assert!(!hub.take_ticket(&token, "t-1", "u1"));

        let token = hub.ticket("t-1", "u1");
        assert!(!hub.take_ticket(&token, "t-2", "u1"));

        let token = hub.ticket("t-1", "u1");
        assert!(hub.take_ticket(&token, "t-1", "u1"));
    }

    #[test]
    fn the_first_driver_keeps_the_keyboard() {
        let room = Room::new("t-1".into(), None, PathBuf::from("/tmp"));
        assert_eq!(room.claim("u1", true), Ok(Role::Controller));
        assert_eq!(room.claim("u2", true), Ok(Role::Watcher));
        assert_eq!(room.controller().as_deref(), Some("u1"));
        room.release("u1", Role::Controller);
        assert_eq!(room.controller(), None);
        assert_eq!(room.claim("u2", true), Ok(Role::Controller));
    }

    #[test]
    fn watchers_are_capped() {
        let room = Room::new("t-1".into(), None, PathBuf::from("/tmp"));
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
