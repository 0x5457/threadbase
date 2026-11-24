//! Claude Code session files: discovery and transcript import.
//!
//! Claude keeps one JSONL per conversation under
//! `~/.claude/projects/<cwd-slug>/<session-id>.jsonl`. Reading it directly
//! avoids replaying a session just to import it, and works while the CLI owns
//! the conversation.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;
use similar::{ChangeTag, TextDiff};

use crate::{
    ExternalSession, HarnessError, HarnessResult, MessageRole, SessionHistory, SessionItem,
    SessionTurn, ToolCallEvent,
};
use threadbase_shared::{DiffChangeDto, DiffSummary, ToolKind, ToolLocation, ToolStatus};

const MAX_SESSION_BYTES: u64 = 64 * 1024 * 1024;

/// Sessions in one project directory, newest first. Only top-level
/// conversations are listed; subagent sidechains belong to their parent.
pub fn list(cwd: &Path) -> Vec<ExternalSession> {
    let Some(root) = config_root() else {
        return Vec::new();
    };
    list_in(&root, cwd)
}

/// `root` is the Claude config directory (`~/.claude`).
pub(crate) fn list_in(root: &Path, cwd: &Path) -> Vec<ExternalSession> {
    let directory = project_dir(root, cwd);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let updated_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        let Some((session_cwd, title)) = peek(&path) else {
            continue;
        };
        if session_cwd != cwd.to_string_lossy() {
            continue;
        }
        sessions.push(ExternalSession {
            id: id.to_owned(),
            title,
            cwd: session_cwd,
            updated_at,
        });
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

/// One conversation in the shape the thread log imports.
pub fn read(cwd: &Path, session: &str) -> HarnessResult<SessionHistory> {
    let root = config_root().ok_or_else(|| HarnessError::SessionNotFound(session.to_owned()))?;
    read_in(&root, cwd, session)
}

pub(crate) fn read_in(root: &Path, cwd: &Path, session: &str) -> HarnessResult<SessionHistory> {
    if session.contains('/') || session.contains('\\') || session.contains("..") {
        return Err(HarnessError::Protocol(
            "invalid Claude session id".to_owned(),
        ));
    }
    let directory = project_dir(root, cwd);
    let path = directory.join(format!("{session}.jsonl"));
    let metadata =
        std::fs::metadata(&path).map_err(|_| HarnessError::SessionNotFound(session.to_owned()))?;
    if metadata.len() > MAX_SESSION_BYTES {
        return Err(HarnessError::Protocol(
            "the Claude session is too large to import".to_owned(),
        ));
    }
    let file = std::fs::File::open(&path).map_err(HarnessError::Io)?;
    let mut history = SessionHistory::default();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        match message.get("type").and_then(Value::as_str) {
            Some("user") => user_message(&mut history, &message),
            Some("assistant") => assistant_message(&mut history, &message),
            _ => {}
        }
    }
    Ok(history)
}

fn config_root() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
}

/// `<root>/projects/<slug>`.
fn project_dir(root: &Path, cwd: &Path) -> PathBuf {
    let slug = cwd.to_string_lossy().replace(['/', '\\'], "-");
    root.join("projects").join(slug)
}

/// Session cwd and title from the first few records; the title is the first
/// real user prompt, which is what the CLI's picker shows.
fn peek(path: &Path) -> Option<(String, String)> {
    let file = std::fs::File::open(path).ok()?;
    let mut cwd = None;
    let mut title = String::new();
    for line in BufReader::new(file).lines().take(200) {
        let Ok(line) = line else {
            break;
        };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if cwd.is_none() {
            cwd = message
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if title.is_empty() && message.get("type").and_then(Value::as_str) == Some("user") {
            title = prompt_text(&message).unwrap_or_default();
        }
        if cwd.is_some() && !title.is_empty() {
            break;
        }
    }
    let cwd = cwd?;
    Some((cwd, title.chars().take(120).collect()))
}

fn user_message(history: &mut SessionHistory, message: &Value) {
    if message.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let Some(content) = message.pointer("/message/content") else {
        return;
    };
    let blocks = match content {
        Value::String(text) => {
            if !text.trim().is_empty() {
                history.turns.push(SessionTurn {
                    user: text.clone(),
                    ..SessionTurn::default()
                });
            }
            return;
        }
        Value::Array(blocks) => blocks,
        _ => return,
    };
    record_time(history, message);
    let mut result_text = String::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_result") => {
                let id = block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(text) = block.get("content").and_then(content_text) {
                    result_text.push_str(&text);
                }
                let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
                patch_tool(history, id, &result_text, failed);
            }
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        history.turns.push(SessionTurn {
                            user: text.to_owned(),
                            ..SessionTurn::default()
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

fn assistant_message(history: &mut SessionHistory, message: &Value) {
    let Some(blocks) = message
        .pointer("/message/content")
        .and_then(Value::as_array)
    else {
        return;
    };
    if history.turns.is_empty() {
        history.turns.push(SessionTurn::default());
    }
    record_time(history, message);
    let turn = history.turns.last_mut().expect("just ensured");
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        turn.items.push(SessionItem::Message {
                            role: MessageRole::Agent,
                            text: text.to_owned(),
                        });
                    }
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let name = block.get("name").and_then(Value::as_str).unwrap_or("Tool");
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                turn.items
                    .push(SessionItem::Tool(Box::new(tool_event(&id, name, input))));
            }
            _ => {}
        }
    }
}

/// The last timestamp seen in a turn is its completion time for backfill.
fn record_time(history: &mut SessionHistory, message: &Value) {
    let Some(raw) = message.get("timestamp").and_then(Value::as_str) else {
        return;
    };
    let Ok(timestamp) = raw.parse::<jiff::Timestamp>() else {
        return;
    };
    let at = timestamp.as_millisecond();
    if let Some(turn) = history.turns.last_mut() {
        turn.completed_at = Some(turn.completed_at.map_or(at, |current| current.max(at)));
    }
}

fn patch_tool(history: &mut SessionHistory, id: &str, output: &str, failed: bool) {
    for turn in history.turns.iter_mut().rev() {
        for item in turn.items.iter_mut().rev() {
            let SessionItem::Tool(tool) = item else {
                continue;
            };
            if tool.id == id {
                tool.output = Some(output.to_owned());
                tool.status = if failed {
                    ToolStatus::Failed
                } else {
                    ToolStatus::Completed
                };
                return;
            }
        }
    }
}

/// A prompt is a string or the first text block; `tool_result` blocks are not
/// prompts.
fn prompt_text(message: &Value) -> Option<String> {
    match message.pointer("/message/content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => blocks.iter().find_map(|block| {
            (block.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| block.get("text").and_then(Value::as_str))
                .flatten()
                .map(str::to_owned)
        }),
        _ => None,
    }
}

fn content_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => Some(
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// A `tool_use` block in transcript shape; shared with the live transport.
pub(crate) fn tool_event(id: &str, name: &str, input: Value) -> ToolCallEvent {
    ToolCallEvent {
        id: id.to_owned(),
        title: tool_title(name, &input),
        kind: tool_kind(name),
        status: ToolStatus::InProgress,
        input: Some(input.to_string()),
        output: None,
        locations: tool_locations(&input),
        terminal_id: None,
        diff: tool_diff(name, &input),
        raw: input,
    }
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "Read" | "NotebookRead" => ToolKind::Read,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ToolKind::Edit,
        "Bash" | "BashOutput" => ToolKind::Execute,
        "Grep" | "Glob" => ToolKind::Search,
        "WebFetch" => ToolKind::Fetch,
        "WebSearch" => ToolKind::Search,
        _ => ToolKind::Other,
    }
}

fn tool_title(name: &str, input: &Value) -> String {
    let detail = [
        "file_path",
        "command",
        "pattern",
        "query",
        "description",
        "url",
    ]
    .iter()
    .find_map(|key| input.get(*key).and_then(Value::as_str))
    .unwrap_or("");
    if detail.is_empty() {
        name.to_owned()
    } else {
        format!("{name} {}", detail.chars().take(120).collect::<String>())
    }
}

fn tool_locations(input: &Value) -> Vec<ToolLocation> {
    input
        .get("file_path")
        .and_then(Value::as_str)
        .map(|path| {
            vec![ToolLocation {
                path: path.to_owned(),
                line: None,
            }]
        })
        .unwrap_or_default()
}

fn tool_diff(name: &str, input: &Value) -> Option<DiffSummary> {
    let path = input.get("file_path").and_then(Value::as_str)?;
    let (old, new) = match name {
        "Edit" => (
            input
                .get("old_string")
                .and_then(Value::as_str)
                .unwrap_or(""),
            input.get("new_string").and_then(Value::as_str)?,
        ),
        "Write" => ("", input.get("content").and_then(Value::as_str)?),
        _ => return None,
    };
    let changes = TextDiff::from_lines(old, new);
    let additions = changes
        .iter_all_changes()
        .filter(|change| change.tag() == ChangeTag::Insert)
        .count();
    let deletions = changes
        .iter_all_changes()
        .filter(|change| change.tag() == ChangeTag::Delete)
        .count();
    let mut patch = format!("--- a/{path}\n+++ b/{path}\n");
    patch.push_str(&changes.unified_diff().context_radius(3).to_string());
    Some(DiffSummary {
        path: path.to_owned(),
        additions: u32::try_from(additions).unwrap_or(u32::MAX),
        deletions: u32::try_from(deletions).unwrap_or(u32::MAX),
        patch: Some(patch),
        changes: vec![DiffChangeDto {
            operation: if old.is_empty() { "add" } else { "modify" }.to_owned(),
            path: path.to_owned(),
            old_path: None,
            file_type: Some("text".to_owned()),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session(root: &Path, cwd: &Path, id: &str, lines: &[Value]) {
        let dir = root
            .join(".claude/projects")
            .join(cwd.to_string_lossy().replace(['/', '\\'], "-"));
        std::fs::create_dir_all(&dir).expect("session dir");
        let body = lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(dir.join(format!("{id}.jsonl")), body).expect("session file");
    }

    #[test]
    fn parses_iso_timestamps() {
        let timestamp = "2026-01-01T00:00:05Z"
            .parse::<jiff::Timestamp>()
            .expect("timestamp");
        assert_eq!(timestamp.as_millisecond(), 1_767_225_605_000);
    }

    #[test]
    fn lists_and_reads_a_conversation() {
        let root = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        let lines = vec![
            serde_json::json!({"type": "user", "cwd": cwd.path(), "sessionId": "s1",
                "timestamp": "2026-01-01T00:00:00Z",
                "message": {"role": "user", "content": "old question"}}),
            serde_json::json!({"type": "assistant", "cwd": cwd.path(), "sessionId": "s1",
            "timestamp": "2026-01-01T00:00:05Z",
            "message": {"role": "assistant", "content": [
                {"type": "text", "text": "old answer"},
                {"type": "tool_use", "id": "t1", "name": "Bash",
                 "input": {"command": "ls"}}
            ]}}),
            serde_json::json!({"type": "user", "cwd": cwd.path(), "sessionId": "s1",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "file\n"}
            ]}}),
        ];
        write_session(root.path(), cwd.path(), "s1", &lines);
        let config = root.path().join(".claude");

        let sessions = list_in(&config, cwd.path());
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "s1");
        assert_eq!(sessions[0].title, "old question");

        let history = read_in(&config, cwd.path(), "s1").expect("history");
        assert_eq!(history.turns.len(), 1);
        assert_eq!(history.turns[0].user, "old question");
        assert_eq!(history.turns[0].completed_at, Some(1_767_225_605_000));
        assert!(matches!(
            &history.turns[0].items[0],
            SessionItem::Message { text, .. } if text == "old answer"
        ));
        let SessionItem::Tool(tool) = &history.turns[0].items[1] else {
            panic!("tool item");
        };
        assert_eq!(tool.id, "t1");
        assert_eq!(tool.output.as_deref(), Some("file\n"));
        assert_eq!(tool.status, ToolStatus::Completed);
    }
}
