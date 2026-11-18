use anyhow::Context;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use threadbase_server::app::AppState;
use threadbase_server::config::Config;
use threadbase_server::{
    db, doctor, install, logging, maintenance, power, routes, service, upgrade,
};

const ENV_HELP: &str = "\
Environment:
  THREADBASE_DATA_DIR          Data directory (default: platform data dir)
  THREADBASE_BIND              Loopback address (default: 127.0.0.1:7777)
  THREADBASE_ENV_FILE          Secrets file (default: ~/.config/threadbase/env)
  THREADBASE_LOG_FILE          Also write logs to this file (daily rotation, 7 kept)
  THREADBASE_KEEP_AWAKE        Hold a power assertion so the host cannot idle-sleep
  THREADBASE_PUBLIC_ORIGIN     HTTPS origin the tailnet reaches this service at
  THREADBASE_ADMINS            Comma-separated tailnet logins that are always admins
  THREADBASE_TERMINAL_DRIVERS  Tailnet logins allowed to drive the browser terminal
  THREADBASE_AUTO_DEPLOY       Set to 1 to install a staged binary when idle
  THREADBASE_AUTO_DEPLOY_PATH  Where a staged binary is expected
  THREADBASE_PERMISSION_MODE   Default permission mode (supervised/auto_edit/full_access)
  THREADBASE_MAX_PERMISSION_MODE  Ceiling no thread may exceed
  THREADBASE_SETTLE_DAYS       Idle threads settle after N days (default 7, 0=off)
  THREADBASE_IDLE_SESSION_SECS Idle agent sessions shut down after N seconds (default 900, 0=off)
  THREADBASE_MAX_ACTIVE_TURNS  Concurrent agent turns across all threads (default 512, 0=unlimited)
  THREADBASE_SETUP_TIMEOUT_SECS Worktree setup command timeout (default 900)
  THREADBASE_SESSION_DAYS      Session lifetime in days (default 30)
  THREADBASE_COOKIE_SECURE     Force the Secure cookie flag on or off
  THREADBASE_DEV_IDENTITY      Identity accepted from direct local connections
                               (development only; ignored behind `tailscale serve`)
  THREADBASE_LOG_JSON          Set to 1 for JSON logs
  THREADBASE_ACP_COMMAND       Override the ACP adapter command (development)
  THREADBASE_TAILSCALE         Path to the Tailscale CLI (default: PATH, then the macOS app)
  THREADBASE_RELEASE_BASE      Override the release download base URL";

#[derive(Debug, Parser)]
#[command(
    name = "threadbase",
    version,
    about = "Threadbase - shared coding agent threads for small teams",
    after_help = ENV_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the local server (default)
    Serve,
    /// Check the environment and agent adapters
    Doctor {
        /// Print the report as JSON
        #[arg(long)]
        json: bool,
    },
    /// Download and verify the latest release
    Upgrade {
        /// Only check for a newer release
        #[arg(long)]
        check: bool,
    },
    /// One-command deployment plus an HTTPS tunnel
    ///
    /// Configure a user service for this binary (systemd or launchd) and expose
    /// it over HTTPS. Nothing is built, copied or downloaded: the unit runs the
    /// current executable.
    Install(install::Options),
    /// Control the installed user service
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
        /// Service unit / launchd label
        #[arg(long, default_value = "threadbase", global = true)]
        unit: String,
    },
    /// Fold superseded stream rows in the event log and reclaim disk
    ///
    /// Rewrites the stored log with the same fold a cold page uses, so replays
    /// show the same transcript with far fewer rows. Safe while the server is
    /// running; pass --vacuum with the service stopped to rebuild the file.
    Compact {
        /// Print what would be removed without writing
        #[arg(long)]
        dry_run: bool,
        /// Rebuild the database file to reclaim the removed rows
        #[arg(long)]
        vacuum: bool,
    },
    /// Rebuild the full-text search index
    Reindex,
    /// Print the lifecycle state machines as text or JSON
    ///
    /// The same catalogue the web client and GET /api/machines consume, so an
    /// agent can ask what can happen instead of grepping the source.
    Machines {
        /// Print JSON instead of the readable outline
        #[arg(long)]
        json: bool,
    },
    /// Print the version
    #[command(hide = true)]
    Version,
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    /// Start the service that `threadbase install` created
    Start,
    /// Stop it (stops the tunnel's upstream, not the tunnel)
    Stop,
    /// Restart it
    Restart,
    /// Report whether the service and the health endpoint answer
    Status,
    /// Print recent log lines
    Logs {
        /// Number of lines
        #[arg(short = 'n', long, default_value_t = 80)]
        lines: usize,
    },
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let success = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            if let Err(write_error) = error.print() {
                eprintln!("threadbase: {write_error}");
                return ExitCode::FAILURE;
            }
            return if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
    };
    // The env file is loaded before the runtime starts: `setenv` is only sound
    // while the process is single-threaded.
    if let Err(error) = Config::load_env_file() {
        eprintln!("threadbase: {error:#}");
        return ExitCode::FAILURE;
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("threadbase: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("threadbase: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Some(Command::Version) => {
            println!("threadbase {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(Command::Doctor { json }) => {
            let config = Config::load().context("invalid configuration")?;
            let report = doctor::report(&config).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                doctor::print(&report);
            }
            return Ok(());
        }
        Some(Command::Upgrade { check }) => {
            upgrade::run(check).await?;
            return Ok(());
        }
        Some(Command::Install(options)) => {
            install::run(options).await?;
            return Ok(());
        }
        Some(Command::Machines { json }) => {
            let catalogue = threadbase_core::state::catalogue();
            if json {
                println!("{}", serde_json::to_string_pretty(&catalogue)?);
            } else {
                print_machines(&catalogue);
            }
            return Ok(());
        }
        Some(Command::Reindex) => {
            let config = Config::load().context("invalid configuration")?;
            let pool = db::connect(&config.db_path).await?;
            threadbase_server::search::rebuild(&pool).await?;
            println!("search index rebuilt");
            return Ok(());
        }
        Some(Command::Compact { dry_run, vacuum }) => {
            let config = Config::load().context("invalid configuration")?;
            let report = maintenance::compact(&config, dry_run, vacuum).await?;
            println!(
                "{}events: {} rows -> {} ({} removed, {} folded)",
                if dry_run { "dry run: " } else { "" },
                report.rows_before,
                report.rows_before.saturating_sub(report.rows_removed),
                report.rows_removed,
                report.rows_folded,
            );
            println!(
                "threads: {}; database: {} -> {}{}",
                report.threads,
                format_bytes(report.bytes_before),
                format_bytes(report.bytes_after),
                if report.vacuumed { " (vacuumed)" } else { "" },
            );
            return Ok(());
        }
        Some(Command::Service { command, unit }) => {
            let command = match command {
                ServiceCommand::Start => service::Command::Start,
                ServiceCommand::Stop => service::Command::Stop,
                ServiceCommand::Restart => service::Command::Restart,
                ServiceCommand::Status => service::Command::Status,
                ServiceCommand::Logs { lines } => service::Command::Logs { lines },
            };
            service::run(command, &unit).await?;
            return Ok(());
        }
        Some(Command::Serve) | None => {}
    }

    let config = Config::load().context("invalid configuration")?;
    let _log_guard = logging::init(config.log_json, config.log_file.as_deref());
    tokio::fs::create_dir_all(&config.data_dir)
        .await
        .with_context(|| format!("create data directory {}", config.data_dir.display()))?;
    let pool = db::connect(&config.db_path).await?;

    let state = AppState::new(config.clone(), pool);
    // The previous process died with every agent child; make persisted state
    // say so and resume input the user had already queued.
    // Setup must be unblocked before agents resume queued input; otherwise the
    // queue worker sees a permanently "running" setup and leaves work stranded.
    if let Err(error) = threadbase_server::setup::recover_after_restart(
        &state.pool,
        &state.projection,
        &state.events,
    )
    .await
    {
        tracing::warn!(%error, "setup recovery failed");
    }
    if let Err(error) = state.agents.recover_after_restart().await {
        tracing::warn!(%error, "startup recovery failed");
    }
    if let Err(error) = state.agents.backfill_derived_subagents().await {
        tracing::warn!(%error, "subagent backfill failed");
    }
    if let Err(error) = state.drafts.backfill_flags().await {
        tracing::warn!(%error, "draft marker backfill failed");
    }
    // Titles written before the prompt cleaner existed still carry paste markup.
    match threadbase_server::threads::backfill_titles(&state.pool).await {
        Ok(0) => {}
        Ok(repaired) => tracing::info!(repaired, "thread titles repaired"),
        Err(error) => tracing::warn!(%error, "thread title backfill failed"),
    }
    let index_pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(error) = threadbase_server::search::ensure_indexed(&index_pool).await {
            tracing::warn!(%error, "search index backfill failed");
        }
    });
    state.push.spawn();
    // A staged binary installs itself only while nothing is running.
    let _deploy = threadbase_server::deploy::spawn(&state.config, state.pool.clone());
    // Parking idle work and waking due snoozes happen without a client; the
    // decision itself lives in `threadbase-core`, the loop only supplies facts.
    let _sweeper = threadbase_server::threads::spawn_sweeper(
        state.pool.clone(),
        state.projection.clone(),
        state.config.settle_days.saturating_mul(24 * 60 * 60 * 1000),
    );
    let agents = state.agents.clone();
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("bind {}", config.bind))?;
    let local = format!("http://{}", config.bind);
    // Keep the assertion for as long as the server serves.
    let _awake = power::hold(config.keep_awake)?;
    tracing::info!(
        url = %local,
        data_dir = %config.data_dir.display(),
        database = %config.db_path.display(),
        public_origin = ?config.public_origin,
        "threadbase started"
    );
    println!(
        "Threadbase {} listening on {local}",
        env!("CARGO_PKG_VERSION")
    );
    println!("Data directory: {}", config.data_dir.display());
    if let Some(path) = &config.log_file {
        println!("Log file: {}", path.display());
    }
    match config.public_origin.as_deref() {
        Some(origin) => println!("Sign-in: Tailscale identity via `tailscale serve` ({origin})"),
        None => {
            println!("Sign-in: reachable on the loopback port only.");
            println!("         Run `threadbase install` to expose it on the tailnet.");
        }
    }
    if config.dev_identity.is_some() {
        println!("Warning: THREADBASE_DEV_IDENTITY is set; direct local requests skip sign-in.");
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        // The unit keeps daemonized tmux servers alive across restarts; the
        // agent adapters are not daemons and must not be left orphaned.
        agents.shutdown_all().await;
    })
    .await
    .context("server error")
}

fn print_machines(catalogue: &[threadbase_shared::MachineSpec]) {
    for (index, machine) in catalogue.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{}  ({})", machine.id, machine.title);
        println!("  {}", machine.summary);
        for state in &machine.states {
            let terminal = if state.terminal { ", terminal" } else { "" };
            println!("  {:<16} {}{}", state.id, state.label, terminal);
            if !state.commands.is_empty() {
                let commands: Vec<String> = state
                    .commands
                    .iter()
                    .map(|command| format!("{} -> {}", command.id, command.to))
                    .collect();
                println!("  {:<16} -> {}", "", commands.join(", "));
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    format!("{mib:.1} MiB")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}
