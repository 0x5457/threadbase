use axum::http::uri::Authority;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

/// GitHub OAuth application used by the sign-in flow.
///
/// The flow is the classic OAuth "web application" one: authorize redirect,
/// authorization code exchange, `GET /user`. GitHub Apps are not used: they
/// require an installation step and private keys for no benefit here.
#[derive(Debug, Clone)]
pub struct GitHubOAuth {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub bind: SocketAddr,
    /// Public HTTPS origin, for example the Funnel hostname.
    pub public_origin: Option<String>,
    /// GitHub logins that are always administrators.
    pub admins: Vec<String>,
    /// Optional identity accepted from direct local connections (development).
    pub dev_identity: Option<String>,
    /// `None` when GitHub sign-in is not configured; dev identity still works.
    pub github: Option<GitHubOAuth>,
    /// GitHub logins allowed to sign in even when no account exists yet.
    pub github_allow: Vec<String>,
    /// Session lifetime; sessions slide forward while in use.
    pub session_seconds: i64,
    /// `Secure` cookie flag. Derived from `public_origin`, overridable.
    pub cookie_secure: bool,
    pub log_json: bool,
    /// `THREADBASE_TERMINAL=1` turns on the browser terminal. A shell over HTTP
    /// is remote code execution by design, so it stays off unless asked for.
    pub terminal: bool,
    /// Non-admin GitHub logins that may drive the browser terminal.
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
    /// Worktree setup commands are killed after this many seconds.
    pub setup_timeout_seconds: i64,
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
            github: None,
            github_allow: Vec::new(),
            session_seconds: 30 * 24 * 60 * 60,
            cookie_secure: false,
            log_json: false,
            terminal: false,
            terminal_drivers: Vec::new(),
            terminal_shell: "/bin/bash".to_owned(),
            terminal_socket: crate::terminal::SOCKET.to_owned(),
            idle_session_seconds: 15 * 60,
            max_active_turns: 4,
            setup_timeout_seconds: 15 * 60,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("THREADBASE_BIND must be a loopback address, got {0}")]
    NonLoopbackBind(String),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error(
        "THREADBASE_GITHUB_CLIENT_ID and THREADBASE_GITHUB_CLIENT_SECRET must be set together"
    )]
    GitHubIncomplete,
    #[error("THREADBASE_SESSION_DAYS must be a positive number of days")]
    InvalidSessionDays,
    #[error("cannot determine a data directory for this platform; set THREADBASE_DATA_DIR")]
    NoDataDir,
}

impl Config {
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
        let github_allow = list_env("THREADBASE_GITHUB_ALLOW");
        let dev_identity = std::env::var("THREADBASE_DEV_IDENTITY")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let client_id = non_empty("THREADBASE_GITHUB_CLIENT_ID");
        let client_secret = non_empty("THREADBASE_GITHUB_CLIENT_SECRET");
        let github = match (client_id, client_secret) {
            (Some(client_id), Some(client_secret)) => Some(GitHubOAuth {
                client_id,
                client_secret,
            }),
            (None, None) => None,
            _ => return Err(ConfigError::GitHubIncomplete),
        };
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
            .unwrap_or(4);
        let setup_timeout_seconds = non_empty("THREADBASE_SETUP_TIMEOUT_SECS")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(15 * 60);
        Ok(Self {
            data_dir,
            db_path,
            bind,
            public_origin,
            admins,
            dev_identity,
            github,
            github_allow,
            session_seconds: session_days * 24 * 60 * 60,
            cookie_secure,
            log_json,
            terminal,
            terminal_drivers,
            terminal_shell,
            terminal_socket,
            idle_session_seconds,
            max_active_turns,
            setup_timeout_seconds,
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

    /// OAuth callback registered on the GitHub app.
    #[must_use]
    pub fn callback_url(&self) -> Option<String> {
        self.public_origin
            .as_ref()
            .map(|origin| format!("{origin}{}", crate::auth_github::CALLBACK_PATH))
    }

    /// True when a GitHub sign-in round trip can actually be completed.
    #[must_use]
    pub fn github_ready(&self) -> bool {
        self.github.is_some() && self.callback_url().is_some()
    }
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

    #[test]
    fn callback_url_requires_a_public_origin() {
        let mut config = Config::default();
        assert_eq!(config.callback_url(), None);
        assert!(!config.github_ready());
        config.public_origin = Some("https://machine.example.ts.net".to_owned());
        assert_eq!(
            config.callback_url().as_deref(),
            Some("https://machine.example.ts.net/api/auth/github/callback")
        );
        config.github = Some(GitHubOAuth {
            client_id: "id".to_owned(),
            client_secret: "secret".to_owned(),
        });
        assert!(config.github_ready());
    }
}
