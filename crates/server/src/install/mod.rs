//! `threadbase install`: turn the running binary into a user service.
//!
//! Lifecycle — write, enable, start, stop, status, uninstall — is delegated to
//! the `service-manager` crate, which speaks launchd, systemd and friends
//! behind one API. Only the *contents* of the service file live here: the
//! crate's templates cannot express `KillMode=process` / `AbandonProcessGroup`
//! (tmux servers must survive a restart, or the browser terminal drops on every
//! deploy), a secrets file instead of inline values, or log destinations.

mod args;
pub(crate) mod command;
pub(crate) mod health;
mod launchd;
mod spec;
mod systemd;

use crate::tailscale;
use anyhow::{bail, Context};
use service_manager::{
    RestartPolicy, ServiceInstallCtx, ServiceLabel, ServiceLevel, ServiceManager,
    ServiceManagerKind, ServiceStartCtx, ServiceStatus, ServiceStatusCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::path::Path;
use std::time::Duration;

pub use args::{normalize_label, parse_args, Options, DEFAULT_HTTPS_PORT, DEFAULT_PORT};
pub(crate) use spec::{Artifact, ServiceSpec};

/// Which backend matches the contents we render. `service-manager` also
/// supports OpenRC and Windows, but a systemd unit means systemd.
pub(crate) fn platform() -> ServiceManagerKind {
    if cfg!(target_os = "macos") {
        ServiceManagerKind::Launchd
    } else {
        ServiceManagerKind::Systemd
    }
}

pub(crate) fn kind_name() -> &'static str {
    match platform() {
        ServiceManagerKind::Launchd => "launchd",
        _ => "systemd",
    }
}

/// A user-level manager, ready for install/start/stop/status/uninstall.
pub(crate) fn manager() -> anyhow::Result<Box<dyn ServiceManager>> {
    let mut manager = <dyn ServiceManager>::target(platform());
    if !manager.available().unwrap_or(false) {
        bail!("{} is not available on this host", kind_name());
    }
    manager.set_level(ServiceLevel::User)?;
    Ok(manager)
}

/// One name for the systemd unit and the launchd label. No dots: the crate
/// splits a dotted label into organization + application and renames the unit
/// on Linux, which would make `--print` lie.
pub(crate) fn label(name: &str) -> ServiceLabel {
    ServiceLabel {
        qualifier: None,
        organization: None,
        application: name.to_owned(),
    }
}

/// The one file the service needs; `service-manager` writes it from `contents`.
pub(crate) fn artifact(spec: &ServiceSpec<'_>) -> Artifact {
    match platform() {
        ServiceManagerKind::Launchd => Artifact {
            path: launchd::plist_path(spec.home, spec.label),
            contents: launchd::plist_file(spec),
        },
        _ => Artifact {
            path: systemd::unit_path(spec.label),
            contents: systemd::unit_file(spec),
        },
    }
}

pub async fn run(options: Options) -> anyhow::Result<()> {
    if cfg!(not(any(target_os = "linux", target_os = "macos"))) {
        bail!("`threadbase install` supports Linux (systemd) and macOS (launchd)");
    }
    let label_name = normalize_label(&options.unit)?;
    let home = spec::home()?;
    let exe = spec::current_binary();
    let config = crate::config::Config::load().context("invalid configuration")?;
    // Tailscale is the only ingress, so the tailnet name is the origin unless the
    // environment already names one (a hand-written unit, or `--no-serve` behind
    // the operator's own proxy).
    let tailnet_host = if options.skip_serve {
        None
    } else {
        tailscale::host()
    };
    let public_origin = config
        .public_origin
        .clone()
        .or_else(|| {
            tailnet_host
                .as_deref()
                .map(|host| origin(host, options.https_port))
        })
        .or_else(|| options.public_origin.clone());
    let path_env = std::env::var("PATH").ok();
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| value.starts_with('/'));
    let spec = ServiceSpec {
        label: &label_name,
        exe: &exe,
        home: &home,
        data_dir: &config.data_dir,
        port: options.port,
        public_origin: public_origin.as_deref(),
        admin: options.admin.as_deref(),
        path_env: path_env.as_deref(),
        env_file: &config.env_file,
        shell: shell.as_deref(),
        keep_awake: options.keep_awake,
    };
    let artifact = artifact(&spec);
    let serve_args = tailscale::serve_arguments(options.https_port, options.port);

    if options.uninstall {
        return uninstall(&spec, &artifact, &options);
    }

    if options.print_only {
        print_plan(&spec, &artifact, &serve_args, &options, &public_origin);
        return Ok(());
    }

    for note in notes(&spec) {
        eprintln!("note: {note}");
    }
    let unchanged = spec::write(&artifact)?;
    println!(
        "{} {}",
        if unchanged { "Unchanged" } else { "Wrote" },
        artifact.path.display()
    );

    if options.skip_start {
        println!("Skipped activation (--no-start).");
        print_next_steps(&spec, &public_origin, &options);
        return Ok(());
    }

    if cfg!(debug_assertions) {
        println!(
            "note: this is a development build; install a release build for a \
             long-running service"
        );
    }
    if cfg!(target_os = "macos") {
        launchd::prepare(&home)?;
    }
    let manager = manager()?;
    install_unit(&*manager, &label_name, &exe, &artifact)?;
    manager
        .start(ServiceStartCtx {
            label: label(&label_name),
        })
        .with_context(|| format!("start {label_name} with {}", kind_name()))?;

    if !options.skip_serve && tailnet_host.is_none() {
        eprintln!(
            "warning: could not read the tailnet DNS name from `{} status`",
            tailscale::program()
        );
    }

    if options.skip_serve {
        println!("Skipped `tailscale serve` (--no-serve).");
    } else {
        match tailscale::enable_serve(&serve_args) {
            Ok(()) => println!(
                "Exposed on the tailnet: https://{}:{}",
                tailnet_host
                    .as_deref()
                    .unwrap_or("<machine>.<tailnet>.ts.net"),
                options.https_port
            ),
            Err(error) => {
                eprintln!("warning: `tailscale serve` failed: {error}");
                eprintln!(
                    "         run: sudo {} {}",
                    tailscale::program(),
                    serve_args.join(" ")
                );
            }
        }
    }

    health::check(options.port, &logs_hint(&spec)).await?;
    print_next_steps(&spec, &public_origin, &options);
    Ok(())
}

/// `https://host` for the default port, with the port only when it is not 443.
pub(crate) fn origin(host: &str, https_port: u16) -> String {
    if https_port == 443 {
        format!("https://{host}")
    } else {
        format!("https://{host}:{https_port}")
    }
}

fn install_unit(
    manager: &dyn ServiceManager,
    name: &str,
    exe: &Path,
    artifact: &Artifact,
) -> anyhow::Result<()> {
    manager
        .install(ServiceInstallCtx {
            label: label(name),
            program: exe.to_path_buf(),
            args: vec!["serve".into()],
            contents: Some(artifact.contents.clone()),
            username: None,
            working_directory: None,
            environment: None,
            autostart: true,
            restart_policy: RestartPolicy::default(),
        })
        .with_context(|| format!("install the {} unit", kind_name()))?;
    // The crate never reloads the systemd manager, so a changed unit would keep
    // running the old definition until something else did.
    if cfg!(target_os = "linux") {
        command::run_checked("systemctl", &["--user", "daemon-reload"])?;
    }
    Ok(())
}

fn state(manager: &dyn ServiceManager, name: &str) -> anyhow::Result<ServiceStatus> {
    manager
        .status(ServiceStatusCtx { label: label(name) })
        .with_context(|| format!("query {} status for {name}", kind_name()))
}

pub(crate) fn start(name: &str) -> anyhow::Result<()> {
    manager()?
        .start(ServiceStartCtx { label: label(name) })
        .with_context(|| format!("start {name} with {}", kind_name()))
}

pub(crate) fn stop(name: &str) -> anyhow::Result<()> {
    manager()?
        .stop(ServiceStopCtx { label: label(name) })
        .with_context(|| format!("stop {name} with {}", kind_name()))
}

/// Restart the installed service: `deploy.sh` and `threadbase upgrade` both use
/// this instead of knowing about systemctl or launchctl.
pub(crate) async fn restart(name: &str) -> anyhow::Result<()> {
    let manager = manager()?;
    manager
        .stop(ServiceStopCtx { label: label(name) })
        .with_context(|| format!("stop {name} with {}", kind_name()))?;
    // Do not race the shutdown: starting while the old process still holds the
    // port makes the new one exit immediately.
    for _ in 0..40 {
        if !matches!(state(&*manager, name), Ok(ServiceStatus::Running)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    manager
        .start(ServiceStartCtx { label: label(name) })
        .with_context(|| format!("start {name} with {}", kind_name()))?;
    Ok(())
}

pub(crate) async fn service_state(name: &str) -> anyhow::Result<ServiceStatus> {
    state(&*manager()?, name)
}

/// Log lines for humans. `service-manager` has no log API, so this is the one
/// place that knows about journald versus files.
pub(crate) fn logs(name: &str, log_file: Option<&Path>, lines: usize) -> anyhow::Result<()> {
    if let Some(path) = log_file.filter(|path| path.exists()) {
        return command::tail(path, lines);
    }
    if cfg!(target_os = "macos") {
        let Some(home) = crate::config::home_dir() else {
            bail!("cannot determine the home directory; set HOME");
        };
        let mut found = false;
        for path in launchd::log_paths(&home)
            .into_iter()
            .filter(|path| path.exists())
        {
            found = true;
            command::tail(&path, lines)?;
        }
        if !found {
            bail!("no logs yet for {name}; it may still be starting");
        }
        return Ok(());
    }
    command::run_checked(
        "journalctl",
        &["--user", "-u", name, "-n", &lines.to_string(), "--no-pager"],
    )
}

fn notes(spec: &ServiceSpec<'_>) -> Vec<String> {
    if cfg!(target_os = "macos") {
        launchd::notes(spec)
    } else {
        systemd::notes(spec)
    }
}

fn activation_lines(spec: &ServiceSpec<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    if cfg!(target_os = "linux") {
        lines.push(format!("loginctl enable-linger {}", whoami()));
    }
    lines.extend(if cfg!(target_os = "macos") {
        launchd::activation_lines(spec)
    } else {
        systemd::activation_lines(spec)
    });
    lines
}

fn uninstall_lines(spec: &ServiceSpec<'_>) -> Vec<String> {
    if cfg!(target_os = "macos") {
        launchd::uninstall_lines(spec)
    } else {
        systemd::uninstall_lines(spec)
    }
}

fn logs_hint(spec: &ServiceSpec<'_>) -> String {
    format!("threadbase service logs --unit {}", spec.label)
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn print_plan(
    spec: &ServiceSpec<'_>,
    artifact: &Artifact,
    serve_args: &[String],
    options: &Options,
    public_origin: &Option<String>,
) {
    println!("# {}", artifact.path.display());
    print!("{}", artifact.contents);
    println!();
    if !options.skip_serve {
        println!("# https exposure");
        println!("{} {}", tailscale::program(), serve_args.join(" "));
    }
    if !options.skip_start {
        println!();
        println!("# activation ({})", kind_name());
        for line in activation_lines(spec) {
            println!("{line}");
        }
    }
    println!();
    println!("# removal");
    for line in uninstall_lines(spec) {
        println!("{line}");
    }
    if let Some(origin) = public_origin {
        println!();
        println!("# open {origin}");
    }
}

fn print_next_steps(spec: &ServiceSpec<'_>, public_origin: &Option<String>, options: &Options) {
    println!();
    match public_origin {
        Some(origin) => println!("Open {origin} on any device."),
        None => println!(
            "Open the URL your tunnel exposes (Tailscale uses port {}).",
            options.https_port
        ),
    }
    println!("Sign in with GitHub. Set THREADBASE_GITHUB_ALLOW to control who may.");
    println!("The first identity to sign in becomes the administrator.");
    println!("Logs: {}", logs_hint(spec));
    println!("Restart: threadbase service restart --unit {}", spec.label);
    println!(
        "Remove: threadbase install --uninstall --unit {}",
        spec.label
    );
}

fn uninstall(spec: &ServiceSpec<'_>, artifact: &Artifact, options: &Options) -> anyhow::Result<()> {
    if options.print_only {
        println!("# would remove {}", options.unit);
        for line in uninstall_lines(spec) {
            println!("{line}");
        }
        if !options.skip_serve {
            println!(
                "{} serve --https={} off",
                tailscale::program(),
                options.https_port
            );
            println!(
                "{} funnel --https={} off",
                tailscale::program(),
                options.https_port
            );
        }
        return Ok(());
    }
    let manager = manager()?;
    match state(&*manager, spec.label)? {
        ServiceStatus::NotInstalled => println!("Not installed: {}", artifact.path.display()),
        _ => {
            manager
                .uninstall(ServiceUninstallCtx {
                    label: label(spec.label),
                })
                .with_context(|| format!("remove {} with {}", spec.label, kind_name()))?;
            println!("Removed {}", artifact.path.display());
        }
    }
    if cfg!(target_os = "linux") {
        let _ = command::run_checked("systemctl", &["--user", "daemon-reload"]);
    }
    if !options.skip_serve {
        tailscale::disable_serve(options.https_port);
        println!(
            "Turned off the Tailscale endpoint on port {}",
            options.https_port
        );
    }
    println!("The binary and data directory were left in place.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_omits_the_default_https_port() {
        assert_eq!(origin("m.tailnet.ts.net", 443), "https://m.tailnet.ts.net");
        assert_eq!(
            origin("m.tailnet.ts.net", 8443),
            "https://m.tailnet.ts.net:8443"
        );
    }

    #[test]
    fn the_platform_backend_matches_the_rendered_contents() {
        let spec = ServiceSpec {
            label: "threadbase",
            exe: Path::new("/Users/ada/.local/bin/threadbase"),
            home: Path::new("/Users/ada"),
            data_dir: Path::new("/Users/ada/Library/Application Support/threadbase"),
            port: 7777,
            public_origin: None,
            admin: None,
            path_env: None,
            env_file: Path::new("/Users/ada/.config/threadbase/env"),
            shell: None,
            keep_awake: false,
        };
        let artifact = artifact(&spec);
        assert!(artifact.path.ends_with(format!(
            "threadbase.{}",
            if cfg!(target_os = "macos") {
                "plist"
            } else {
                "service"
            }
        )));
        assert!(!artifact.contents.contains("CLIENT_SECRET"));
    }

    #[test]
    fn labels_are_plain_application_names() {
        let label = label("threadbase");
        assert_eq!(label.to_qualified_name(), "threadbase");
        assert_eq!(label.to_script_name(), "threadbase");
    }
}
