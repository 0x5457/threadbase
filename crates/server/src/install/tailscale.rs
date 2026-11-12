//! Tailscale serve/funnel lifecycle for the install command.

use crate::install::command;

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
    command::run_quiet("tailscale", &args)
}

pub(crate) fn disable(https_port: u16) {
    let port = format!("--https={https_port}");
    let _ = command::run_quiet("tailscale", &["funnel", &port, "off"]);
    let _ = command::run_quiet("tailscale", &["serve", &port, "off"]);
}

pub(crate) fn host() -> Option<String> {
    let status = command::stdout("tailscale", &["status", "--json"])?;
    let json: serde_json::Value = serde_json::from_str(&status).ok()?;
    let dns = json["Self"]["DNSName"].as_str()?.trim_end_matches('.');
    (!dns.is_empty()).then(|| dns.to_owned())
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
}
