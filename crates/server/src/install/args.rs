//! `threadbase install` options and help text.

use clap::Parser;

pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HTTPS_PORT: u16 = 8443;

const INSTALL_AFTER_HELP: &str = "\
Secrets belong in ~/.config/threadbase/env (the unit loads it with
EnvironmentFile= and never stores them itself):

  THREADBASE_GITHUB_CLIENT_ID=...
  THREADBASE_GITHUB_CLIENT_SECRET=...
  THREADBASE_GITHUB_ALLOW=octocat,ada

The unit is written to ~/.config/systemd/user/<unit>.service and enabled with
lingering so it survives logout and reboot. Data is left untouched.";

#[derive(Debug, Clone, clap::Args)]
#[command(after_help = INSTALL_AFTER_HELP)]
pub struct Options {
    /// systemd unit name (default: threadbase)
    #[arg(long, default_value = "threadbase")]
    pub unit: String,
    /// Loopback port the server binds
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    /// Port the Tailscale endpoint exposes
    #[arg(long, default_value_t = DEFAULT_HTTPS_PORT)]
    pub https_port: u16,
    /// Pin THREADBASE_ADMINS to this GitHub login
    #[arg(long)]
    pub admin: Option<String>,
    /// Pin THREADBASE_PUBLIC_ORIGIN instead of deriving it
    #[arg(long)]
    pub public_origin: Option<String>,
    /// Expose publicly with `tailscale funnel` (GitHub OAuth needs this);
    /// default is tailnet-only `tailscale serve`
    #[arg(long)]
    pub funnel: bool,
    /// Do not touch `tailscale serve` / `tailscale funnel`
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
            funnel: false,
            skip_serve: false,
            skip_start: false,
            print_only: false,
            uninstall: false,
        }
    }
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
            "--funnel",
            "--no-serve",
            "--print",
        ])
        .unwrap();
        assert_eq!(options.port, 7788);
        assert_eq!(options.https_port, 9443);
        assert_eq!(options.admin.as_deref(), Some("octocat"));
        assert!(options.funnel);
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
}
