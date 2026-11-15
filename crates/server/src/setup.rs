//! Project setup hooks for new thread worktrees.
//!
//! A setup command runs once per worktree before the agent is allowed to
//! start. It is asynchronous because dependency installs routinely outlive an
//! HTTP request; progress is written into the thread as notices and agent-log
//! lines, and the persisted `setup_status` gates `ensure_started`.

use crate::app::AppState;
use crate::error::ServerError;
use crate::events::EventMeta;
use crate::projection::Projection;
use sqlx::SqlitePool;
use std::path::Path;
use threadbase_core::state::{parse_setup_status, setup_status_str, SetupAction, SetupMachine};
use threadbase_shared::{
    now_ms, Actor, ApiError, SetupStatus, ThreadEvent, ThreadId, ThreadStatus,
};
use tokio::process::Command;

/// Start setup for a freshly created or forked thread, when the project has a
/// command and the thread has a worktree. Returns whether a setup task started.
pub async fn maybe_start(state: &AppState, thread_id: &str) -> Result<bool, ServerError> {
    let row = crate::threads::load_row(&state.pool, thread_id).await?;
    let Some(cwd) = row.worktree_path else {
        return Ok(false);
    };
    let project = crate::projects::row(&state.pool, &row.project_id).await?;
    let command = project
        .setup_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(command) = command else {
        return Ok(false);
    };
    start(state, thread_id, &cwd, command).await?;
    Ok(true)
}

/// Re-run setup from the thread menu / failure banner.
pub async fn rerun(state: &AppState, thread_id: &str) -> Result<(), ServerError> {
    let row = crate::threads::load_row(&state.pool, thread_id).await?;
    let Some(cwd) = row.worktree_path else {
        return Err(ServerError(ApiError::validation(
            "this thread has no worktree to set up",
        )));
    };
    if matches!(
        crate::threads::status(&state.pool, thread_id).await?,
        ThreadStatus::Preparing | ThreadStatus::Running | ThreadStatus::RequiresAction
    ) {
        return Err(ServerError(ApiError::conflict(
            "the agent is busy; stop it before re-running setup",
        )));
    }
    if busy(&state.pool, thread_id).await {
        return Err(ServerError(ApiError::conflict(
            "worktree setup is already running",
        )));
    }
    let project = crate::projects::row(&state.pool, &row.project_id).await?;
    let command = project
        .setup_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ServerError(ApiError::validation(
                "set a Setup command in project settings first",
            ))
        })?;
    start(state, thread_id, &cwd, command).await
}

/// True while a thread must not start its agent.
pub async fn busy(pool: &SqlitePool, thread_id: &str) -> bool {
    let status: Option<String> =
        sqlx::query_scalar("SELECT setup_status FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    status
        .as_deref()
        .and_then(parse_setup_status)
        .is_some_and(SetupMachine::is_busy)
}

/// Apply one setup transition, persist the result with its error text, and
/// publish the projection. The machine decides legality; callers only name
/// the event.
async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    action: &SetupAction,
    error: Option<&str>,
) -> Result<SetupStatus, ServerError> {
    let current = read_status(pool, thread_id).await?;
    let next = SetupMachine::transition(current, action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE threads SET setup_status = ?, setup_error = ?, updated_at = ? WHERE id = ?",
    )
    .bind(setup_status_str(next))
    .bind(error)
    .bind(now_ms())
    .bind(thread_id)
    .execute(pool)
    .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

async fn read_status(pool: &SqlitePool, thread_id: &str) -> Result<SetupStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT setup_status FROM threads WHERE id = ?")
        .bind(thread_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("thread not found"))?;
    Ok(parse_setup_status(&value).unwrap_or(SetupStatus::None))
}

async fn start(
    state: &AppState,
    thread_id: &str,
    cwd: &str,
    command: &str,
) -> Result<(), ServerError> {
    transition(
        &state.pool,
        &state.projection,
        thread_id,
        &SetupAction::Start,
        None,
    )
    .await?;
    let app = state.clone();
    let thread_id = thread_id.to_owned();
    let cwd = cwd.to_owned();
    let command = command.to_owned();
    tokio::spawn(async move {
        if let Err(error) = run(&app, &thread_id, &cwd, &command).await {
            tracing::warn!(thread_id, %error, "worktree setup task failed");
        }
    });
    Ok(())
}

async fn run(
    state: &AppState,
    thread_id: &str,
    cwd: &str,
    command: &str,
) -> Result<(), ServerError> {
    let _ = transition(
        &state.pool,
        &state.projection,
        thread_id,
        &SetupAction::Started,
        None,
    )
    .await;
    append(
        state,
        thread_id,
        ThreadEvent::SystemNotice {
            level: "info".to_owned(),
            message: format!("Running worktree setup: {command}"),
        },
    )
    .await;

    let mut cmd = Command::new("sh");
    cmd.arg("-lc")
        .arg(command)
        .current_dir(Path::new(cwd))
        .kill_on_drop(true);
    let timeout = std::time::Duration::from_secs(
        u64::try_from(state.config.setup_timeout_seconds).unwrap_or(900),
    );
    let output = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return finish_failed(
                state,
                thread_id,
                &format!("could not start the setup command: {error}"),
                "",
            )
            .await
        }
        Err(_) => {
            return finish_failed(
                state,
                thread_id,
                &format!(
                    "setup timed out after {} seconds",
                    state.config.setup_timeout_seconds
                ),
                "",
            )
            .await
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    if !combined.trim().is_empty() {
        append(
            state,
            thread_id,
            ThreadEvent::AgentLog {
                level: if output.status.success() {
                    "info".to_owned()
                } else {
                    "error".to_owned()
                },
                text: tail(&combined, 32_000),
            },
        )
        .await;
    }
    if output.status.success() {
        transition(
            &state.pool,
            &state.projection,
            thread_id,
            &SetupAction::Finish,
            None,
        )
        .await?;
        append(
            state,
            thread_id,
            ThreadEvent::SystemNotice {
                level: "info".to_owned(),
                message: "Worktree setup finished.".to_owned(),
            },
        )
        .await;
        state.projection.publish(thread_id).await;
        if has_queued_input(&state.pool, thread_id).await {
            state.agents.kick_queue(thread_id);
        }
        Ok(())
    } else {
        finish_failed(
            state,
            thread_id,
            &format!("setup exited with {}", output.status),
            &tail(&combined, 4000),
        )
        .await
    }
}

async fn finish_failed(
    state: &AppState,
    thread_id: &str,
    message: &str,
    output: &str,
) -> Result<(), ServerError> {
    let error = if output.trim().is_empty() {
        message.to_owned()
    } else {
        format!("{message}\n{}", output.trim())
    };
    let error = tail(&error, 4000);
    let _ = transition(
        &state.pool,
        &state.projection,
        thread_id,
        &SetupAction::Fail,
        Some(&error),
    )
    .await;
    append(
        state,
        thread_id,
        ThreadEvent::SystemNotice {
            level: "error".to_owned(),
            message: format!("Worktree setup failed: {message}"),
        },
    )
    .await;
    let _ = crate::notify::notify_watchers(
        &state.pool,
        &state.projection,
        thread_id,
        "error",
        None,
        None,
        &format!("Worktree setup failed: {message}"),
        None,
    )
    .await;
    state.projection.publish(thread_id).await;
    Ok(())
}

async fn append(state: &AppState, thread_id: &str, payload: ThreadEvent) {
    let _ = state
        .events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::system(),
            payload,
            EventMeta::default(),
        )
        .await;
}

async fn has_queued_input(pool: &SqlitePool, thread_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM queued_inputs WHERE thread_id = ? AND status = 'queued'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await
    .unwrap_or_default()
        > 0
}

/// Mark setup interrupted by a server restart as failed so the thread does not
/// stay permanently gated. The user can retry from the banner.
pub async fn recover_after_restart(
    pool: &SqlitePool,
    projection: &Projection,
    events: &crate::events::EventStore,
) -> Result<(), ServerError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, setup_status FROM threads WHERE setup_status IN ('pending', 'running')",
    )
    .fetch_all(pool)
    .await?;
    for (thread_id, previous) in rows {
        let message = format!(
            "Worktree setup was interrupted by a server restart ({previous}). Run setup again."
        );
        let _ = transition(
            pool,
            projection,
            &thread_id,
            &SetupAction::Interrupt,
            Some(&message),
        )
        .await;
        let _ = events
            .append(
                &ThreadId::from(thread_id.clone()),
                Actor::system(),
                ThreadEvent::SystemNotice {
                    level: "error".to_owned(),
                    message,
                },
                EventMeta::default(),
            )
            .await;
        projection.publish(&thread_id).await;
    }
    Ok(())
}

fn tail(value: &str, limit: usize) -> String {
    let count = value.chars().count();
    value
        .chars()
        .skip(count.saturating_sub(limit))
        .collect::<String>()
        .trim()
        .to_owned()
}
