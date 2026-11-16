use anyhow::Context;
use clap::{Parser, Subcommand};
use std::process::ExitCode;
use threadbase_server::app::AppState;
use threadbase_server::config::Config;
use threadbase_server::{db, doctor, install, maintenance, routes, service, upgrade};
use tracing_subscriber::EnvFilter;

const ENV_HELP: &str = "\
Environment:
  THREADBASE_DATA_DIR          Data directory (default: platform data dir)
  THREADBASE_BIND              Loopback address (default: 127.0.0.1:7777)
  THREADBASE_PUBLIC_ORIGIN     Public HTTPS origin; required for GitHub sign-in
  THREADBASE_GITHUB_CLIENT_ID  GitHub OAuth app client id
  THREADBASE_GITHUB_CLIENT_SECRET
                               GitHub OAuth app client secret
  THREADBASE_GITHUB_ALLOW      Comma-separated GitHub logins allowed to sign in
  THREADBASE_ADMINS            Comma-separated GitHub logins that are always admins
  THREADBASE_TERMINAL_DRIVERS  GitHub logins allowed to drive the browser terminal
  THREADBASE_IDLE_SESSION_SECS Idle agent sessions shut down after N seconds (default 900, 0=off)
  THREADBASE_MAX_ACTIVE_TURNS  Concurrent agent turns across all threads (default 4, 0=unlimited)
  THREADBASE_SETUP_TIMEOUT_SECS Worktree setup command timeout (default 900)
  THREADBASE_SESSION_DAYS      Session lifetime in days (default 30)
  THREADBASE_COOKIE_SECURE     Force the Secure cookie flag on or off
  THREADBASE_DEV_IDENTITY      Identity accepted from direct local connections
                               (development only; ignored behind a tunnel)
  THREADBASE_LOG_JSON          Set to 1 for JSON logs
  THREADBASE_ACP_COMMAND       Override the ACP adapter command (development)
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
    /// One-command systemd deployment plus an HTTPS tunnel
    ///
    /// Configure a systemd user service for this binary and expose it over HTTPS.
    /// Nothing is built, copied or downloaded: the unit runs the current executable.
    Install(install::Options),
    /// Write a launchd/systemd user unit
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
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
    /// Write a launchd/systemd user unit
    Install {
        /// Print the unit and change nothing
        #[arg(long)]
        print: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
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
    match run(cli).await {
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
        Some(Command::Service { command }) => {
            let ServiceCommand::Install { print } = command;
            service::install(print)?;
            return Ok(());
        }
        Some(Command::Serve) | None => {}
    }

    let config = Config::load().context("invalid configuration")?;
    init_tracing(config.log_json);
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
    let index_pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(error) = threadbase_server::search::ensure_indexed(&index_pool).await {
            tracing::warn!(%error, "search index backfill failed");
        }
    });
    state.push.spawn();
    let agents = state.agents.clone();
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("bind {}", config.bind))?;
    let local = format!("http://{}", config.bind);
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
    if config.github_ready() {
        println!(
            "Sign-in: GitHub OAuth, callback {}",
            config.callback_url().unwrap_or_default()
        );
    } else if config.github.is_some() {
        println!("Sign-in: GitHub client configured but THREADBASE_PUBLIC_ORIGIN is missing.");
        println!("         Set it to the public HTTPS origin so the OAuth callback resolves.");
    } else {
        println!("Sign-in: GitHub OAuth is not configured; only local development works.");
        println!("         Set THREADBASE_GITHUB_CLIENT_ID and THREADBASE_GITHUB_CLIENT_SECRET.");
    }
    if config.dev_identity.is_some() {
        println!(
            "Warning: THREADBASE_DEV_IDENTITY is set; direct local requests skip GitHub sign-in."
        );
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

fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("threadbase=info,tower_http=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
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
