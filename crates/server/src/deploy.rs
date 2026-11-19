//! Installs a staged binary, but only when no turn is running.
//!
//! `deploy.sh --stage` writes a binary to `incoming/threadbase`; this loop
//! swaps it in and exits, so the service manager restarts into it. "Idle" is the
//! same check `thread_wait` uses, and a pending approval counts as work because
//! a restart cancels it.

use crate::agents::AgentManager;
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
/// Exit code that means "a new binary is installed, start it".
///
/// Non-zero on purpose: units installed before `Restart=always` still use
/// `Restart=on-failure`, and launchd's `KeepAlive.SuccessfulExit=false` reads a
/// clean exit as final. 75 is `EX_TEMPFAIL` and says what happened in a journal.
pub const EXIT_RESTART: i32 = 75;

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
pub fn spawn(
    config: &Config,
    pool: SqlitePool,
    agents: AgentManager,
) -> Option<tokio::task::JoinHandle<()>> {
    if !config.auto_deploy {
        return None;
    }
    let staged = staged_path(config);
    let target = std::env::current_exe().ok()?;
    let backups = config.data_dir.join("backups");
    tracing::info!(
        staged = %staged.display(),
        target = %target.display(),
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
                        "installed the staged binary; exiting so the service manager starts it"
                    );
                    // Never restart from here: a restart stops this unit and
                    // waits for this process to exit, so it cannot return while
                    // it is still awaited, and the service manager kills the
                    // stop after 90 s and drops the queued start. Exiting is the
                    // restart both systemd and launchd pick up. The unit's
                    // `KillMode=process` leaves tmux alone, so the adapters are
                    // stopped here before the process goes.
                    agents.shutdown_all().await;
                    std::process::exit(EXIT_RESTART);
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
    // Copy to a sibling file and rename it into place. Writing straight onto
    // `target` fails with ETXTBSY while this very binary is running — the
    // auto-deploy loop's whole situation on Linux — while a rename atomically
    // swaps the name and leaves the running process on its own inode.
    let temp = target.with_extension("incoming");
    std::fs::copy(staged, &temp).map_err(|source| DeployError::Write {
        path: temp.clone(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755)).map_err(
            |source| DeployError::Write {
                path: temp.clone(),
                source,
            },
        )?;
    }
    std::fs::rename(&temp, target).map_err(|source| DeployError::Write {
        path: target.to_path_buf(),
        source,
    })?;
    // Rename the staged file so the same build cannot install twice.
    let consumed = staged.with_extension("installed");
    std::fs::rename(staged, &consumed).map_err(|source| DeployError::Write {
        path: staged.to_path_buf(),
        source,
    })?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_swaps_the_binary_and_consumes_the_staged_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let staged = dir.path().join("incoming");
        let target = dir.path().join("threadbase");
        std::fs::write(&staged, b"new").expect("write staged");
        std::fs::write(&target, b"old").expect("write target");

        let backup = install(&staged, &target, &dir.path().join("backups")).expect("install");

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read(&backup).unwrap(), b"old");
        assert!(!staged.exists(), "the staged file must not install twice");
    }
}
