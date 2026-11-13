//! Subagents derived from adapter tool calls.
//!
//! pi reports delegations as `subagent` / `subagent_launch` tool calls instead
//! of ACP subagent events. Threadbase mirrors them into its subagent table so
//! the sidebar can render the delegation tree; recent history is backfilled at
//! startup so a thread that ran before this bridge still gets its tree.

use crate::error::ServerError;
use crate::projection::Projection;
use crate::subagents;
use sqlx::SqlitePool;
use threadbase_shared::{
    now_ms, SubagentDto, SubagentStatus, ThreadEvent, ToolCallData, ToolStatus,
};

use super::mapping;

pub(crate) struct DerivedSubagent {
    pub(crate) id: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) status: SubagentStatus,
    /// `pi-subagent:<workflow run id>` for launches, so the log route can find
    /// the child's session file.
    pub(crate) marker: Option<String>,
}

/// Map one delegation tool call onto a subagent row. Workflow containers
/// (`subagent` with a script) are skipped: their `subagent_launch` children
/// carry the real identities.
pub(crate) fn derive(call: &ToolCallData) -> Option<DerivedSubagent> {
    let status = match call.status {
        ToolStatus::Pending => SubagentStatus::Idle,
        ToolStatus::InProgress => SubagentStatus::Running,
        ToolStatus::Completed => SubagentStatus::Idle,
        ToolStatus::Failed => SubagentStatus::Error,
    };
    let input: serde_json::Value = call
        .input
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())?;
    match call.title.as_str() {
        "subagent_launch" => {
            let title = input
                .get("key")
                .and_then(serde_json::Value::as_str)
                .or_else(|| input.get("agent").and_then(serde_json::Value::as_str))?;
            let task = input.get("task").and_then(serde_json::Value::as_str);
            let run_id = input
                .get("workflowRunId")
                .and_then(serde_json::Value::as_str)
                .map(|run| format!("pi-subagent:{run}"));
            Some(DerivedSubagent {
                id: call.id.0.clone(),
                parent_id: call
                    .id
                    .0
                    .rsplit_once('/')
                    .map(|(parent, _)| parent.to_owned()),
                title: title.to_owned(),
                description: task.map(first_line),
                status,
                marker: run_id,
            })
        }
        "subagent" => {
            if input.get("script").is_some()
                || input.get("workflow").and_then(serde_json::Value::as_bool) == Some(true)
            {
                return None;
            }
            let agent = input.get("agent").and_then(serde_json::Value::as_str)?;
            let task = input.get("task").and_then(serde_json::Value::as_str)?;
            Some(DerivedSubagent {
                id: call.id.0.clone(),
                parent_id: None,
                title: agent.to_owned(),
                description: Some(first_line(task)),
                status,
                marker: None,
            })
        }
        _ => None,
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let mut out: String = line.chars().take(140).collect();
    if line.chars().count() > 140 {
        out.push('…');
    }
    out
}

/// Persist a derived node (and, for live calls, publish it to the thread).
pub(crate) async fn apply(
    pool: &SqlitePool,
    projection: &Projection,
    thread_id: &str,
    node: &DerivedSubagent,
) -> Result<Option<SubagentDto>, ServerError> {
    let current = subagents::status_of(pool, &node.id).await.ok();
    if current == Some(node.status) {
        return Ok(None);
    }
    let status = match subagents::upsert(
        pool,
        projection,
        subagents::Upsert {
            thread_id,
            id: &node.id,
            parent_id: node.parent_id.as_deref(),
            title: &node.title,
            description: node.description.as_deref(),
            cwd: node.marker.as_deref(),
            can_cancel: false,
            usage_json: None,
            action: mapping::subagent_action(node.status),
        },
    )
    .await
    {
        Ok(status) => status,
        Err(error) => {
            tracing::warn!(%error, subagent_id = %node.id, "illegal derived subagent transition");
            current.unwrap_or(node.status)
        }
    };
    let now = now_ms();
    Ok(Some(SubagentDto {
        id: node.id.clone().into(),
        thread_id: thread_id.to_owned().into(),
        parent_id: node.parent_id.clone().map(Into::into),
        title: node.title.clone(),
        description: node.description.clone(),
        status,
        cwd: node.marker.clone(),
        can_cancel: false,
        usage: None,
        last_seq: 0,
        created_at: now,
        updated_at: now,
    }))
}

/// Rebuild subagent rows for threads that ran before the bridge existed.
/// Threads that already have rows are left alone; live events keep them fresh.
pub async fn backfill(pool: &SqlitePool, projection: &Projection) -> Result<(), ServerError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT e.thread_id, e.payload_json FROM events e
          WHERE e.kind IN ('tool_call', 'tool_call_update')
            AND e.payload_json LIKE '%\"title\":\"subagent%'
            AND NOT EXISTS (SELECT 1 FROM subagents s WHERE s.thread_id = e.thread_id)
          ORDER BY e.thread_id, e.seq",
    )
    .fetch_all(pool)
    .await?;
    for (thread_id, payload) in rows {
        let Ok(event) = serde_json::from_str::<ThreadEvent>(&payload) else {
            continue;
        };
        let call = match event {
            ThreadEvent::ToolCall { call } | ThreadEvent::ToolCallUpdate { call } => call,
            _ => continue,
        };
        if let Some(node) = derive(&call) {
            apply(pool, projection, &thread_id, &node).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_shared::{ThreadId, ToolCallId, ToolKind};

    fn call(id: &str, title: &str, status: ToolStatus, input: &str) -> ToolCallData {
        ToolCallData {
            id: ToolCallId::from(id),
            thread_id: ThreadId::from("t"),
            title: title.to_owned(),
            kind: ToolKind::Other,
            status,
            input: Some(input.to_owned()),
            output: None,
            locations: Vec::new(),
            diff: None,
            terminal_id: None,
            subagent_id: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn launch_calls_become_tree_nodes() {
        let node = derive(&call(
            "call_1/2",
            "subagent_launch",
            ToolStatus::InProgress,
            r#"{"key":"buffer","agent":"worker","task":"Read the brief.\nThen implement."}"#,
        ))
        .unwrap();
        assert_eq!(node.id, "call_1/2");
        assert_eq!(node.parent_id.as_deref(), Some("call_1"));
        assert_eq!(node.title, "buffer");
        assert_eq!(node.description.as_deref(), Some("Read the brief."));
        assert_eq!(node.status, SubagentStatus::Running);
    }

    #[test]
    fn workflow_containers_are_skipped() {
        assert!(derive(&call(
            "call_1",
            "subagent",
            ToolStatus::InProgress,
            r#"{"options":{"script":"..."},"worktree":true}"#,
        ))
        .is_none());
    }

    #[test]
    fn single_delegations_are_nodes() {
        let node = derive(&call(
            "call_2",
            "subagent",
            ToolStatus::Completed,
            r#"{"agent":"reviewer","task":"Review the diff."}"#,
        ))
        .unwrap();
        assert_eq!(node.title, "reviewer");
        assert_eq!(node.status, SubagentStatus::Idle);
    }

    #[test]
    fn unrelated_tool_calls_are_ignored() {
        assert!(derive(&call("call_3", "shell", ToolStatus::Completed, "{}")).is_none());
    }
}
