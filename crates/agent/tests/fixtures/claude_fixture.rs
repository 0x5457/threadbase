//! Minimal Claude Code CLI speaking the stream-json control protocol.
//!
//! It scripts one turn: streamed text, a Bash tool use, a permission request,
//! then the tool result and a successful result.

use std::io::{self, BufRead, BufReader, BufWriter, Write};

use serde_json::{json, Value};

fn main() {
    if let Err(error) = run() {
        eprintln!("claude fixture: {error}");
        std::process::exit(2);
    }
}

fn run() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--version") {
        println!("2.1.330 (Claude Code)");
        return Ok(());
    }
    let session = args
        .iter()
        .position(|arg| arg == "--session-id" || arg == "--resume")
        .and_then(|index| args.get(index + 1))
        .cloned()
        .unwrap_or_else(|| "fixture-session".to_owned());
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = BufWriter::new(stdout.lock());
    write(
        &mut out,
        json!({
            "type": "system", "subtype": "init", "session_id": session,
            "model": "fake-model", "claude_code_version": "2.1.330",
            "cwd": std::env::current_dir().unwrap_or_default().to_string_lossy(),
            "tools": ["Bash"], "permissionMode": "default", "slash_commands": [],
            "output_style": "default", "skills": [], "plugins": [], "agents": [],
            "apiKeySource": "none", "mcp_servers": [], "betas": [],
        }),
    )?;
    while let Some(message) = read_message(&mut reader)? {
        match message.get("type").and_then(Value::as_str) {
            Some("control_request") => {
                let request = message.get("request").cloned().unwrap_or(Value::Null);
                let request_id = message.get("request_id").cloned().unwrap_or(Value::Null);
                let subtype = request
                    .get("subtype")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if subtype == "list_models" {
                    write(
                        &mut out,
                        json!({
                            "type": "control_response",
                            "response": {
                                "subtype": "success", "request_id": request_id,
                                "response": {"models": [
                                    {"id": "fake-model", "displayName": "Fake model"},
                                    {"id": "fake-model-2", "displayName": "Fake model 2"}
                                ]},
                            },
                        }),
                    )?;
                    continue;
                }
                control_success(&mut out, &request_id)?;
                if subtype == "interrupt" {
                    write(
                        &mut out,
                        json!({
                            "type": "result", "subtype": "error_during_execution",
                            "is_error": true, "result": "Interrupted", "session_id": session,
                            "duration_ms": 1, "duration_api_ms": 1, "num_turns": 1,
                            "usage": {"input_tokens": 1, "output_tokens": 1},
                        }),
                    )?;
                }
            }
            Some("user") => {
                write(
                    &mut out,
                    json!({
                        "type": "stream_event",
                        "event": {"type": "content_block_delta",
                                  "delta": {"type": "text_delta", "text": "Hello"}},
                        "uuid": "m-1", "session_id": session,
                    }),
                )?;
                write(
                    &mut out,
                    json!({
                        "type": "assistant", "uuid": "m-1", "session_id": session,
                        "message": {"id": "m-1", "role": "assistant", "content": [
                            {"type": "text", "text": "Hello"},
                            {"type": "tool_use", "id": "t1", "name": "Bash",
                             "input": {"command": "echo hi"}},
                            {"type": "tool_use", "id": "task-1", "name": "Task",
                             "input": {"description": "Investigate", "prompt": "Look around"}}
                        ]},
                    }),
                )?;
                write(
                    &mut out,
                    json!({
                        "type": "control_request", "request_id": "perm-1",
                        "request": {
                            "subtype": "can_use_tool", "tool_name": "Bash",
                            "input": {"command": "echo hi"}, "tool_use_id": "t1",
                            "description": "Run the command",
                        },
                    }),
                )?;
            }
            Some("control_response") => {
                let response = message.get("response").cloned().unwrap_or(Value::Null);
                if response.get("request_id").and_then(Value::as_str) == Some("perm-1") {
                    write(
                        &mut out,
                        json!({
                            "type": "user", "session_id": session,
                            "message": {"role": "user", "content": [
                                {"type": "tool_result", "tool_use_id": "t1", "content": "hi\n"},
                                {"type": "tool_result", "tool_use_id": "task-1", "content": "done",
                                 "is_error": false}
                            ]},
                            "tool_use_result": {"usage": {"input_tokens": 7, "output_tokens": 3}},
                        }),
                    )?;
                    write(
                        &mut out,
                        json!({
                            "type": "result", "subtype": "success", "is_error": false,
                            "result": "done", "session_id": session,
                            "duration_ms": 1, "duration_api_ms": 1, "num_turns": 1,
                            "usage": {"input_tokens": 3, "output_tokens": 2},
                        }),
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn control_success(out: &mut impl Write, request_id: &Value) -> io::Result<()> {
    write(
        out,
        json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": {}},
        }),
    )
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

fn write(out: &mut impl Write, message: Value) -> io::Result<()> {
    writeln!(out, "{message}")?;
    out.flush()
}
