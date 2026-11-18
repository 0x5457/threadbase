//! Installing a new binary while nothing is running.
//!
//! The server is the thing being replaced, so it does the swap itself.
//! `deploy.sh --stage` puts a built binary in `incoming/threadbase`; this loop
//! installs it and restarts, but only when no turn is running. If something is
//! running, it waits.
//!
//! "Idle" means the same thing as in `thread_wait`: no turn, no queued message,
//! no running subagent, and no approval or question waiting for a person.
//! A pending approval blocks the deploy because a restart would cancel it.

use crate::config::Config;
use crate::error::ServerError;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::time::Duration;
use threadbase_shared::now_ms;

/// How often to look for a staged binary.
const PERIOD: Duration = Duration::from_secs(15);
/// Waiting is normal, so log it at most every ten minutes.
const REMIND_EVERY: i64 = 10 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Starts the watcher when `THREADBASE_AUTO_DEPLOY` is on.
#[must_use]
pub fn spawn(config: &Config, pool: SqlitePool) -> Option<tokio::task::JoinHandle<()>> {
    if !config.auto_deploy {
        return None;
    }
    let staged = staged_path(config);
    let target = std::env::current_exe().ok()?;
    let backups = config.data_dir.join("backups");
    let unit = config.unit.clone();
    tracing::info!(
        staged = %staged.display(),
        target = %target.display(),
        unit,
        "auto-deploy is on: a staged binary installs when no turn is running"
    );
    Some(tokio::spawn(async move {
        let mut announced = 0i64;
        loop {
            tokio::time::sleep(PERIOD).await;
            if !is_staged(&staged).await {
                continue;
            }
            match busy(&pool).await {
                Ok(true) => {
                    let now = now_ms();
                    if now - announced > REMIND_EVERY {
                        announced = now;
                        tracing::info!(
                            staged = %staged.display(),
                            "a new binary is waiting; installing it when no turn is running"
                        );
                    }
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(%error, "could not tell whether the machine is idle; not deploying");
                    continue;
                }
            }
            match install(&staged, &target, &backups) {
                Ok(backup) => {
                    tracing::warn!(
                        backup = %backup.display(),
                        "installed the staged binary; restarting into it"
                    );
                    // The restart ends this process, so code after this may not
                    // run. `service::run` is not `Send` (it holds a
                    // service-manager handle across an await), so it gets its
                    // own thread and runtime.
                    let restart_unit = unit.clone();
                    let restarted = tokio::task::spawn_blocking(move || {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()?;
                        runtime.block_on(crate::service::run(
                            crate::service::Command::Restart,
                            &restart_unit,
                        ))
                    })
                    .await;
                    match restarted {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            tracing::error!(%error, "installed but could not restart the service")
                        }
                        Err(error) => {
                            tracing::error!(%error, "restart task failed after installing")
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "could not install the staged binary");
                }
            }
            // Do not retry in a loop: the error is in the log, and dropping a
            // fresh file is the operator's decision.
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }))
}

#[must_use]
pub fn staged_path(config: &Config) -> PathBuf {
    config
        .auto_deploy_path
        .clone()
        .unwrap_or_else(|| config.data_dir.join("incoming").join("threadbase"))
}

/// Whether `path` is an executable file.
pub async fn is_staged(path: &Path) -> bool {
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Whether a restart right now would interrupt someone.
pub async fn busy(pool: &SqlitePool) -> Result<bool, ServerError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT
            (SELECT COUNT(*) FROM threads
              WHERE status IN ('preparing','running','requires_action'))
          + (SELECT COUNT(*) FROM approvals WHERE status = 'pending')
          + (SELECT COUNT(*) FROM elicitations WHERE status = 'pending')
          + (SELECT COUNT(*) FROM queued_inputs WHERE status = 'queued')
          + (SELECT COUNT(*) FROM subagents WHERE status IN ('running','requires_action'))",
    )
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Replaces `target` with `staged` and keeps the old binary as a backup.
///
/// The backup is written first and the staged file is renamed afterwards, so a
/// failure cannot leave a half-written binary or a staged file that installs
/// itself again on the next loop.
pub fn install(staged: &Path, target: &Path, backups: &Path) -> Result<PathBuf, DeployError> {
    std::fs::create_dir_all(backups).map_err(|source| DeployError::Write {
        path: backups.to_path_buf(),
        source,
    })?;
    let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let backup = backups.join(format!("threadbase-{stamp}"));
    if target.exists() {
        std::fs::copy(target, &backup).map_err(|source| DeployError::Write {
            path: backup.clone(),
            source,
        })?;
    }
    // Copy rather than rename: the staged path can be on another filesystem.
    std::fs::copy(staged, target).map_err(|source| DeployError::Write {
        path: target.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755)).map_err(
            |source| DeployError::Write {
                path: target.to_path_buf(),
                source,
            },
        )?;
    }
    // Rename the staged file so the same build cannot install twice.
    let consumed = staged.with_extension("installed");
    std::fs::rename(staged, &consumed).map_err(|source| DeployError::Write {
        path: staged.to_path_buf(),
        source,
    })?;
    Ok(backup)
}
