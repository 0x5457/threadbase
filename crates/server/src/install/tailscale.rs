//! Tailscale serve/funnel lifecycle for the deployment commands.

use crate::install::command;
#[cfg(target_os = "macos")]
use std::path::Path;
use threadbase_agent::profile::find_in_path;

/// Full argv for `tailscale`, including the subcommand.
pub(crate) fn expose_arguments(funnel: bool, https_port: u16, port: u16) -> Vec<String> {
    vec![
        if funnel { "funnel" } else { "serve" }.to_owned(),
        "--bg".to_owned(),
        format!("--https={https_port}"),
        format!("http://127.0.0.1:{port}"),
    ]
}

pub(crate) fn enable(serve_args: &[String]) -> anyhow::Result<()> {
    let args: Vec<&str> = serve_args.iter().map(String::as_str).collect();
    command::run_quiet(&program(), &args)
}

pub(crate) fn disable(https_port: u16) {
    let port = format!("--https={https_port}");
    let _ = command::run_quiet(&program(), &["funnel", &port, "off"]);
    let _ = command::run_quiet(&program(), &["serve", &port, "off"]);
}

pub(crate) fn host() -> Option<String> {
    let status = command::stdout(&program(), &["status", "--json"])?;
    let json: serde_json::Value = serde_json::from_str(&status).ok()?;
    let dns = json["Self"]["DNSName"].as_str()?.trim_end_matches('.');
    (!dns.is_empty()).then(|| dns.to_owned())
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
    fn expose_arguments_cover_serve_and_funnel() {
        assert_eq!(
            expose_arguments(false, 8443, 7777),
            vec!["serve", "--bg", "--https=8443", "http://127.0.0.1:7777"]
        );
        assert_eq!(
            expose_arguments(true, 443, 7777),
            vec!["funnel", "--bg", "--https=443", "http://127.0.0.1:7777"]
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
