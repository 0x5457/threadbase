//! `threadbase service`: restart, status and logs for the installed service.
//!
//! One implementation for both platforms: `deploy.sh`, `threadbase upgrade`
//! and humans all use these commands instead of `systemctl` or `launchctl`.

use crate::config::Config;
use crate::install::{
    health, kind_name, logs, normalize_label, restart, service_state, start, stop,
};
use service_manager::ServiceStatus;

pub enum Command {
    Start,
    Stop,
    Restart,
    Status,
    Logs { lines: usize },
}

pub async fn run(command: Command, unit: &str) -> anyhow::Result<()> {
    let label = normalize_label(unit)?;
    match command {
        Command::Start => {
            start(&label)?;
            println!("Started {label} ({})", kind_name());
            Ok(())
        }
        Command::Stop => {
            stop(&label)?;
            println!("Stopped {label} ({})", kind_name());
            Ok(())
        }
        Command::Restart => {
            restart(&label).await?;
            println!("Restarted {label} ({})", kind_name());
            Ok(())
        }
        Command::Status => {
            println!("service   {label} ({})", kind_name());
            let state = match service_state(&label).await {
                Ok(state) => {
                    println!("state     {}", describe(&state));
                    Some(state)
                }
                Err(error) => {
                    println!("state     unknown ({error})");
                    None
                }
            };
            // The health endpoint is the real answer; a service manager that
            // cannot report its status must not hide a working server.
            let mut running = matches!(state, Some(ServiceStatus::Running));
            if let Ok(config) = Config::load() {
                let url = format!("http://{}/api/health", config.bind);
                match health::probe(config.bind.port()).await {
                    Ok(()) => {
                        println!("health    ok {url}");
                        running = true;
                    }
                    Err(error) => println!("health    {error}"),
                }
            }
            if running {
                Ok(())
            } else {
                anyhow::bail!("{label} is not running; run `threadbase install` first")
            }
        }
        Command::Logs { lines } => {
            let log_file = Config::load().ok().and_then(|config| config.log_file);
            logs(&label, log_file.as_deref(), lines)
        }
    }
}

fn describe(state: &ServiceStatus) -> &'static str {
    match state {
        ServiceStatus::Running => "running",
        ServiceStatus::Stopped(_) => "stopped",
        ServiceStatus::NotInstalled => "not installed",
    }
}
