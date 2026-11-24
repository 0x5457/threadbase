//! Minimal Codex app-server used by the native contract tests.
//!
//! It speaks the same newline-delimited JSON as `codex app-server` (no
//! `jsonrpc` field) and scripts one turn: a message, a command, a file change,
//! an approval request, then completion.

use std::io::{self, BufRead, BufReader, BufWriter, Write};

use serde_json::{json, Value};

fn main() {
    if let Err(error) = run() {
        eprintln!("codex fixture: {error}");
        std::process::exit(2);
    }
}

fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = BufWriter::new(stdout.lock());
    let mut turn = false;
    while let Some(message) = read_message(&mut reader)? {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, id) {
            ("initialize", Some(id)) => respond(
                &mut out,
                &id,
                json!({"userAgent": "codex-cli/0.171.0 (fake)", "codexHome": "/tmp"}),
            )?,
            ("model/list", Some(id)) => respond(
                &mut out,
                &id,
                json!({"data": [{"id": "fake-model", "displayName": "Fake model"}]}),
            )?,
            ("thread/start", Some(id)) => {
                respond(
                    &mut out,
                    &id,
                    json!({
                        "thread": {"id": "thread-1"},
                        "model": "fake-model",
                        "reasoningEffort": "high",
                    }),
                )?;
            }
            ("thread/list", Some(id)) => {
                let cwd =
                    std::env::var("CODEX_FIXTURE_CWD").unwrap_or_else(|_| "/tmp/old".to_owned());
                respond(
                    &mut out,
                    &id,
                    json!({"data": [
                        {"id": "thread-0", "name": "Old thread", "preview": "old question",
                         "cwd": cwd, "updatedAt": 123},
                        {"id": "thread-1", "name": "Empty thread", "preview": "",
                         "cwd": cwd, "updatedAt": 122}
                    ]}),
                )?
            }
            ("thread/turns/list", Some(id)) => respond(
                &mut out,
                &id,
                json!({
                    "data": [{
                        "id": "turn-old",
                        "status": "completed",
                        "completedAt": 1700000000,
                        "items": [
                            {"type": "userMessage", "id": "u1",
                             "content": [{"type": "text", "text": "old question"}]},
                            {"type": "agentMessage", "id": "a1", "text": "old answer"},
                            {"type": "commandExecution", "id": "c1", "command": "ls",
                             "commandActions": [], "cwd": "/tmp", "status": "completed",
                             "aggregatedOutput": "file\n"}
                        ]
                    }],
                    "nextCursor": null
                }),
            )?,
            ("thread/resume", Some(id)) => {
                let thread = params
                    .get("threadId")
                    .cloned()
                    .unwrap_or_else(|| json!("thread-1"));
                respond(
                    &mut out,
                    &id,
                    json!({"thread": {"id": thread}, "model": "fake-model"}),
                )?;
            }
            ("turn/start", Some(id)) => {
                turn = true;
                respond(
                    &mut out,
                    &id,
                    json!({"turn": {"id": "turn-1", "status": "inProgress", "items": []}}),
                )?;
                notify(
                    &mut out,
                    "turn/started",
                    json!({"threadId": "thread-1", "turn": {"id": "turn-1", "status": "inProgress", "items": []}}),
                )?;
                notify(
                    &mut out,
                    "item/agentMessage/delta",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "itemId": "msg-1", "delta": "Hello"}),
                )?;
                notify(
                    &mut out,
                    "item/started",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "startedAtMs": 0, "item": {
                        "type": "commandExecution", "id": "cmd-1", "command": "echo hi",
                        "commandActions": [], "cwd": "/tmp/work", "status": "inProgress"
                    }}),
                )?;
                notify(
                    &mut out,
                    "item/commandExecution/outputDelta",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "itemId": "cmd-1", "delta": "hi\n"}),
                )?;
                notify(
                    &mut out,
                    "item/completed",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "completedAtMs": 1, "item": {
                        "type": "commandExecution", "id": "cmd-1", "command": "echo hi",
                        "commandActions": [], "cwd": "/tmp/work", "status": "completed",
                        "aggregatedOutput": "hi\n", "exitCode": 0
                    }}),
                )?;
                notify(
                    &mut out,
                    "item/started",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "startedAtMs": 0, "item": {
                        "type": "fileChange", "id": "file-1", "status": "inProgress",
                        "changes": [{"path": "/tmp/work/a.rs", "kind": "update",
                            "diff": "--- a\n+++ b\n@@ -1 +1 @@\n-a\n+b\n"}]
                    }}),
                )?;
                notify(
                    &mut out,
                    "item/completed",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "completedAtMs": 1, "item": {
                        "type": "fileChange", "id": "file-1", "status": "completed",
                        "changes": [{"path": "/tmp/work/a.rs", "kind": "update",
                            "diff": "--- a\n+++ b\n@@ -1 +1 @@\n-a\n+b\n"}]
                    }}),
                )?;
                // The client must answer this before the turn continues.
                write_message(
                    &mut out,
                    json!({
                        "id": "approval-1",
                        "method": "item/commandExecution/requestApproval",
                        "params": {
                            "threadId": "thread-1", "turnId": "turn-1", "itemId": "cmd-1",
                            "startedAtMs": 0, "command": "rm -rf /tmp/x", "cwd": "/tmp/work",
                            "reason": "test approval"
                        }
                    }),
                )?;
            }
            ("turn/steer", Some(id)) => respond(&mut out, &id, json!({}))?,
            ("turn/interrupt", Some(id)) => {
                respond(&mut out, &id, json!({}))?;
                notify(
                    &mut out,
                    "turn/completed",
                    json!({"threadId": "thread-1", "turn": {"id": "turn-1", "status": "interrupted", "items": []}}),
                )?;
            }
            (_, Some(id)) if id == "approval-1" && turn => {
                turn = false;
                notify(
                    &mut out,
                    "item/agentMessage/delta",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "itemId": "msg-1", "delta": " world"}),
                )?;
                notify(
                    &mut out,
                    "item/completed",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "completedAtMs": 2, "item": {
                        "type": "agentMessage", "id": "msg-1", "text": "Hello world"
                    }}),
                )?;
                notify(
                    &mut out,
                    "thread/tokenUsage/updated",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "tokenUsage": {
                        "last": {"cachedInputTokens": 0, "inputTokens": 10, "outputTokens": 5,
                                 "reasoningOutputTokens": 0, "totalTokens": 15},
                        "total": {"cachedInputTokens": 0, "inputTokens": 10, "outputTokens": 5,
                                  "reasoningOutputTokens": 0, "totalTokens": 15},
                        "modelContextWindow": 1000
                    }}),
                )?;
                notify(
                    &mut out,
                    "turn/plan/updated",
                    json!({"threadId": "thread-1", "turnId": "turn-1", "explanation": null,
                        "plan": [{"step": "Run the command", "status": "completed"}]}),
                )?;
                notify(
                    &mut out,
                    "turn/completed",
                    json!({"threadId": "thread-1", "turn": {"id": "turn-1", "status": "completed", "items": []}}),
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let line = line.trim();
    if line.is_empty() {
        return Ok(Some(Value::Null));
    }
    serde_json::from_str(line)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn respond(out: &mut impl Write, id: &Value, result: Value) -> io::Result<()> {
    write_message(out, json!({"id": id, "result": result}))
}

fn notify(out: &mut impl Write, method: &str, params: Value) -> io::Result<()> {
    write_message(out, json!({"method": method, "params": params}))
}

fn write_message(out: &mut impl Write, message: Value) -> io::Result<()> {
    writeln!(out, "{message}")?;
    out.flush()
}
