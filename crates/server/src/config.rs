use axum::http::uri::Authority;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use threadbase_shared::PermissionMode;

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub bind: SocketAddr,
    /// Public HTTPS origin, for example the ts.net hostname.
    pub public_origin: Option<String>,
    /// Tailnet logins that are always administrators.
    pub admins: Vec<String>,
    /// Optional identity accepted from direct local connections (development).
    pub dev_identity: Option<String>,
    /// Session lifetime; sessions slide forward while in use.
    pub session_seconds: i64,
    /// `Secure` cookie flag. Derived from `public_origin`, overridable.
    pub cookie_secure: bool,
    pub log_json: bool,
    /// `THREADBASE_TERMINAL=1` turns on the browser terminal. A shell over HTTP
    /// is remote code execution by design, so it stays off unless asked for.
    pub terminal: bool,
    /// `THREADBASE_TERMINAL_HOST=1` also lists and attaches to the tmux
    /// servers outside the private socket. Every member gets a shell as the
    /// service account, so this is a separate opt-in from `terminal`.
    pub terminal_host: bool,
    /// Named tmux sockets to expose as host terminals (`-L` names).
    pub terminal_host_sockets: Vec<String>,
    /// Non-admin tailnet logins that may drive the browser terminal.
    pub terminal_drivers: Vec<String>,
    /// Login shell started by tmux; `$SHELL` when it looks like a path.
    pub terminal_shell: String,
    /// tmux socket name. Tests and side-by-side instances use their own.
    pub terminal_socket: String,
    /// Idle ACP sessions are shut down after this many seconds; `0` disables
    /// the reaper. The binding keeps the ACP session id, so the next prompt
    /// resumes it transparently.
    pub idle_session_seconds: i64,
    /// Maximum number of agent turns running at once across all threads.
    /// `0` means unlimited.
    pub max_active_turns: usize,
    /// Idle threads settle themselves after this many days; `0` disables the
    /// rule. Snooze deadlines keep waking regardless, because a person asked
    /// to see the thread again.
    pub settle_days: i64,
    /// Mode a thread starts in when neither it nor its project chose one.
    pub permission_mode: PermissionMode,
    /// Ceiling no thread may exceed. Whoever runs the machine decides how much
    /// autonomy the machine hands out; the UI renders modes above it as managed.
    pub max_permission_mode: PermissionMode,
    /// Install a staged binary and restart, but only while no turn is running.
    /// Off by default: a server replacing itself is not something to do unasked.
    pub auto_deploy: bool,
    /// Where a staged binary is expected. `None` means
    /// `<data dir>/incoming/threadbase`.
    pub auto_deploy_path: Option<PathBuf>,
    /// Service unit to restart after installing. Only matters when one machine
    /// runs several instances.
    pub unit: String,
    /// Worktree setup commands are killed after this many seconds.
    pub setup_timeout_seconds: i64,
    /// Secrets file loaded into the environment at startup. Both service
    /// managers use this one file, so no unit ever contains a secret.
    pub env_file: PathBuf,
    /// Optional log file. Daily rotation keeps one week; `None` means the
    /// service manager's own log (journald on Linux).
    pub log_file: Option<PathBuf>,
    /// Hold a power assertion so a dedicated Mac does not sleep.
    pub keep_awake: bool,
}

/// Test and tooling defaults. Production always goes through [`Config::load`],
/// which derives paths and the cookie policy from the environment.
impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/tmp/threadbase"),
            db_path: PathBuf::from("/tmp/threadbase/threadbase.db"),
            bind: "127.0.0.1:7777".parse().expect("valid loopback address"),
            public_origin: None,
            admins: Vec::new(),
            dev_identity: None,
            session_seconds: 30 * 24 * 60 * 60,
            cookie_secure: false,
            log_json: false,
            terminal: false,
            terminal_host: false,
            terminal_host_sockets: Vec::new(),
            terminal_drivers: Vec::new(),
            terminal_shell: "/bin/bash".to_owned(),
            terminal_socket: crate::terminal::SOCKET.to_owned(),
            idle_session_seconds: 15 * 60,
            max_active_turns: 512,
            settle_days: 7,
            permission_mode: PermissionMode::Supervised,
            max_permission_mode: PermissionMode::FullAccess,
            auto_deploy: false,
            auto_deploy_path: None,
            unit: "threadbase".to_owned(),
            setup_timeout_seconds: 15 * 60,
            env_file: PathBuf::from("/tmp/threadbase/env"),
            log_file: None,
            keep_awake: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("THREADBASE_BIND must be a loopback address, got {0}")]
    NonLoopbackBind(String),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("THREADBASE_SESSION_DAYS must be a positive number of days")]
    InvalidSessionDays,
    #[error("cannot determine a data directory for this platform; set THREADBASE_DATA_DIR")]
    NoDataDir,
}

impl Config {
    /// Load `THREADBASE_ENV_FILE` (default `~/.config/threadbase/env`) into the
    /// process environment.
    ///
    /// launchd has no `EnvironmentFile=`, and systemd's is not visible to a
    /// manually started binary, so the file belongs to the process. Call this
    /// once from `main` **before** the async runtime starts: `setenv` is only
    /// sound while the process is single-threaded.
    pub fn load_env_file() -> anyhow::Result<()> {
        let path = env_file_path();
        if !path.exists() {
            return Ok(());
        }
        dotenvy::from_path(&path)
            .map_err(|error| anyhow::anyhow!("read {}: {error}", path.display()))?;
        Ok(())
    }

    pub fn load() -> Result<Self, ConfigError> {
        let data_dir = match std::env::var_os("THREADBASE_DATA_DIR") {
            Some(value) => PathBuf::from(value),
            None => directories::ProjectDirs::from("", "", "threadbase")
                .map(|dirs| dirs.data_dir().to_path_buf())
                .ok_or(ConfigError::NoDataDir)?,
        };
        let bind_raw =
            std::env::var("THREADBASE_BIND").unwrap_or_else(|_| "127.0.0.1:7777".to_owned());
        let bind: SocketAddr = bind_raw
            .parse()
            .map_err(|_| ConfigError::InvalidAddress(bind_raw.clone()))?;
        if !is_loopback(bind.ip()) {
            return Err(ConfigError::NonLoopbackBind(bind.to_string()));
        }
        let db_path = data_dir.join("threadbase.db");
        let public_origin = std::env::var("THREADBASE_PUBLIC_ORIGIN")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty());
        let log_json = std::env::var("THREADBASE_LOG_JSON")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let admins = list_env("THREADBASE_ADMINS");
        let dev_identity = std::env::var("THREADBASE_DEV_IDENTITY")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let session_days = match non_empty("THREADBASE_SESSION_DAYS") {
            Some(value) => value
                .parse::<i64>()
                .ok()
                .filter(|days| *days > 0 && *days <= 3650)
                .ok_or(ConfigError::InvalidSessionDays)?,
            None => 30,
        };
        let cookie_secure = match non_empty("THREADBASE_COOKIE_SECURE") {
            Some(value) => matches!(value.as_str(), "1" | "true" | "yes"),
            None => public_origin
                .as_deref()
                .is_some_and(|origin| origin.starts_with("https://")),
        };
        let terminal = std::env::var("THREADBASE_TERMINAL")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        let terminal_host = std::env::var("THREADBASE_TERMINAL_HOST")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        let mut terminal_host_sockets = list_env("THREADBASE_TERMINAL_HOST_SOCKETS");
        if terminal_host && terminal_host_sockets.is_empty() {
            terminal_host_sockets.push("default".to_owned());
        }
        let mut seen = std::collections::HashSet::new();
        terminal_host_sockets
            .retain(|name| crate::terminal::is_socket_name(name) && seen.insert(name.clone()));
        let terminal_drivers = list_env("THREADBASE_TERMINAL_DRIVERS");
        let terminal_shell = non_empty("THREADBASE_TERMINAL_SHELL")
            .or_else(|| non_empty("SHELL"))
            .unwrap_or_else(|| "/bin/bash".to_owned());
        let terminal_socket = non_empty("THREADBASE_TERMINAL_SOCKET")
            .unwrap_or_else(|| crate::terminal::SOCKET.to_owned());
        let idle_session_seconds = non_empty("THREADBASE_IDLE_SESSION_SECS")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value >= 0)
            .unwrap_or(15 * 60);
        let max_active_turns = non_empty("THREADBASE_MAX_ACTIVE_TURNS")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(512);
        let permission_mode = non_empty("THREADBASE_PERMISSION_MODE")
            .and_then(|value| value.parse::<PermissionMode>().ok())
            .unwrap_or(PermissionMode::Supervised);
        let max_permission_mode = non_empty("THREADBASE_MAX_PERMISSION_MODE")
            .and_then(|value| value.parse::<PermissionMode>().ok())
            .unwrap_or(PermissionMode::FullAccess);
        let auto_deploy = std::env::var("THREADBASE_AUTO_DEPLOY")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        let auto_deploy_path = non_empty("THREADBASE_AUTO_DEPLOY_PATH").map(PathBuf::from);
        let unit = non_empty("THREADBASE_UNIT").unwrap_or_else(|| "threadbase".to_owned());
        let settle_days = non_empty("THREADBASE_SETTLE_DAYS")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value >= 0)
            .unwrap_or(7);
        let setup_timeout_seconds = non_empty("THREADBASE_SETUP_TIMEOUT_SECS")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(15 * 60);
        let env_file = env_file_path();
        let log_file = flag_path("THREADBASE_LOG_FILE").or_else(default_log_file);
        let keep_awake = std::env::var("THREADBASE_KEEP_AWAKE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        Ok(Self {
            data_dir,
            db_path,
            bind,
            public_origin,
            admins,
            dev_identity,
            session_seconds: session_days * 24 * 60 * 60,
            cookie_secure,
            log_json,
            terminal,
            terminal_host,
            terminal_host_sockets,
            terminal_drivers,
            terminal_shell,
            terminal_socket,
            idle_session_seconds,
            max_active_turns,
            settle_days,
            permission_mode,
            max_permission_mode,
            auto_deploy,
            auto_deploy_path,
            unit,
            setup_timeout_seconds,
            env_file,
            log_file,
            keep_awake,
        })
    }

    /// True when `host_header` names this server: loopback names, the bind
    /// address, or the host of `public_origin`. A port is ignored and the
    /// comparison is case-insensitive.
    #[must_use]
    pub fn is_allowed_host(&self, host_header: &str) -> bool {
        let Some(host) = normalized_host_header(host_header) else {
            return false;
        };
        if host == "localhost" {
            return true;
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            return ip == self.bind.ip()
                || ip == IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                || ip == IpAddr::V6(std::net::Ipv6Addr::LOCALHOST);
        }
        self.public_origin
            .as_deref()
            .and_then(|origin| url::Url::parse(origin).ok())
            .and_then(|origin| origin.host_str().and_then(normalize_host))
            .is_some_and(|public_host| public_host == host)
    }
}

/// Home directory without trusting `HOME` alone: service managers set it, but
/// a stripped environment may not.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
}

/// Secrets file, overridable so one checkout can serve several instances.
pub fn env_file_path() -> PathBuf {
    flag_path("THREADBASE_ENV_FILE")
        .or_else(|| home_dir().map(|home| home.join(".config/threadbase/env")))
        .unwrap_or_else(|| PathBuf::from(".config/threadbase/env"))
}

/// journald already owns log files on Linux, so a file is only the default on
/// macOS where launchd only appends to whatever path the plist names.
fn default_log_file() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    Some(home_dir()?.join("Library/Logs/threadbase/threadbase.log"))
}

fn flag_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn list_env(name: &str) -> Vec<String> {
    std::env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

/// Strip the optional port from a Host header, including bracketed IPv6.
fn normalized_host_header(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    match value.parse::<Authority>() {
        Ok(authority) => {
            // A Host header cannot carry userinfo, and accepting it would let
            // `evil.example@localhost` pass as `localhost`.
            if authority.as_str().contains('@') {
                return None;
            }
            normalize_authority(&authority)
        }
        // Unbracketed IPv6 is not a valid Authority but was accepted before.
        Err(_) => Some(value.parse::<IpAddr>().ok()?.to_string()),
    }
}

fn normalize_host(host: &str) -> Option<String> {
    let authority = host.trim().parse::<Authority>().ok()?;
    if authority.as_str().contains('@') {
        return None;
    }
    normalize_authority(&authority)
}

fn normalize_authority(authority: &Authority) -> Option<String> {
    let host = authority.host();
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return None;
    }
    // `Authority::host()` stops at `]`/`:` and ignores trailing junk; reject
    // anything that is not exactly host or host:digits.
    let bracketed = host.contains(':');
    let host_repr = if bracketed {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let raw = authority.as_str();
    let port = authority.port();
    let valid = match raw.strip_prefix(&host_repr) {
        Some("") => port.is_none(),
        Some(rest) => {
            port.is_some()
                && rest.starts_with(':')
                && rest[1..].chars().all(|ch| ch.is_ascii_digit())
        }
        None => false,
    };
    valid.then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_check() {
        assert!(is_loopback("127.0.0.1".parse().unwrap()));
        assert!(is_loopback("::1".parse().unwrap()));
        assert!(!is_loopback("0.0.0.0".parse().unwrap()));
        assert!(!is_loopback("192.168.1.2".parse().unwrap()));
    }

    #[test]
    fn host_header_allowlist() {
        let mut config = Config::default();
        assert!(config.is_allowed_host("127.0.0.1:7777"));
        assert!(config.is_allowed_host("LOCALHOST"));
        assert!(config.is_allowed_host("[::1]:7777"));
        assert!(config.is_allowed_host("::1"));
        assert!(!config.is_allowed_host(""));
        assert!(!config.is_allowed_host("evil.example"));
        assert!(!config.is_allowed_host("127.0.0.2:7777"));
        assert!(!config.is_allowed_host("[::1]junk"));
        assert!(!config.is_allowed_host("[::1]evil:7777"));
        assert!(!config.is_allowed_host("localhost:abc"));

        config.bind = "127.0.0.2:7777".parse().unwrap();
        assert!(config.is_allowed_host("127.0.0.2:7777"));

        config.public_origin = Some("https://machine.example.ts.net:8443".to_owned());
        assert!(config.is_allowed_host("machine.example.ts.net"));
        assert!(config.is_allowed_host("MACHINE.EXAMPLE.TS.NET:443"));
        assert!(!config.is_allowed_host("other.example"));
    }

    /// The session cookie is only `Secure` when the operator named an https
    /// origin — which `threadbase install` derives from Tailscale.
    #[test]
    fn cookie_secure_follows_the_public_origin() {
        let config = Config::default();
        assert!(!config.cookie_secure);
        let config = Config {
            public_origin: Some("https://machine.example.ts.net".to_owned()),
            ..Config::default()
        };
        assert!(config.is_allowed_host("machine.example.ts.net"));
    }
}
