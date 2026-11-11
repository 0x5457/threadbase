//! `threadbase install`: configure a systemd user service for the binary that
//! is running right now.
//!
//! The command writes the unit, enables lingering so the service survives
//! logout, starts it, and points `tailscale serve` at the loopback port. It
//! never builds, copies or downloads anything: `ExecStart` is the current
//! executable.

use anyhow::{bail, Context};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HTTPS_PORT: u16 = 8443;

#[derive(Debug, Clone)]
pub struct Options {
    pub unit: String,
    pub port: u16,
    pub https_port: u16,
    pub admin: Option<String>,
    pub public_origin: Option<String>,
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

Configure a systemd user service for this binary and expose it on the tailnet.
Nothing is built, copied or downloaded: the unit runs the current executable.

Options:
  --port <port>          Loopback port the server binds (default 7777)
  --https-port <port>    Port `tailscale serve` exposes (default 8443)
  --admin <login>        Pin THREADBASE_TAILSCALE_ADMIN to this login
  --public-origin <url>  Pin THREADBASE_PUBLIC_ORIGIN instead of deriving it
  --unit <name>          systemd unit name (default threadbase)
  --no-serve             Do not touch `tailscale serve`
  --no-start             Write the unit but do not enable or start it
  --print                Print the unit and commands, change nothing
  --uninstall            Stop and remove the unit, turn off `tailscale serve`
  --help                 Print this help

The unit is written to ~/.config/systemd/user/<unit>.service and enabled with
lingering so it survives logout and reboot. Data is left untouched."
    );
}

pub async fn run(options: Options) -> anyhow::Result<()> {
    if cfg!(not(target_os = "linux")) {
        bail!(
            "`threadbase install` targets systemd on Linux. On macOS use \
             `threadbase service install` for a launchd agent."
        );
    }
    let home = std::env::var("HOME").context("HOME is not set")?;
    let unit_path = PathBuf::from(&home)
        .join(".config/systemd/user")
        .join(format!("{}.service", options.unit));

    if options.uninstall {
        return uninstall(&options, &unit_path).await;
    }

    let exe = current_binary();
    let config = crate::config::Config::load().context("invalid configuration")?;
    let tailnet_host = tailnet_host();
    let public_origin = options.public_origin.clone().or_else(|| {
        tailnet_host
            .as_ref()
            .map(|host| format!("https://{host}:{}", options.https_port))
    });
    let unit = render_unit(&UnitSpec {
        exe: &exe,
        data_dir: &config.data_dir,
        port: options.port,
        public_origin: public_origin.as_deref(),
        admin: options.admin.as_deref(),
        path_env: std::env::var("PATH").ok().as_deref(),
    });
    let serve_args = serve_arguments(options.https_port, options.port);

    if options.print_only {
        println!("# {}", unit_path.display());
        print!("{unit}");
        println!();
        if !options.skip_serve {
            println!("# tailnet exposure");
            println!("tailscale {}", serve_args.join(" "));
        }
        if !options.skip_start {
            println!();
            println!("# activation");
            println!("loginctl enable-linger {}", whoami());
            println!("systemctl --user daemon-reload");
            println!("systemctl --user enable --now {}", options.unit);
        }
        if let Some(origin) = &public_origin {
            println!();
            println!("# open {origin}");
        }
        return Ok(());
    }

    ensure_systemctl()?;

    if cfg!(debug_assertions) {
        println!(
            "note: this is a development build; install a release build for a \
             long-running service"
        );
    }

    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    if std::fs::read_to_string(&unit_path).ok().as_deref() != Some(unit.as_str()) {
        std::fs::write(&unit_path, &unit)
            .with_context(|| format!("write {}", unit_path.display()))?;
        println!("Wrote {}", unit_path.display());
    } else {
        println!("Unit unchanged: {}", unit_path.display());
    }

    if options.skip_start {
        println!("Skipped activation (--no-start).");
        print_next_steps(&options, &public_origin);
        return Ok(());
    }

    // User services only survive logout with lingering enabled; this is what
    // makes the deployment usable from other tailnet devices.
    if let Err(error) = run_quiet("loginctl", &["enable-linger", &whoami()]) {
        eprintln!("warning: could not enable lingering ({error}); the service stops at logout");
    }

    run_checked("systemctl", &["--user", "daemon-reload"])?;
    run_checked("systemctl", &["--user", "enable", &options.unit])?;
    run_checked("systemctl", &["--user", "restart", &options.unit])?;

    if tailnet_host.is_none() {
        eprintln!("warning: could not read the tailnet DNS name from `tailscale status`");
    }

    if options.skip_serve {
        println!("Skipped `tailscale serve` (--no-serve).");
    } else {
        let serve_refs: Vec<&str> = serve_args.iter().map(String::as_str).collect();
        match run_quiet("tailscale", &serve_refs) {
            Ok(()) => println!(
                "Exposed on the tailnet: https://{}:{}",
                tailnet_host
                    .as_deref()
                    .unwrap_or("<machine>.<tailnet>.ts.net"),
                options.https_port
            ),
            Err(error) => {
                eprintln!("warning: `tailscale serve` failed: {error}");
                eprintln!("         run: sudo tailscale {}", serve_args.join(" "));
            }
        }
    }

    health_check(options.port).await?;
    print_next_steps(&options, &public_origin);
    Ok(())
}

fn print_next_steps(options: &Options, public_origin: &Option<String>) {
    println!();
    match public_origin {
        Some(origin) => println!("Open {origin} on any tailnet device."),
        None => println!(
            "Open the tailnet address on any device, for example \
             https://<machine>.<tailnet>.ts.net:{}",
            options.https_port
        ),
    }
    println!("The first Tailscale identity to sign in becomes the administrator.");
    println!("Logs: journalctl --user -u {} -f", options.unit);
    println!("Remove: threadbase install --uninstall");
}

async fn health_check(port: u16) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    for attempt in 0..20 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                println!("Service is healthy on {url}");
                return Ok(());
            }
            Ok(response) => anyhow::bail!("{url} returned {}", response.status()),
            Err(_) => continue,
        }
    }
    bail!("service did not answer on {url}; check `journalctl --user -u` output")
}

async fn uninstall(options: &Options, unit_path: &Path) -> anyhow::Result<()> {
    if options.print_only {
        println!("# would stop and remove {}", options.unit);
        println!("systemctl --user disable --now {}", options.unit);
        println!("rm {}", unit_path.display());
        println!("systemctl --user daemon-reload");
        if !options.skip_serve {
            println!("tailscale serve --https={} off", options.https_port);
        }
        return Ok(());
    }
    ensure_systemctl()?;
    let _ = run_quiet("systemctl", &["--user", "disable", "--now", &options.unit]);
    if unit_path.exists() {
        std::fs::remove_file(unit_path)
            .with_context(|| format!("remove {}", unit_path.display()))?;
        println!("Removed {}", unit_path.display());
    }
    let _ = run_quiet("systemctl", &["--user", "daemon-reload"]);
    if !options.skip_serve {
        let _ = run_quiet(
            "tailscale",
            &["serve", &format!("--https={}", options.https_port), "off"],
        );
        println!(
            "Turned off `tailscale serve` on port {}",
            options.https_port
        );
    }
    println!("The binary and data directory were left in place.");
    Ok(())
}

struct UnitSpec<'a> {
    exe: &'a Path,
    data_dir: &'a Path,
    port: u16,
    public_origin: Option<&'a str>,
    admin: Option<&'a str>,
    path_env: Option<&'a str>,
}

fn render_unit(spec: &UnitSpec<'_>) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=Threadbase local agent workspace\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n");
    unit.push_str("StartLimitIntervalSec=0\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("ExecStart=\"{}\" serve\n", spec.exe.display()));
    unit.push_str(&format!(
        "Environment=THREADBASE_BIND=127.0.0.1:{}\n",
        spec.port
    ));
    unit.push_str(&format!(
        "Environment=\"THREADBASE_DATA_DIR={}\"\n",
        spec.data_dir.display()
    ));
    if let Some(origin) = spec.public_origin {
        unit.push_str(&format!("Environment=THREADBASE_PUBLIC_ORIGIN={origin}\n"));
    }
    if let Some(admin) = spec.admin {
        unit.push_str(&format!("Environment=THREADBASE_TAILSCALE_ADMIN={admin}\n"));
    }
    // Agent adapters usually live in PATH entries the login shell adds.
    if let Some(path) = spec.path_env {
        let mut seen: Vec<&str> = Vec::new();
        for entry in path.split(':').filter(|entry| !entry.is_empty()) {
            if !seen.contains(&entry) {
                seen.push(entry);
            }
        }
        unit.push_str(&format!("Environment=\"PATH={}\"\n", seen.join(":")));
    }
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=2\n");
    unit.push_str("NoNewPrivileges=yes\n");
    unit.push_str("PrivateTmp=no\n\n");
    unit.push_str("[Install]\n");
    unit.push_str("WantedBy=default.target\n");
    unit
}

/// Full argv for `tailscale`, including the subcommand.
fn serve_arguments(https_port: u16, port: u16) -> Vec<String> {
    vec![
        "serve".to_owned(),
        "--bg".to_owned(),
        format!("--https={https_port}"),
        format!("http://127.0.0.1:{port}"),
    ]
}

/// The path this process was started from, so the unit points at the same file
/// the caller used rather than a resolved build artifact.
fn current_binary() -> PathBuf {
    let argv0 = std::env::args().next().unwrap_or_default();
    if argv0.contains('/') {
        let path = PathBuf::from(&argv0);
        if path.is_absolute() {
            return normalize(&path);
        }
        if let Ok(cwd) = std::env::current_dir() {
            return normalize(&cwd.join(path));
        }
    } else if !argv0.is_empty() {
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                let candidate = Path::new(dir).join(&argv0);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("threadbase"))
}

/// Lexical cleanup so the unit reads `/opt/bin/threadbase`, not `/opt/./bin/x`.
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::RootDir | Component::Prefix(_) => {}
            Component::Normal(part) => parts.push(part.to_owned()),
        }
    }
    let mut output = PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
    for part in parts {
        output.push(part);
    }
    output
}

fn ensure_systemctl() -> anyhow::Result<()> {
    if Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    bail!(
        "`systemctl --user` is unavailable. This usually means systemd is not \
         running or XDG_RUNTIME_DIR is unset; run install from a normal login \
         session on a systemd host."
    )
}

fn run_checked(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("spawn {program}"))?;
    if !status.success() {
        bail!("`{program} {}` failed", args.join(" "));
    }
    Ok(())
}

fn run_quiet(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("spawn {program}"))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn tailnet_host() -> Option<String> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let dns = json["Self"]["DNSName"].as_str()?.trim_end_matches('.');
    (!dns.is_empty()).then(|| dns.to_owned())
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>() -> UnitSpec<'a> {
        UnitSpec {
            exe: Path::new("/home/ada/.local/bin/threadbase"),
            data_dir: Path::new("/home/ada/.local/share/threadbase"),
            port: 7777,
            public_origin: Some("https://machine.tailnet.ts.net:8443"),
            admin: Some("ada@example.com"),
            path_env: Some("/usr/local/bin:/usr/bin:/usr/local/bin"),
        }
    }

    #[test]
    fn unit_runs_the_current_binary_and_binds_loopback() {
        let unit = render_unit(&spec());
        assert!(unit.contains("ExecStart=\"/home/ada/.local/bin/threadbase\" serve"));
        assert!(unit.contains("Environment=THREADBASE_BIND=127.0.0.1:7777"));
        assert!(unit.contains("THREADBASE_DATA_DIR=/home/ada/.local/share/threadbase"));
        assert!(unit.contains("THREADBASE_PUBLIC_ORIGIN=https://machine.tailnet.ts.net:8443"));
        assert!(unit.contains("THREADBASE_TAILSCALE_ADMIN=ada@example.com"));
        assert!(unit.contains("\"PATH=/usr/local/bin:/usr/bin\""), "{unit}");
        assert!(unit.contains("WantedBy=default.target"));
        assert!(
            !unit.contains("0.0.0.0"),
            "unit must never expose a public bind"
        );
    }

    #[test]
    fn unit_omits_optional_environment() {
        let unit = render_unit(&UnitSpec {
            exe: Path::new("/bin/threadbase"),
            data_dir: Path::new("/tmp/data"),
            port: 7788,
            public_origin: None,
            admin: None,
            path_env: None,
        });
        assert!(!unit.contains("THREADBASE_PUBLIC_ORIGIN"));
        assert!(!unit.contains("THREADBASE_TAILSCALE_ADMIN"));
        assert!(!unit.contains("PATH="));
        assert!(unit.contains("THREADBASE_BIND=127.0.0.1:7788"));
    }

    #[test]
    fn serve_arguments_expose_the_loopback_port() {
        assert_eq!(
            serve_arguments(8443, 7777),
            vec!["serve", "--bg", "--https=8443", "http://127.0.0.1:7777"]
        );
    }

    #[test]
    fn arguments_parse_flags_and_values() {
        let options = parse_args(
            [
                "--port",
                "7788",
                "--https-port",
                "9443",
                "--admin",
                "ada@example.com",
                "--no-serve",
                "--print",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(options.port, 7788);
        assert_eq!(options.https_port, 9443);
        assert_eq!(options.admin.as_deref(), Some("ada@example.com"));
        assert!(options.skip_serve);
        assert!(options.print_only);
        assert!(!options.uninstall);
    }

    #[test]
    fn unknown_option_is_rejected() {
        assert!(parse_args(["--nope".to_owned()].into_iter()).is_err());
    }

    #[test]
    fn paths_are_normalized() {
        assert_eq!(
            normalize(Path::new("/home/ada/./bin/../bin/threadbase")),
            PathBuf::from("/home/ada/bin/threadbase")
        );
    }
}
