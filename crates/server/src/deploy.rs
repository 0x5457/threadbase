//! Staged deploys during an idle window.
//!
//! A self-hosted server is also the thing being replaced, and the one thing a
//! deploy must not do is cut a running turn in half. So the server does the
//! swap, not the script: `deploy.sh --stage` drops a built binary in
//! `incoming/threadbase`, and this loop installs it the moment nothing is
//! happening — or waits, indefinitely, until that is true.
//!
//! "Nothing happening" is the same predicate `thread_wait` uses: no turn in
//! flight, no queued input, no live subagents, and no unanswered approval or
//! question. A pending approval is not idle: a restart would cancel it.

use crate::config::Config;
use crate::error::ServerError;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::time::Duration;
use threadbase_shared::now_ms;

/// How often a staged binary is considered. Small enough that "it deploys when
/// idle" feels immediate, large enough to be invisible.
const PERIOD: Duration = Duration::from_secs(15);
/// Waiting for an idle window is normal, so say it once and then rarely.
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

/// Start the watcher when enabled. Returns `None` when the feature is off, which
/// is the default.
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
                            "waiting for an idle window to install the staged binary"
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
                    // The restart stops this process, so nothing after this line
                    // is guaranteed to run. `service::run` holds a service-manager
                    // handle across an await and is therefore not `Send`; a
                    // dedicated thread gives it a runtime of its own.
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
            // Whether it worked or not, do not retry in a hot loop: the operator
            // sees the log, and `deploy.sh --stage` can drop a fresh file.
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

/// True when `path` is a file the server can plausibly run.
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

/// Move `staged` over `target`, keeping the previous binary. Returns the backup
/// path.
///
/// The order matters: the backup is taken first and the staged file is consumed
/// by the move, so a failure cannot leave two half-written binaries or a staged
/// file that installs itself again on the next tick.
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
    // Consume the staged file so the same build cannot be installed twice.
    let consumed = staged.with_extension("installed");
    std::fs::rename(staged, &consumed).map_err(|source| DeployError::Write {
        path: staged.to_path_buf(),
        source,
    })?;
    Ok(backup)
}
