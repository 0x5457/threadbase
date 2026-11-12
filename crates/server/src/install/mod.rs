//! `threadbase install`: configure a systemd user service for the binary that
//! is running right now.
//!
//! The command writes the unit, enables lingering so the service survives
//! logout, starts it, and points a Tailscale endpoint at the loopback port. It
//! never builds, copies or downloads anything: `ExecStart` is the current
//! executable. GitHub OAuth needs a public HTTPS callback, so `--funnel` is what
//! most deployments want; `tailscale serve` stays available for tailnet-only
//! installs.

mod args;
pub(crate) mod command;
mod health;
mod tailscale;
pub(crate) mod unit;

use anyhow::{bail, Context};
use std::path::Path;

pub use args::{parse_args, print_help, Options, DEFAULT_HTTPS_PORT, DEFAULT_PORT};

pub async fn run(options: Options) -> anyhow::Result<()> {
    if cfg!(not(target_os = "linux")) {
        bail!(
            "`threadbase install` targets systemd on Linux. On macOS use \
             `threadbase service install` for a launchd agent."
        );
    }
    let unit_path = unit::path(&options.unit)?;

    if options.uninstall {
        return uninstall(&options, &unit_path).await;
    }

    let exe = unit::current_binary();
    let config = crate::config::Config::load().context("invalid configuration")?;
    let tailnet_host = tailscale::host();
    let public_origin = options.public_origin.clone().or_else(|| {
        tailnet_host
            .as_ref()
            .map(|host| format!("https://{host}:{}", options.https_port))
    });
    let unit = unit::render(&unit::UnitSpec {
        exe: &exe,
        data_dir: &config.data_dir,
        port: options.port,
        public_origin: public_origin.as_deref(),
        admin: options.admin.as_deref(),
        path_env: std::env::var("PATH").ok().as_deref(),
    });
    let serve_args = tailscale::expose_arguments(options.funnel, options.https_port, options.port);

    if options.print_only {
        println!("# {}", unit_path.display());
        print!("{unit}");
        println!();
        if !options.skip_serve {
            println!("# https exposure");
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

    command::ensure_systemctl()?;

    if cfg!(debug_assertions) {
        println!(
            "note: this is a development build; install a release build for a \
             long-running service"
        );
    }

    if std::fs::read_to_string(&unit_path).ok().as_deref() != Some(unit.as_str()) {
        unit::write(&unit_path, &unit)?;
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
    if let Err(error) = command::run_quiet("loginctl", &["enable-linger", &whoami()]) {
        eprintln!("warning: could not enable lingering ({error}); the service stops at logout");
    }

    command::run_checked("systemctl", &["--user", "daemon-reload"])?;
    command::run_checked("systemctl", &["--user", "enable", &options.unit])?;
    command::run_checked("systemctl", &["--user", "restart", &options.unit])?;

    if tailnet_host.is_none() {
        eprintln!("warning: could not read the tailnet DNS name from `tailscale status`");
    }

    if options.skip_serve {
        println!("Skipped `tailscale serve` / `tailscale funnel` (--no-serve).");
    } else {
        match tailscale::enable(&serve_args) {
            Ok(()) => println!(
                "Exposed {}: https://{}:{}",
                if options.funnel {
                    "publicly with Tailscale Funnel"
                } else {
                    "on the tailnet"
                },
                tailnet_host
                    .as_deref()
                    .unwrap_or("<machine>.<tailnet>.ts.net"),
                options.https_port
            ),
            Err(error) => {
                eprintln!("warning: `tailscale {}` failed: {error}", serve_args[0]);
                eprintln!("         run: sudo tailscale {}", serve_args.join(" "));
                if options.funnel {
                    eprintln!(
                        "         Funnel also needs the `funnel` node attribute in the tailscale ACL."
                    );
                }
            }
        }
    }

    health::check(options.port).await?;
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
    println!("Sign in with GitHub. Set THREADBASE_GITHUB_ALLOW to control who may.");
    println!("The first identity to sign in becomes the administrator.");
    println!("Logs: journalctl --user -u {} -f", options.unit);
    println!("Remove: threadbase install --uninstall");
}

async fn uninstall(options: &Options, unit_path: &Path) -> anyhow::Result<()> {
    if options.print_only {
        println!("# would stop and remove {}", options.unit);
        println!("systemctl --user disable --now {}", options.unit);
        println!("rm {}", unit_path.display());
        println!("systemctl --user daemon-reload");
        if !options.skip_serve {
            println!("tailscale funnel --https={} off", options.https_port);
            println!("tailscale serve --https={} off", options.https_port);
        }
        return Ok(());
    }
    command::ensure_systemctl()?;
    let _ = command::run_quiet("systemctl", &["--user", "disable", "--now", &options.unit]);
    if unit_path.exists() {
        std::fs::remove_file(unit_path)
            .with_context(|| format!("remove {}", unit_path.display()))?;
        println!("Removed {}", unit_path.display());
    }
    let _ = command::run_quiet("systemctl", &["--user", "daemon-reload"]);
    if !options.skip_serve {
        tailscale::disable(options.https_port);
        println!(
            "Turned off the Tailscale endpoint on port {}",
            options.https_port
        );
    }
    println!("The binary and data directory were left in place.");
    Ok(())
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
}
