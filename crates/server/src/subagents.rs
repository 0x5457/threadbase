//! Subagent records reported by the agent harness.

use crate::error::ServerError;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;
use std::path::PathBuf;
use threadbase_core::state::{
    parse_subagent_status, subagent_status_str, SubagentAction, SubagentMachine,
};
use threadbase_shared::{now_ms, ApiError, SubagentDto, SubagentLogEntryDto, SubagentStatus};

/// Only recent delegations can still have a live worker on disk. Keep the
/// status probe off the historical rows that make up most of a long-lived
/// sidebar.
const LIVE_STATUS_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(FromRow)]
struct SubagentRow {
    id: String,
    thread_id: String,
    parent_id: Option<String>,
    title: String,
    description: Option<String>,
    status: String,
    cwd: Option<String>,
    can_cancel: i64,
    usage_json: Option<String>,
    last_seq: i64,
    created_at: i64,
    updated_at: i64,
}

pub async fn list(pool: &SqlitePool, thread_id: &str) -> Result<Vec<SubagentDto>, ServerError> {
    let mut rows = sqlx::query_as::<_, SubagentRow>(
        "SELECT id, thread_id, parent_id, title, description, status, cwd, can_cancel,
                usage_json, last_seq, created_at, updated_at
         FROM subagents WHERE thread_id = ? ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    for row in &mut rows {
        apply_live_status(row);
    }
    Ok(rows.into_iter().map(to_dto).collect())
}

/// Subagents for several threads in one round trip, keyed by thread id.
pub async fn list_for_threads(
    pool: &SqlitePool,
    thread_ids: &[&str],
) -> Result<HashMap<String, Vec<SubagentDto>>, ServerError> {
    if thread_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = sqlx::QueryBuilder::new(
        "SELECT id, thread_id, parent_id, title, description, status, cwd, can_cancel,
                usage_json, last_seq, created_at, updated_at
         FROM subagents WHERE thread_id IN (",
    );
    let mut separated = query.separated(", ");
    for id in thread_ids {
        separated.push_bind(*id);
    }
    separated.push_unseparated(") ORDER BY created_at ASC");
    let rows = query
        .build_query_as::<SubagentRow>()
        .fetch_all(pool)
        .await?;
    let mut out: HashMap<String, Vec<SubagentDto>> = HashMap::new();
    for mut row in rows {
        apply_live_status(&mut row);
        out.entry(row.thread_id.clone())
            .or_default()
            .push(to_dto(row));
    }
    Ok(out)
}

pub async fn status_of(
    pool: &SqlitePool,
    subagent_id: &str,
) -> Result<SubagentStatus, ServerError> {
    let value: Option<String> = sqlx::query_scalar("SELECT status FROM subagents WHERE id = ?")
        .bind(subagent_id)
        .fetch_optional(pool)
        .await?;
    let value = value.ok_or_else(|| ServerError::internal("subagent not found"))?;
    parse_subagent_status(&value)
        .ok_or_else(|| ServerError::internal(format!("unknown subagent status: {value}")))
}

pub struct Upsert<'a> {
    pub thread_id: &'a str,
    pub id: &'a str,
    pub parent_id: Option<&'a str>,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub can_cancel: bool,
    pub usage_json: Option<&'a str>,
    pub action: SubagentAction,
}

/// Insert or update the identity fields, then apply the requested transition.
pub async fn upsert(
    pool: &SqlitePool,
    projection: &Projection,
    record: Upsert<'_>,
) -> Result<SubagentStatus, ServerError> {
    let created = SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Create)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let now = now_ms();
    sqlx::query(
        "INSERT INTO subagents(id, thread_id, parent_id, title, description, status, cwd,
            can_cancel, usage_json, last_seq, created_at, updated_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title,
            description = excluded.description, cwd = excluded.cwd,
            can_cancel = excluded.can_cancel, usage_json = excluded.usage_json,
            updated_at = excluded.updated_at",
    )
    .bind(record.id)
    .bind(record.thread_id)
    .bind(record.parent_id)
    .bind(record.title)
    .bind(record.description)
    .bind(subagent_status_str(created))
    .bind(record.cwd)
    .bind(i64::from(record.can_cancel))
    .bind(record.usage_json)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    transition(pool, projection, record.thread_id, record.id, record.action).await
}

pub async fn transition(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    subagent_id: &str,
    action: SubagentAction,
) -> Result<SubagentStatus, ServerError> {
    let current = {
        let value: Option<String> = sqlx::query_scalar("SELECT status FROM subagents WHERE id = ?")
            .bind(subagent_id)
            .fetch_optional(pool)
            .await?;
        match value {
            Some(value) => parse_subagent_status(&value).ok_or_else(|| {
                ServerError::internal(format!("unknown subagent status: {value}"))
            })?,
            None => SubagentStatus::Idle,
        }
    };
    let next = SubagentMachine::transition(current, &action)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    if next == current {
        return Ok(next);
    }
    sqlx::query("UPDATE subagents SET status = ?, updated_at = ? WHERE id = ?")
        .bind(subagent_status_str(next))
        .bind(now_ms())
        .bind(subagent_id)
        .execute(pool)
        .await?;
    projection.publish(thread_id).await;
    Ok(next)
}

fn to_dto(row: SubagentRow) -> SubagentDto {
    let status = parse_subagent_status(&row.status).unwrap_or(SubagentStatus::Idle);
    SubagentDto {
        id: row.id.into(),
        thread_id: row.thread_id.into(),
        parent_id: row.parent_id.map(Into::into),
        title: row.title,
        description: row.description,
        status,
        cwd: row.cwd,
        can_cancel: row.can_cancel != 0
            && !matches!(status, SubagentStatus::Cancelled | SubagentStatus::Error),
        usage: row
            .usage_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        last_seq: u64::try_from(row.last_seq).unwrap_or(0),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// One subagent's own conversation, when the adapter leaves it on disk. pi
/// writes each detached run to `~/.pi/agent/sessions/<cwd>/<parent>/<run>/run-0/
/// session.jsonl`; workflow children are mapped from the run receipt.
pub async fn transcript(
    pool: &SqlitePool,
    thread_id: &str,
    subagent_id: &str,
) -> Result<Vec<SubagentLogEntryDto>, ServerError> {
    let row: Option<(String, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT id, parent_id, title, cwd FROM subagents WHERE id = ? AND thread_id = ?",
    )
    .bind(subagent_id)
    .bind(thread_id)
    .fetch_optional(pool)
    .await?;
    let Some((id, parent_id, title, cwd)) = row else {
        return Err(ServerError(ApiError::not_found("subagent not found")));
    };
    let parent_id = parent_id.as_deref();
    let workflow_run = cwd
        .as_deref()
        .and_then(|value| value.strip_prefix("pi-subagent:"))
        .filter(|run| valid_run_id(run));
    let mut run_id = None;
    if let Some(workflow) = workflow_run {
        // A single delegation stores its session under the run id itself. A
        // detached workflow's row points at the workflow run, while the
        // actual conversation lives in one of its children; the child run id
        // is only published in the async run's status file.
        if session_file(workflow).is_some() {
            run_id = Some(workflow.to_owned());
        } else {
            run_id = workflow_child_run(workflow, &title);
        }
    }
    if run_id.is_none() {
        run_id = event_run_id(pool, thread_id, &id, parent_id, &title).await;
    }
    if run_id.is_none() {
        run_id = receipt_run_id(parent_id.unwrap_or(&id), &title);
    }
    let Some(run_id) = run_id else {
        return Ok(failure_entries(pool, thread_id, subagent_id).await);
    };
    let Some(file) = session_file(&run_id) else {
        return Ok(failure_entries(pool, thread_id, subagent_id).await);
    };
    let Ok(text) = tokio::fs::read_to_string(&file).await else {
        return Ok(failure_entries(pool, thread_id, subagent_id).await);
    };
    let entries = parse_session(&text);
    if entries.is_empty() {
        return Ok(failure_entries(pool, thread_id, subagent_id).await);
    }
    Ok(entries)
}

/// The entry list to show when no session can be replayed: the delegation's
/// launch error when it has one, otherwise nothing.
async fn failure_entries(
    pool: &SqlitePool,
    thread_id: &str,
    subagent_id: &str,
) -> Vec<SubagentLogEntryDto> {
    launch_failure(pool, thread_id, subagent_id)
        .await
        .map(|text| {
            vec![SubagentLogEntryDto {
                role: "error".to_owned(),
                text,
                ts: 0,
            }]
        })
        .unwrap_or_default()
}

/// A delegation that never launched has no session to replay. Its tool result
/// still carries the adapter's reason (an unknown agent, a bad script), which
/// is the only useful thing to show for that row.
async fn launch_failure(pool: &SqlitePool, thread_id: &str, subagent_id: &str) -> Option<String> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT payload_json FROM events
         WHERE thread_id = ? AND kind = 'tool_call_update'
           AND instr(payload_json, ?) > 0
         ORDER BY seq DESC",
    )
    .bind(thread_id)
    .bind(subagent_id)
    .fetch_all(pool)
    .await
    .ok()?;
    for payload in rows {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&payload) else {
            continue;
        };
        let Some(call) = event.get("call") else {
            continue;
        };
        if call.get("id").and_then(serde_json::Value::as_str) != Some(subagent_id) {
            continue;
        }
        if call.get("status").and_then(serde_json::Value::as_str) != Some("failed") {
            continue;
        }
        let Some(output) = call.get("output").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let text = serde_json::from_str::<serde_json::Value>(output)
            .ok()
            .and_then(|value| {
                value
                    .get("content")
                    .and_then(serde_json::Value::as_array)
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter_map(|block| {
                                block.get("text").and_then(serde_json::Value::as_str)
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
            })
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| output.to_owned());
        return Some(truncate(&text, 4000));
    }
    None
}

/// The run id a delegation reported back, read from the thread's own event
/// log. pi stores `details.runId` on the launch tool result (single agents)
/// and per child in `details.workflowChildren` (workflow runs), which is more
/// durable than the receipt directory in /tmp that pi may have cleaned up.
async fn event_run_id(
    pool: &SqlitePool,
    thread_id: &str,
    subagent_id: &str,
    parent_id: Option<&str>,
    title: &str,
) -> Option<String> {
    let (call_id, child) = match parent_id.and_then(|parent| {
        subagent_id
            .strip_prefix(parent)
            .and_then(|rest| rest.strip_prefix('/'))
            .map(|child| (parent, child))
    }) {
        // Workflow children are stored as `<parent>/<index>`; the run receipt
        // names them by their workflow child id, which is the row's title.
        Some((parent, child)) => (
            parent,
            Some(if child.chars().all(|c| c.is_ascii_digit()) {
                title
            } else {
                child
            }),
        ),
        None => (subagent_id, None),
    };
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT payload_json FROM events
         WHERE thread_id = ? AND kind = 'tool_call_update'
           AND instr(payload_json, ?) > 0
         ORDER BY seq DESC",
    )
    .bind(thread_id)
    .bind(call_id)
    .fetch_all(pool)
    .await
    .ok()?;
    for payload in rows {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&payload) else {
            continue;
        };
        let Some(call) = event.get("call") else {
            continue;
        };
        let Some(output) = call.get("output").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Ok(output) = serde_json::from_str::<serde_json::Value>(output) else {
            continue;
        };
        let Some(details) = output.get("details") else {
            continue;
        };
        match child {
            None => {
                if call.get("id").and_then(serde_json::Value::as_str) != Some(call_id) {
                    continue;
                }
                if let Some(run) = details.get("runId").and_then(serde_json::Value::as_str) {
                    if valid_run_id(run) {
                        return Some(run.to_owned());
                    }
                }
            }
            Some(child) => {
                let Some(children) = details.get("workflowChildren") else {
                    continue;
                };
                if children
                    .get("parentToolCallId")
                    .and_then(serde_json::Value::as_str)
                    != Some(call_id)
                {
                    continue;
                }
                let Some(items) = children
                    .get("children")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for item in items {
                    if item.get("childId").and_then(serde_json::Value::as_str) != Some(child) {
                        continue;
                    }
                    if let Some(run) = item.get("runId").and_then(serde_json::Value::as_str) {
                        if valid_run_id(run) {
                            return Some(run.to_owned());
                        }
                    }
                }
            }
        }
    }
    None
}

fn valid_run_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn receipt_run_id(parent_id: &str, title: &str) -> Option<String> {
    let temp = std::env::temp_dir();
    for entry in std::fs::read_dir(temp).ok()?.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("pi-subagents-uid-")
        {
            continue;
        }
        let Ok(runs) = std::fs::read_dir(entry.path().join("async-subagent-runs")) else {
            continue;
        };
        for run in runs.flatten() {
            let Ok(text) = std::fs::read_to_string(run.path().join("workflow-receipt.json")) else {
                continue;
            };
            let Ok(receipt) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let Some(children) = receipt.pointer("/workflowChildren") else {
                continue;
            };
            if children
                .get("parentToolCallId")
                .and_then(serde_json::Value::as_str)
                != Some(parent_id)
            {
                continue;
            }
            let Some(items) = children
                .get("children")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for child in items {
                if child.get("childId").and_then(serde_json::Value::as_str) != Some(title) {
                    continue;
                }
                if let Some(run_id) = child.get("runId").and_then(serde_json::Value::as_str) {
                    if valid_run_id(run_id) {
                        return Some(run_id.to_owned());
                    }
                }
            }
        }
    }
    None
}

/// pi reports a detached workflow child as idle once its launch returns, even
/// while the child keeps running in a separate process. The async run's
/// `status.json` stays current, so prefer it over the stale event state
/// whenever the row points at a pi subagent run.
fn apply_live_status(row: &mut SubagentRow) {
    if now_ms().saturating_sub(row.created_at) > LIVE_STATUS_WINDOW_MS {
        return;
    }
    let Some(run_id) = row
        .cwd
        .as_deref()
        .and_then(|value| value.strip_prefix("pi-subagent:"))
        .filter(|run| valid_run_id(run))
    else {
        return;
    };
    let Some(status) = workflow_step_status(run_id, &row.title) else {
        return;
    };
    let current = parse_subagent_status(&row.status).unwrap_or(SubagentStatus::Idle);
    // A child waiting on its own question is not made active by the workflow
    // still running, and a terminal row must not come back to life.
    if SubagentMachine::is_terminal(current) || current == SubagentStatus::RequiresAction {
        return;
    }
    row.status = subagent_status_str(status).to_owned();
}

fn workflow_step_status(run_id: &str, title: &str) -> Option<SubagentStatus> {
    let value = read_workflow_status(run_id)?;
    if let Some(steps) = value.get("steps").and_then(serde_json::Value::as_array) {
        for step in steps {
            if !step_matches(step, title) {
                continue;
            }
            return step
                .get("status")
                .and_then(serde_json::Value::as_str)
                .and_then(step_status);
        }
        // A workflow names its children; a step that is not in the file is a
        // different delegation, not the whole run's state.
        if !steps.is_empty() {
            return None;
        }
    }
    value
        .get("state")
        .and_then(serde_json::Value::as_str)
        .and_then(step_status)
}

fn step_status(value: &str) -> Option<SubagentStatus> {
    match value {
        "running" | "started" | "active" | "in_progress" => Some(SubagentStatus::Running),
        "requires_action" | "blocked" => Some(SubagentStatus::RequiresAction),
        "completed" | "succeeded" | "done" => Some(SubagentStatus::Idle),
        "failed" | "error" => Some(SubagentStatus::Error),
        "cancelled" | "canceled" => Some(SubagentStatus::Cancelled),
        _ => None,
    }
}

fn step_matches(step: &serde_json::Value, title: &str) -> bool {
    ["label", "workflowKey", "childId"]
        .iter()
        .any(|key| step.get(*key).and_then(serde_json::Value::as_str) == Some(title))
}

fn workflow_child_run(run_id: &str, title: &str) -> Option<String> {
    for dir in workflow_run_dirs(run_id) {
        if let Ok(text) = std::fs::read_to_string(dir.join("status.json")) {
            if let Some(run) = child_run_from_status(&text, title) {
                return Some(run);
            }
        }
        if let Ok(text) = std::fs::read_to_string(dir.join("workflow-children.jsonl")) {
            if let Some(run) = child_run_from_children(&text, title) {
                return Some(run);
            }
        }
    }
    None
}

fn read_workflow_status(run_id: &str) -> Option<serde_json::Value> {
    for dir in workflow_run_dirs(run_id) {
        let Ok(text) = std::fs::read_to_string(dir.join("status.json")) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str(&text) {
            return Some(value);
        }
    }
    None
}

fn workflow_run_dirs(run_id: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("pi-subagents-uid-")
        {
            continue;
        }
        let dir = entry.path().join("async-subagent-runs").join(run_id);
        if dir.is_dir() {
            out.push(dir);
        }
    }
    out
}

fn child_run_from_status(text: &str, title: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    for step in value.get("steps").and_then(serde_json::Value::as_array)? {
        if !step_matches(step, title) {
            continue;
        }
        let run = step.get("runId").and_then(serde_json::Value::as_str)?;
        if valid_run_id(run) {
            return Some(run.to_owned());
        }
    }
    None
}

fn child_run_from_children(text: &str, title: &str) -> Option<String> {
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(serde_json::Value::as_str) != Some("start") {
            continue;
        }
        let matches = ["key", "childId", "label"]
            .iter()
            .any(|key| value.get(*key).and_then(serde_json::Value::as_str) == Some(title));
        if !matches {
            continue;
        }
        let run = value.get("runId").and_then(serde_json::Value::as_str)?;
        if valid_run_id(run) {
            return Some(run.to_owned());
        }
    }
    None
}

fn session_file(run_id: &str) -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("HOME")?).join(".pi/agent/sessions");
    for cwd in std::fs::read_dir(root).ok()?.flatten() {
        let Ok(parents) = std::fs::read_dir(cwd.path()) else {
            continue;
        };
        for parent in parents.flatten() {
            let file = parent.path().join(run_id).join("run-0/session.jsonl");
            if file.is_file() {
                return Some(file);
            }
        }
    }
    None
}

fn parse_session(text: &str) -> Vec<SubagentLogEntryDto> {
    let mut entries = Vec::new();
    let mut saw_user = false;
    for line in text.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event.get("type").and_then(serde_json::Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = event.get("message") else {
            continue;
        };
        let ts = entry_ts(&event, message);
        let role = message
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("agent");
        if role == "system" {
            continue;
        }
        let tool_name = message
            .get("toolName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("tool");
        let mut parts: Vec<String> = Vec::new();
        match message.get("content") {
            Some(serde_json::Value::String(value)) => parts.push(value.clone()),
            Some(serde_json::Value::Array(blocks)) => {
                for block in blocks {
                    match block.get("type").and_then(serde_json::Value::as_str) {
                        Some("text") => {
                            if let Some(value) =
                                block.get("text").and_then(serde_json::Value::as_str)
                            {
                                parts.push(value.to_owned());
                            }
                        }
                        Some("toolCall") => {
                            let name = block
                                .get("name")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("tool");
                            let args = block
                                .get("arguments")
                                .map(summarize_arguments)
                                .unwrap_or_default();
                            parts.push(format!("→ {name}{args}"));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        let text = parts.join("\n").trim().to_owned();
        if text.is_empty() {
            continue;
        }
        // The first user message is the brief; later ones are the parent
        // steering the child (or the runtime nudging it). Keeping them apart
        // is what makes the parent's side of the conversation visible.
        let role = match role {
            "user" if !saw_user => {
                saw_user = true;
                "task"
            }
            "user" if runtime_notice(&text) => "notice",
            "user" => "parent",
            "assistant" => "agent",
            "toolResult" => "tool",
            other => other,
        };
        let text = if role == "tool" {
            format!("[{tool_name}]\n{text}")
        } else {
            text
        };
        entries.push(SubagentLogEntryDto {
            role: role.to_owned(),
            text: truncate(&text, 4000),
            ts,
        });
    }
    let keep = entries.len().saturating_sub(300);
    entries.split_off(keep)
}

/// Budget and context warnings are injected into the session as user
/// messages, but they are the harness talking, not the parent agent.
fn runtime_notice(text: &str) -> bool {
    let head = text.trim_start().to_ascii_lowercase();
    [
        "tool budget",
        "run budget",
        "time budget",
        "turn budget",
        "budget soft limit",
        "soft limit reached",
    ]
    .iter()
    .any(|prefix| head.starts_with(prefix))
}

/// The session log keeps an ISO timestamp on the envelope and, usually, a
/// copy on the message itself. Missing or malformed values stay 0 so the UI
/// can fall back to its own ordering.
fn entry_ts(event: &serde_json::Value, message: &serde_json::Value) -> i64 {
    let raw = event
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .or_else(|| message.get("timestamp").and_then(serde_json::Value::as_str));
    raw.and_then(|value| value.parse::<jiff::Timestamp>().ok())
        .map(|value| value.as_millisecond())
        .unwrap_or(0)
}

fn summarize_arguments(value: &serde_json::Value) -> String {
    let Some(map) = value.as_object() else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    for (key, value) in map.iter().take(3) {
        let text = match value {
            serde_json::Value::String(value) => value.clone(),
            other => other.to_string(),
        };
        parts.push(format!("{key}={}", truncate(text.trim(), 120)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("({})", parts.join(", "))
    }
}

fn truncate(value: &str, limit: usize) -> String {
    let mut out: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod log_tests {
    use super::*;

    #[test]
    fn run_ids_must_be_uuids() {
        assert!(valid_run_id("ec23a790-52c7-443e-ae0a-8487d5b4bef3"));
        assert!(!valid_run_id("../etc/passwd"));
        assert!(!valid_run_id(""));
    }

    #[test]
    fn active_workflow_children_resolve_and_refresh_status() {
        let workflow = format!(
            "00000000-0000-4000-8000-{:012x}",
            now_ms() as u128 & 0xffff_ffff_ffff
        );
        let child = "6be7ad46-2deb-4c01-b08f-e7af5a62b785";
        let root = std::env::temp_dir().join(format!(
            "pi-subagents-uid-threadbase-test-{}",
            std::process::id()
        ));
        let dir = root.join("async-subagent-runs").join(&workflow);
        std::fs::create_dir_all(&dir).unwrap();
        let write_status = |status: &str| {
            std::fs::write(
                dir.join("status.json"),
                serde_json::json!({
                    "state": "running",
                    "steps": [{
                        "label": "task-1",
                        "status": status,
                        "runId": child
                    }]
                })
                .to_string(),
            )
            .unwrap();
        };

        write_status("running");
        assert_eq!(
            child_run_from_status(
                &std::fs::read_to_string(dir.join("status.json")).unwrap(),
                "task-1"
            ),
            Some(child.to_owned())
        );
        assert_eq!(
            workflow_child_run(&workflow, "task-1"),
            Some(child.to_owned())
        );
        assert_eq!(
            workflow_step_status(&workflow, "task-1"),
            Some(SubagentStatus::Running)
        );
        assert_eq!(
            workflow_step_status(&workflow, "task-2"),
            None,
            "a title that is not a step must not inherit the run state"
        );

        let mut row = SubagentRow {
            id: "call/task-1".to_owned(),
            thread_id: "t".to_owned(),
            parent_id: Some("call".to_owned()),
            title: "task-1".to_owned(),
            description: None,
            status: "idle".to_owned(),
            cwd: Some(format!("pi-subagent:{workflow}")),
            can_cancel: 0,
            usage_json: None,
            last_seq: 0,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        apply_live_status(&mut row);
        assert_eq!(row.status, "running");

        write_status("completed");
        apply_live_status(&mut row);
        assert_eq!(row.status, "idle");

        // Older pi builds only write the child index, not the full status.
        std::fs::remove_file(dir.join("status.json")).unwrap();
        let line = serde_json::json!({
            "type": "start",
            "key": "task-1",
            "runId": child
        });
        std::fs::write(dir.join("workflow-children.jsonl"), format!("{line}\n")).unwrap();
        assert_eq!(
            workflow_child_run(&workflow, "task-1"),
            Some(child.to_owned())
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn delegation_run_ids_come_from_tool_results() {
        let pool = crate::db::connect_memory().await.unwrap();
        let now = now_ms();
        for statement in [
            "INSERT INTO users(id, username, display_name, created_at)
             VALUES('u', 'ada', 'Ada', 1)",
            "INSERT INTO projects(id, name, path, default_branch, created_by,
                created_at, updated_at)
             VALUES('p', 'repo', '/tmp/repo', 'main', 'u', 1, 1)",
            "INSERT INTO threads(id, project_id, title, status, agent, branch,
                base_branch, created_by, created_at, updated_at)
             VALUES('t', 'p', 'Delegations', 'idle', 'pi', 'thread/t', 'main',
                'u', 1, 1)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        let insert = |seq: i64, payload: String| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO events(thread_id, seq, ts, actor_kind, origin,
                        kind, payload_json)
                     VALUES('t', ?, ?, 'agent', 'agent', 'tool_call_update', ?)",
                )
                .bind(seq)
                .bind(now)
                .bind(payload)
                .execute(&pool)
                .await
                .unwrap();
            }
        };
        let single = serde_json::json!({
            "type": "tool_call_update",
            "call": {
                "id": "call_single",
                "status": "completed",
                "output": serde_json::json!({
                    "content": [],
                    "details": {
                        "runId": "ec23a790-52c7-443e-ae0a-8487d5b4bef3",
                        "toolCallId": "call_single"
                    }
                })
                .to_string()
            }
        })
        .to_string();
        insert(1, single).await;
        let workflow = serde_json::json!({
            "type": "tool_call_update",
            "call": {
                "id": "call_status",
                "status": "completed",
                "output": serde_json::json!({
                    "details": {
                        "workflowChildren": {
                            "parentToolCallId": "call_workflow",
                            "children": [{
                                "childId": "buffer",
                                "runId": "1fca55a5-2c0f-4a6e-91a9-5df5c7bd8ec1"
                            }]
                        }
                    }
                })
                .to_string()
            }
        })
        .to_string();
        insert(2, workflow).await;

        assert_eq!(
            event_run_id(&pool, "t", "call_single", None, "worker").await,
            Some("ec23a790-52c7-443e-ae0a-8487d5b4bef3".to_owned())
        );
        // Workflow children are addressed by index in the row id, but the
        // receipt names them by title.
        assert_eq!(
            event_run_id(
                &pool,
                "t",
                "call_workflow/1",
                Some("call_workflow"),
                "buffer"
            )
            .await,
            Some("1fca55a5-2c0f-4a6e-91a9-5df5c7bd8ec1".to_owned())
        );
        assert_eq!(
            event_run_id(&pool, "t", "call_missing", None, "worker").await,
            None
        );
    }

    #[test]
    fn session_lines_become_readable_entries() {
        let text = r#"{"type":"session"}
{"type":"message","timestamp":"2026-11-14T17:28:38.899Z","message":{"role":"user","content":[{"type":"text","text":"Do the task."}]}}
{"type":"message","message":{"role":"assistant","timestamp":"2026-11-14T17:28:39.500Z","content":[{"type":"text","text":"On it."},{"type":"toolCall","name":"read","arguments":{"path":"/tmp/a.md"}}]}}
{"type":"message","message":{"role":"toolResult","toolName":"read","content":[{"type":"text","text":"file body"}]}}
{"type":"message","timestamp":"2026-11-14T17:29:00.000Z","message":{"role":"user","content":[{"type":"text","text":"Mid-run steering from the parent orchestrator:\n\nDo X."}]}}
{"type":"message","timestamp":"2026-11-14T17:30:00.000Z","message":{"role":"user","content":[{"type":"text","text":"Tool budget soft limit reached after 240 tool calls."}]}}"#;
        let entries = parse_session(text);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].role, "task");
        assert_eq!(entries[0].text, "Do the task.");
        assert_eq!(
            entries[0].ts, 1_794_677_318_899,
            "the envelope timestamp wins when the message has none"
        );
        assert_eq!(entries[1].role, "agent");
        assert!(entries[1].text.contains("→ read(path=/tmp/a.md)"));
        assert_eq!(entries[1].ts, 1_794_677_319_500);
        assert_eq!(entries[2].role, "tool");
        assert!(entries[2].text.starts_with("[read]"));
        assert_eq!(entries[2].ts, 0, "no timestamp is not a crash");
        assert_eq!(entries[3].role, "parent");
        assert_eq!(entries[4].role, "notice");
    }
}
