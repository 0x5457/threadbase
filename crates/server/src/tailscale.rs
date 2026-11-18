//! The Tailscale CLI: where the identity comes from, and how the app is exposed.
//!
//! Two jobs, one dependency. `whois` turns a tailnet address into the person
//! behind it, which is the only identity this product has. `serve` is the only
//! supported ingress: the server binds a loopback port and `tailscaled` proxies
//! it inside the tailnet, so there is no public surface and no tunnel to
//! configure.

use crate::install::command;
use std::net::IpAddr;
#[cfg(target_os = "macos")]
use std::path::Path;
use threadbase_agent::profile::find_in_path;

/// HTTPS port `tailscale serve` terminates. 443 keeps the URL a bare hostname.
pub const DEFAULT_HTTPS_PORT: u16 = 443;

/// A tailnet user, as `whois` reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Full argv for `tailscale serve`, including the subcommand.
#[must_use]
pub fn serve_arguments(https_port: u16, port: u16) -> Vec<String> {
    vec![
        "serve".to_owned(),
        "--bg".to_owned(),
        format!("--https={https_port}"),
        format!("http://127.0.0.1:{port}"),
    ]
}

pub fn enable_serve(serve_args: &[String]) -> anyhow::Result<()> {
    let args: Vec<&str> = serve_args.iter().map(String::as_str).collect();
    command::run_quiet(&program(), &args)
}

pub fn disable_serve(https_port: u16) {
    let port = format!("--https={https_port}");
    let _ = command::run_quiet(&program(), &["serve", &port, "off"]);
}

/// The machine's tailnet DNS name, without the trailing dot.
pub fn host() -> Option<String> {
    let status = command::stdout(&program(), &["status", "--json"])?;
    let json: serde_json::Value = serde_json::from_str(&status).ok()?;
    let dns = json["Self"]["DNSName"].as_str()?.trim_end_matches('.');
    (!dns.is_empty()).then(|| dns.to_owned())
}

/// Whether the local tailscaled is up. `serve` and `whois` both need it.
pub fn running() -> bool {
    command::stdout(&program(), &["status", "--json"])
        .and_then(|status| serde_json::from_str::<serde_json::Value>(&status).ok())
        .and_then(|json| json["BackendState"].as_str().map(str::to_owned))
        .is_some_and(|state| state == "Running")
}

/// The person behind a tailnet address.
///
/// This spawns the CLI, so it belongs on the sign-in path and nowhere else. It
/// returns `None` for an address that is not a tailnet peer — which is the
/// answer for a browser reaching the loopback port directly.
#[must_use]
pub fn whois(ip: IpAddr) -> Option<Profile> {
    // `whois` wants a bare address; IPv6 in particular must not carry brackets.
    let status = command::stdout(&program(), &["whois", "--json", &ip.to_string()])?;
    let json: serde_json::Value = serde_json::from_str(&status).ok()?;
    let login = json["UserProfile"]["LoginName"]
        .as_str()
        .map(str::trim)
        .filter(|login| !login.is_empty())?
        .to_owned();
    Some(Profile {
        display_name: json["UserProfile"]["DisplayName"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned),
        avatar_url: json["UserProfile"]["ProfilePicURL"]
            .as_str()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(ToOwned::to_owned),
        login,
    })
}

/// The Tailscale CLI is not always on PATH: the macOS app bundles it inside
/// `Tailscale.app`, and that is the build that can talk to the GUI daemon.
pub(crate) fn program() -> String {
    if let Ok(explicit) = std::env::var("THREADBASE_TAILSCALE") {
        if !explicit.trim().is_empty() {
            return explicit;
        }
    }
    if find_in_path("tailscale").is_some() {
        return "tailscale".to_owned();
    }
    #[cfg(target_os = "macos")]
    {
        const APP_CLI: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
        if Path::new(APP_CLI).exists() {
            return APP_CLI.to_owned();
        }
    }
    "tailscale".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_arguments_are_tailnet_only() {
        assert_eq!(
            serve_arguments(443, 7777),
            vec!["serve", "--bg", "--https=443", "http://127.0.0.1:7777"]
        );
    }

    #[test]
    fn the_cli_falls_back_to_a_bare_name() {
        // Tests never run with THREADBASE_TAILSCALE set, and mutating the
        // process environment is unsafe once the test harness has threads.
        if std::env::var_os("THREADBASE_TAILSCALE").is_none() && find_in_path("tailscale").is_none()
        {
            assert_eq!(program(), "tailscale");
        }
    }
}
