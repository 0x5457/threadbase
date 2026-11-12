//! `threadbase install` option parsing and help text.

use anyhow::{bail, Context};

pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HTTPS_PORT: u16 = 8443;

#[derive(Debug, Clone)]
pub struct Options {
    pub unit: String,
    pub port: u16,
    pub https_port: u16,
    pub admin: Option<String>,
    pub public_origin: Option<String>,
    pub funnel: bool,
    pub skip_serve: bool,
    pub skip_start: bool,
    pub print_only: bool,
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

pub fn parse_args(mut args: impl Iterator<Item = String>) -> anyhow::Result<Options> {
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--print" | "--dry-run" => options.print_only = true,
            "--uninstall" => options.uninstall = true,
            "--no-serve" => options.skip_serve = true,
            "--funnel" => options.funnel = true,
            "--no-start" => options.skip_start = true,
            "--unit" => options.unit = value(&mut args, "--unit")?,
            "--port" => {
                options.port = value(&mut args, "--port")?
                    .parse()
                    .context("--port must be a number")?;
            }
            "--https-port" => {
                options.https_port = value(&mut args, "--https-port")?
                    .parse()
                    .context("--https-port must be a number")?;
            }
            "--admin" => options.admin = Some(value(&mut args, "--admin")?),
            "--public-origin" => options.public_origin = Some(value(&mut args, "--public-origin")?),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown install option `{other}`; run `threadbase install --help`"),
        }
    }
    Ok(options)
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> anyhow::Result<String> {
    args.next()
        .with_context(|| format!("{flag} requires a value"))
}

pub fn print_help() {
    println!(
        "\
Usage: threadbase install [options]

Configure a systemd user service for this binary and expose it over HTTPS.
Nothing is built, copied or downloaded: the unit runs the current executable.

Options:
  --port <port>          Loopback port the server binds (default 7777)
  --https-port <port>    Port the Tailscale endpoint exposes (default 8443)
  --funnel               Expose publicly with `tailscale funnel` (GitHub OAuth
                         needs this); default is tailnet-only `tailscale serve`
  --admin <login>        Pin THREADBASE_ADMINS to this GitHub login
  --public-origin <url>  Pin THREADBASE_PUBLIC_ORIGIN instead of deriving it
  --unit <name>          systemd unit name (default threadbase)
  --no-serve             Do not touch `tailscale serve` / `tailscale funnel`
  --no-start             Write the unit but do not enable or start it
  --print                Print the unit and commands, change nothing
  --uninstall            Stop and remove the unit, turn the endpoint off
  --help                 Print this help

Secrets belong in ~/.config/threadbase/env (the unit loads it with
EnvironmentFile= and never stores them itself):

  THREADBASE_GITHUB_CLIENT_ID=...
  THREADBASE_GITHUB_CLIENT_SECRET=...
  THREADBASE_GITHUB_ALLOW=octocat,ada

The unit is written to ~/.config/systemd/user/<unit>.service and enabled with
lingering so it survives logout and reboot. Data is left untouched."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_parse_flags_and_values() {
        let options = parse_args(
            [
                "--port",
                "7788",
                "--https-port",
                "9443",
                "--admin",
                "octocat",
                "--funnel",
                "--no-serve",
                "--print",
            ]
            .into_iter()
            .map(str::to_owned),
        )
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
    fn unknown_option_is_rejected() {
        assert!(parse_args(["--nope".to_owned()].into_iter()).is_err());
    }
}
