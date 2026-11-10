//! Minimal ACP v1 agent used by the harness contract tests.
//!
//! It speaks newline-delimited JSON-RPC on stdio and exposes scripted
//! scenarios selected by the prompt text, so tests can exercise streaming,
//! permissions, elicitation, steering, subagents, malformed input and crashes
//! without any model credentials.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process;

use serde_json::{json, Value};

fn main() {
    if let Err(error) = run() {
        eprintln!("fake agent: {error}");
        process::exit(2);
    }
}

fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = BufWriter::new(stdout.lock());
    let mut fixture = Fixture::new();
    while let Some(message) = read_message(&mut reader)? {
        fixture.journal_received(&message);
        fixture.dispatch(&mut reader, &mut out, message)?;
    }
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => return Ok(Some(value)),
            Err(error) => eprintln!("fake agent: ignoring unparsable input: {error}"),
        }
    }
}

fn prompt_text(params: &Value) -> String {
    // The first text block is the user prompt; any trailing blocks are
    // server-injected state summaries and must not change scenario dispatch.
    params
        .get("prompt")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn steering_result(text: &str) -> (&'static str, Option<&'static str>) {
    match text.trim() {
        "steer:startedNewTurn" => ("startedNewTurn", None),
        "steer:promptRequired" => ("promptRequired", Some("noRunningTurn")),
        "steer:failed" => ("failed", Some("boom")),
        _ => ("injected", None),
    }
}

struct Fixture {
    journal: Option<File>,
    next_request_id: u64,
    root_session: String,
}

impl Fixture {
    fn new() -> Self {
        let journal = std::env::var_os("FAKE_AGENT_JOURNAL")
            .and_then(|path| OpenOptions::new().create(true).append(true).open(path).ok());
        Self {
            journal,
            next_request_id: 1_000,
            root_session: "sess-1".to_owned(),
        }
    }

    fn journal_received(&mut self, message: &Value) {
        let Some(journal) = self.journal.as_mut() else {
            return;
        };
        let entry = json!({ "received": message });
        let _ = writeln!(journal, "{entry}");
        let _ = journal.flush();
    }

    fn send(&self, out: &mut impl Write, message: Value) -> io::Result<()> {
        writeln!(out, "{message}")?;
        out.flush()
    }

    fn respond(&self, out: &mut impl Write, id: &Value, result: Value) -> io::Result<()> {
        self.send(out, json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    fn respond_error(
        &self,
        out: &mut impl Write,
        id: &Value,
        code: i64,
        message: &str,
    ) -> io::Result<()> {
        self.send(
            out,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": code, "message": message},
            }),
        )
    }

    fn notify_update(&self, out: &mut impl Write, session: &str, update: Value) -> io::Result<()> {
        self.send(
            out,
            json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {"sessionId": session, "update": update},
            }),
        )
    }

    fn chunk(&self, out: &mut impl Write, session: &str, text: &str) -> io::Result<()> {
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": text},
            }),
        )
    }

    fn next_request(
        &mut self,
        out: &mut impl Write,
        method: &str,
        params: Value,
    ) -> io::Result<Value> {
        let id = json!(format!("fixture-{}", self.next_request_id));
        self.next_request_id += 1;
        self.send(
            out,
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )?;
        Ok(id)
    }

    fn wait_for_response(
        &mut self,
        reader: &mut impl BufRead,
        out: &mut impl Write,
        id: &Value,
    ) -> io::Result<Value> {
        loop {
            let Some(message) = read_message(reader)? else {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "client disconnected",
                ));
            };
            self.journal_received(&message);
            if message.get("id") == Some(id) {
                return Ok(message);
            }
            self.dispatch(reader, out, message)?;
        }
    }

    fn dispatch(
        &mut self,
        reader: &mut impl BufRead,
        out: &mut impl Write,
        message: Value,
    ) -> io::Result<()> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method.as_deref(), id) {
            (Some("initialize"), Some(id)) => {
                let version = params.get("protocolVersion").cloned().unwrap_or(json!(1));
                self.respond(
                    out,
                    &id,
                    json!({
                        "protocolVersion": version,
                        "agentCapabilities": {
                            "loadSession": true,
                            "promptCapabilities": {
                                "image": false,
                                "audio": false,
                                "embeddedContext": false,
                            },
                        },
                        "authMethods": [],
                        "agentInfo": {"name": "fake-agent", "version": "0.0.1"},
                        "_meta": {"steering": {"supported": true}},
                    }),
                )
            }
            (Some("session/new"), Some(id)) => {
                self.respond(out, &id, json!({"sessionId": self.root_session}))
            }
            (Some("session/load"), Some(id)) => {
                let session = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or(&self.root_session)
                    .to_owned();
                if session == "expired" {
                    self.respond_error(out, &id, -32001, "session expired")?;
                    return Ok(());
                }
                self.respond(out, &id, json!({}))?;
                self.chunk(out, &session, "replayed")
            }
            (Some("session/prompt"), Some(id)) => self.prompt(reader, out, &id, &params),
            (Some("_session/steering"), Some(id)) => {
                let (outcome, reason) = steering_result(&prompt_text(&params));
                let mut result = json!({"outcome": outcome});
                if let Some(reason) = reason {
                    result["reason"] = json!(reason);
                }
                self.respond(out, &id, result)
            }
            (Some("session/cancel"), None) => {
                let session = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if session.starts_with("sub-") {
                    let root = self.root_session.clone();
                    self.chunk(out, &root, &format!("subagent-cancelled:{session}"))
                } else {
                    Ok(())
                }
            }
            (Some(_), Some(id)) => self.respond_error(out, &id, -32601, "method not found"),
            _ => Ok(()),
        }
    }

    fn prompt(
        &mut self,
        reader: &mut impl BufRead,
        out: &mut impl Write,
        id: &Value,
        params: &Value,
    ) -> io::Result<()> {
        let text = prompt_text(params);
        let session = params
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or("sess-1")
            .to_owned();
        match text.trim() {
            "stream" => self.stream(out, id, &session),
            "permission" => self.permission(reader, out, id, &session, "permission"),
            "permission-deny" => self.permission(reader, out, id, &session, "permission-deny"),
            "permission-reject" => self.permission(reader, out, id, &session, "permission-reject"),
            "permission-none" => self.permission(reader, out, id, &session, "permission-none"),
            "elicit" => self.elicitation(reader, out, id, &session),
            "subagents" => self.subagents(out, id, &session),
            "agent-message" => self.agent_message(out, id, &session),
            "malformed" => self.malformed(out, id, &session),
            "crash" => {
                self.chunk(out, &session, "crashing")?;
                process::exit(7);
            }
            "slow" => self.slow(reader, out, id, &session),
            _ => self.respond(out, id, json!({"stopReason": "end_turn"})),
        }
    }

    fn stream(&self, out: &mut impl Write, id: &Value, session: &str) -> io::Result<()> {
        self.chunk(out, session, "Hello")?;
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "agent_thought_chunk",
                "content": {"type": "text", "text": "Considering"},
            }),
        )?;
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "call-1",
                "title": "Run tests",
                "kind": "execute",
                "status": "pending",
                "locations": [{"path": "/tmp/work/src/lib.rs", "line": 12}],
                "content": [{
                    "type": "diff",
                    "path": "/tmp/work/src/lib.rs",
                    "oldText": "fn main() {\n}\n",
                    "newText": "fn main() {\n    run();\n}\n"
                }],
                "rawInput": {"command": "cargo test"},
            }),
        )?;
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "call-1",
                "status": "in_progress",
            }),
        )?;
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "call-1",
                "status": "completed",
                "rawOutput": {"stdout": "ok"},
            }),
        )?;
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "plan",
                "entries": [
                    {"content": "Write code", "priority": "high", "status": "completed"},
                    {"content": "Run tests", "priority": "medium", "status": "in_progress"},
                ],
            }),
        )?;
        self.notify_update(
            out,
            session,
            json!({"sessionUpdate": "usage_update", "used": 1234, "size": 200000}),
        )?;
        self.respond(out, id, json!({"stopReason": "end_turn"}))
    }

    fn permission(
        &mut self,
        reader: &mut impl BufRead,
        out: &mut impl Write,
        id: &Value,
        session: &str,
        mode: &str,
    ) -> io::Result<()> {
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "call-perm",
                "title": "Write file",
                "kind": "edit",
                "status": "pending",
                "rawInput": {"command": "echo hi", "cwd": "/tmp/work"},
            }),
        )?;
        let options = match mode {
            "permission-deny" => json!([
                {"optionId": "opt-allow-once", "name": "Allow once", "kind": "allow_once"},
                {"optionId": "opt-reject-always", "name": "Never allow", "kind": "reject_always"},
            ]),
            "permission-reject" => json!([
                {"optionId": "opt-allow-once", "name": "Allow once", "kind": "allow_once"},
                {"optionId": "opt-reject-always", "name": "Never allow", "kind": "reject_always"},
                {"optionId": "opt-reject-once", "name": "Reject", "kind": "reject_once"},
            ]),
            "permission-none" => json!([
                {"optionId": "opt-allow-always", "name": "Always allow", "kind": "allow_always"},
            ]),
            _ => json!([
                {"optionId": "opt-allow-always", "name": "Always allow", "kind": "allow_always"},
                {"optionId": "opt-reject-once", "name": "Reject", "kind": "reject_once"},
                {"optionId": "opt-allow-once", "name": "Allow once", "kind": "allow_once"},
            ]),
        };
        let request_id = self.next_request(
            out,
            "session/request_permission",
            json!({
                "sessionId": session,
                "toolCall": {
                    "toolCallId": "call-perm",
                    "title": "Write file",
                    "kind": "edit",
                    "status": "pending",
                },
                "options": options,
            }),
        )?;
        let response = self.wait_for_response(reader, out, &request_id)?;
        let selected = response
            .pointer("/result/outcome/optionId")
            .and_then(Value::as_str)
            .unwrap_or("cancelled")
            .to_owned();
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "call-perm",
                "status": "completed",
                "rawOutput": {"selected": selected},
            }),
        )?;
        self.respond(out, id, json!({"stopReason": "end_turn"}))
    }

    fn elicitation(
        &mut self,
        reader: &mut impl BufRead,
        out: &mut impl Write,
        id: &Value,
        session: &str,
    ) -> io::Result<()> {
        let request_id = self.next_request(
            out,
            "elicitation/create",
            json!({
                "sessionId": session,
                "mode": "form",
                "message": "Which environment?",
                "requestedSchema": {
                    "type": "object",
                    "properties": {
                        "environment": {
                            "type": "string",
                            "enum": ["staging", "production"],
                        },
                    },
                    "required": ["environment"],
                },
            }),
        )?;
        let response = self.wait_for_response(reader, out, &request_id)?;
        let action = response
            .pointer("/result/action")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let content = response
            .pointer("/result/content")
            .cloned()
            .unwrap_or(Value::Null);
        self.chunk(out, session, &format!("elicitation:{action}"))?;
        self.chunk(out, session, &format!("elicitation-content:{content}"))?;
        self.respond(out, id, json!({"stopReason": "end_turn"}))
    }

    fn subagents(&self, out: &mut impl Write, id: &Value, session: &str) -> io::Result<()> {
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "subagent_update",
                "sessionId": "sub-1",
                "title": "Test investigator",
                "description": "Checks failures",
                "capabilities": {"cancel": {}},
                "state": {"state": "running"},
                "cwd": "/tmp/fake-worktree/sub-1",
            }),
        )?;
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "subagent_update",
                "sessionId": "sub-3",
                "title": "No cancel",
                "state": {"state": "running"},
            }),
        )?;
        self.notify_update(
            out,
            "sub-1",
            json!({
                "sessionUpdate": "subagent_update",
                "sessionId": "sub-2",
                "title": "Nested helper",
                "capabilities": {"cancel": {}},
                "state": {"state": "requires_action"},
            }),
        )?;
        self.notify_update(
            out,
            "sub-1",
            json!({
                "sessionUpdate": "subagent_update",
                "sessionId": "sub-1",
                "state": {
                    "state": "idle",
                    "stopReason": "cancelled",
                    "usage": {"totalTokens": 120, "inputTokens": 100, "outputTokens": 20},
                },
            }),
        )?;
        self.respond(out, id, json!({"stopReason": "end_turn"}))
    }

    fn agent_message(&self, out: &mut impl Write, id: &Value, session: &str) -> io::Result<()> {
        self.notify_update(
            out,
            session,
            json!({
                "sessionUpdate": "session_message",
                "messageId": "msg-agent-1",
                "senderSessionId": session,
                "recipientSessionId": "sub-1",
                "content": [{"type": "text", "text": "peer message"}],
            }),
        )?;
        self.respond(out, id, json!({"stopReason": "end_turn"}))
    }

    fn malformed(&self, out: &mut impl Write, id: &Value, session: &str) -> io::Result<()> {
        writeln!(
            out,
            "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{"
        )?;
        out.flush()?;
        self.chunk(out, session, "after malformed")?;
        self.respond(out, id, json!({"stopReason": "end_turn"}))
    }

    fn slow(
        &mut self,
        reader: &mut impl BufRead,
        out: &mut impl Write,
        id: &Value,
        session: &str,
    ) -> io::Result<()> {
        self.chunk(out, session, "working")?;
        loop {
            let Some(message) = read_message(reader)? else {
                return Ok(());
            };
            self.journal_received(&message);
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if method == "session/cancel"
                && message.pointer("/params/sessionId").and_then(Value::as_str) == Some(session)
            {
                self.chunk(out, session, "cancelled")?;
                return self.respond(out, id, json!({"stopReason": "cancelled"}));
            }
            self.dispatch(reader, out, message)?;
        }
    }
}
