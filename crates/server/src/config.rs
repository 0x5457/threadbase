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
        })
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
