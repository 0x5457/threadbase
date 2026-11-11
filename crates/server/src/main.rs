use anyhow::Context;
use std::process::ExitCode;
use threadbase_server::config::Config;
use threadbase_server::state::AppState;
use threadbase_server::{db, doctor, install, routes, service, upgrade};
use tracing_subscriber::EnvFilter;

const HELP: &str = "\
Threadbase - shared coding agent threads for small teams

Usage:
  threadbase [serve]           Start the local server (default)
  threadbase doctor [--json]   Check the environment and agent adapters
  threadbase upgrade [--check] Download and verify the latest release
  threadbase install [options]
                               One-command systemd deployment plus tailscale serve
  threadbase service install [--print]
                               Write a launchd/systemd user unit
  threadbase --version         Print the version
  threadbase --help            Print this help

Environment:
  THREADBASE_DATA_DIR          Data directory (default: platform data dir)
  THREADBASE_BIND              Loopback address (default: 127.0.0.1:7777)
  THREADBASE_PUBLIC_ORIGIN     Public HTTPS origin used in share links
  THREADBASE_TAILSCALE_ADMIN   Comma-separated Tailscale logins that are admins
  THREADBASE_DEV_IDENTITY      Identity accepted from direct local connections
                               (development only; ignored behind tailscale serve)
  THREADBASE_LOG_JSON          Set to 1 for JSON logs
  THREADBASE_ACP_COMMAND       Override the ACP adapter command (development)
  THREADBASE_RELEASE_BASE      Override the release download base URL
";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("threadbase: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    match command.as_deref() {
        Some("--version" | "-V" | "version") => {
            println!("threadbase {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h" | "help") => {
            print!("{HELP}");
            return Ok(());
        }
        Some("doctor") => {
            let config = Config::load().context("invalid configuration")?;
            let report = doctor::report(&config).await;
            if args.any(|arg| arg == "--json") {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                doctor::print(&report);
            }
            return Ok(());
        }
        Some("upgrade") => {
            upgrade::run(args.any(|arg| arg == "--check")).await?;
            return Ok(());
        }
        Some("install") => {
            let options = install::parse_args(args)?;
            install::run(options).await?;
            return Ok(());
        }
        Some("service") => {
            if args.next().as_deref() != Some("install") {
                anyhow::bail!("usage: threadbase service install [--print]");
            }
            service::install(args.any(|arg| arg == "--print"))?;
            return Ok(());
        }
        Some("serve") | None => {}
        Some(other) => {
            anyhow::bail!("unknown command `{other}`; run `threadbase --help`");
        }
    }

    let config = Config::load().context("invalid configuration")?;
    init_tracing(config.log_json);
    tokio::fs::create_dir_all(&config.data_dir)
        .await
        .with_context(|| format!("create data directory {}", config.data_dir.display()))?;
    let pool = db::connect(&config.db_path).await?;

    let state = AppState::new(config.clone(), pool);
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
    println!("Access: run `tailscale serve --bg 443 {local}` and open the tailnet URL.");
    if config.dev_identity.is_some() {
        println!(
            "Warning: THREADBASE_DEV_IDENTITY is set; direct local requests bypass Tailscale identity."
        );
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")
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
