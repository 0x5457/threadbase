//! `threadbase install` options and help text.

use clap::Parser;

pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HTTPS_PORT: u16 = 8443;

const INSTALL_AFTER_HELP: &str = "\
Runs the current binary as a user service: a systemd unit on Linux, a launchd
agent on macOS. Nothing is built, copied or downloaded.

Secrets belong in ~/.config/threadbase/env (or THREADBASE_ENV_FILE). The
service file only names that path; the binary loads it at startup:

  THREADBASE_ADMINS=you@example.com
  THREADBASE_TERMINAL_DRIVERS=teammate@example.com
  THREADBASE_WEBHOOK_URL=https://example.com/hook

The unit is written to ~/.config/systemd/user/<unit>.service or
~/Library/LaunchAgents/<unit>.plist. Data is left untouched.";

#[derive(Debug, Clone, clap::Args)]
#[command(after_help = INSTALL_AFTER_HELP)]
pub struct Options {
    /// Service unit / launchd label (default: threadbase)
    #[arg(long, default_value = "threadbase")]
    pub unit: String,
    /// Loopback port the server binds
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    /// Port the Tailscale endpoint exposes
    #[arg(long, default_value_t = DEFAULT_HTTPS_PORT)]
    pub https_port: u16,
    /// Pin THREADBASE_ADMINS to this tailnet login
    #[arg(long)]
    pub admin: Option<String>,
    /// Pin THREADBASE_PUBLIC_ORIGIN instead of deriving it
    #[arg(long)]
    pub public_origin: Option<String>,
    /// Keep a dedicated host awake (power assertion; macOS)
    #[arg(long = "keep-awake")]
    pub keep_awake: bool,
    /// Do not touch `tailscale serve` / `tailscale funnel`; use this when
    /// another process owns the public endpoint, and pass --public-origin
    #[arg(long = "no-serve")]
    pub skip_serve: bool,
    /// Write the unit but do not enable or start it
    #[arg(long = "no-start")]
    pub skip_start: bool,
    /// Print the unit and commands, change nothing
    #[arg(long = "print", visible_alias = "dry-run")]
    pub print_only: bool,
    /// Stop and remove the unit, turn the endpoint off
    #[arg(long)]
    pub uninstall: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            unit: "threadbase".to_owned(),
            port: DEFAULT_PORT,
            https_port: DEFAULT_HTTPS_PORT,
            admin: None,
            public_origin: None,
            keep_awake: false,
            skip_serve: false,
            skip_start: false,
            print_only: false,
            uninstall: false,
        }
    }
}

/// One name for the systemd unit and the launchd label. Accepts the decorated
/// forms people already have in scripts (`threadbase.service`) and rejects path
/// separators, because the name becomes a file name.
pub fn normalize_label(raw: &str) -> anyhow::Result<String> {
    let label = raw
        .trim()
        .trim_end_matches(".service")
        .trim_end_matches(".plist")
        .trim();
    if label.is_empty() {
        anyhow::bail!("unit name cannot be empty");
    }
    // No dots: `service-manager` splits a dotted label into organization +
    // application and would rename the Linux unit behind our back.
    if !label
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        anyhow::bail!("invalid unit name {raw:?}: use letters, digits, '_' or '-'");
    }
    Ok(label.to_owned())
}

#[derive(Debug, Parser)]
#[command(name = "threadbase install", after_help = INSTALL_AFTER_HELP)]
struct InstallCli {
    #[command(flatten)]
    options: Options,
}

/// Parse install arguments. The binary uses the clap subcommand directly; this
/// helper keeps the option set independently testable.
pub fn parse_args(args: impl Iterator<Item = String>) -> anyhow::Result<Options> {
    let argv = std::iter::once("threadbase install".to_owned()).chain(args);
    InstallCli::try_parse_from(argv)
        .map(|cli| cli.options)
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> anyhow::Result<Options> {
        parse_args(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn arguments_parse_flags_and_values() {
        let options = parse(&[
            "--port",
            "7788",
            "--https-port",
            "9443",
            "--admin",
            "octocat",
            "--keep-awake",
            "--no-serve",
            "--print",
        ])
        .unwrap();
        assert_eq!(options.port, 7788);
        assert_eq!(options.https_port, 9443);
        assert_eq!(options.admin.as_deref(), Some("octocat"));
        assert!(options.keep_awake);
        assert!(options.skip_serve);
        assert!(options.print_only);
        assert!(!options.uninstall);
    }

    #[test]
    fn dry_run_is_an_alias_for_print() {
        let options = parse(&["--dry-run"]).unwrap();
        assert!(options.print_only);
    }

    #[test]
    fn unknown_option_is_rejected() {
        assert!(parse(&["--nope"]).is_err());
    }

    #[test]
    fn labels_are_one_name_for_both_managers() {
        assert_eq!(normalize_label("threadbase").unwrap(), "threadbase");
        assert_eq!(normalize_label("threadbase.service").unwrap(), "threadbase");
        assert_eq!(
            normalize_label("threadbase-dev.plist").unwrap(),
            "threadbase-dev"
        );
        assert!(normalize_label("../../etc/passwd").is_err());
        assert!(normalize_label("  ").is_err());
        // Dotted labels are re-split by service-manager into org + app.
        assert!(normalize_label("dev.threadbase").is_err());
    }
}
