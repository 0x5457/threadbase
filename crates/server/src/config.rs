use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub bind: SocketAddr,
    /// Public origin used in share links, for example https://machine.tailnet.ts.net
    pub public_origin: Option<String>,
    /// Tailscale logins that are always administrators.
    pub tailscale_admins: Vec<String>,
    /// Optional identity accepted from direct local connections (development).
    pub dev_identity: Option<String>,
    pub log_json: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("THREADBASE_BIND must be a loopback address, got {0}")]
    NonLoopbackBind(String),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
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
            .filter(|value| !value.trim().is_empty());
        let log_json = std::env::var("THREADBASE_LOG_JSON")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let tailscale_admins = std::env::var("THREADBASE_TAILSCALE_ADMIN")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let dev_identity = std::env::var("THREADBASE_DEV_IDENTITY")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        Ok(Self {
            data_dir,
            db_path,
            bind,
            public_origin,
            tailscale_admins,
            dev_identity,
            log_json,
        })
    }
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
}
