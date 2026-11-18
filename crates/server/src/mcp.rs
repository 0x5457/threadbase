//! The MCP endpoint, where an outside agent can drive threads.
//!
//! A token is created by a signed-in person on the settings page. It has
//! explicit scopes, is stored only as a hash, and can be revoked at any time.
//! Every call runs as that person, so roles, permission modes and worktree
//! checks apply exactly as they do in the browser.
//!
//! Calls that change something (prompt, approval) also leave a system notice in
//! the thread, so the transcript shows that an outside agent started the turn.
//! Read calls only go to the audit log; a notice per poll would flood the
//! conversation.

use crate::app::AppState;
use crate::auth::{self, CurrentUser};
use crate::error::ServerError;
use crate::threads;
use serde_json::{json, Value};
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::api::McpTokenDto;
use threadbase_shared::McpScope;
use threadbase_shared::{now_ms, Actor, ApiError, ApprovalDecision, ThreadEvent, ThreadId};
use ulid::Ulid;

/// Header the MCP client sends. This endpoint never reads cookies, so a browser
/// page cannot call it with the user's session.
const BEARER: &str = "authorization";
/// Longest `thread_wait` may block.
const MAX_WAIT_SECONDS: u64 = 300;

pub fn enabled(config: &crate::config::Config) -> bool {
    config.mcp
}

#[derive(Debug, Clone)]
pub struct Principal {
    pub user: CurrentUser,
    pub scopes: Vec<McpScope>,
    pub label: String,
}

impl Principal {
    fn allows(&self, scope: McpScope) -> bool {
        self.scopes.contains(&scope)
    }
}

#[derive(Debug, FromRow)]
struct TokenRow {
    id: String,
    user_id: String,
    label: String,
    scopes: String,
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

pub async fn mint(
    pool: &SqlitePool,
    principal: &CurrentUser,
    label: &str,
    scopes: &[McpScope],
) -> Result<String, ServerError> {
    let label = label.trim();
    if label.is_empty() || label.len() > 60 {
        return Err(ServerError(ApiError::validation(
            "give the token a name between 1 and 60 characters",
        )));
    }
    if !scopes.contains(&McpScope::Read) {
        return Err(ServerError(ApiError::validation(
            "a token needs the read scope to be useful",
        )));
    }
    // Approving lets an outside agent answer permission requests, so only an
    // administrator may hand it out.
    if scopes.contains(&McpScope::Approve) && !principal.is_admin() {
        return Err(ServerError(ApiError::forbidden(
            "only an administrator can mint a token that answers approvals",
        )));
    }
    // A read-only account cannot start turns, so a write scope would be useless.
    if scopes.contains(&McpScope::Write) && !principal.user.access.can_write() {
        return Err(ServerError(ApiError::forbidden(
            "this account cannot start turns, so it cannot mint a write token",
        )));
    }
    let token = crate::auth_session::new_token();
    sqlx::query(
        "INSERT INTO mcp_tokens(id, token_hash, user_id, label, scopes, created_at)
         VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(Ulid::new().to_string())
    .bind(crate::auth_session::hash(&token))
    .bind(principal.user.id.as_str())
    .bind(label)
    .bind(scopes_json(scopes))
    .bind(now_ms())
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn list(pool: &SqlitePool, user_id: &str) -> Result<Vec<McpTokenDto>, ServerError> {
    let rows: Vec<(String, String, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, label, scopes, created_at, last_used_at FROM mcp_tokens
          WHERE user_id = ? AND revoked_at IS NULL ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, label, scopes, created_at, last_used_at)| McpTokenDto {
                id,
                label,
                scopes: scopes_from_json(&scopes),
                created_at,
                last_used_at,
            },
        )
        .collect())
}

pub async fn revoke(pool: &SqlitePool, user_id: &str, id: &str) -> Result<(), ServerError> {
    let changed = sqlx::query(
        "UPDATE mcp_tokens SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(now_ms())
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    if changed.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found("token not found")));
    }
    Ok(())
}

/// Returns the person a bearer token acts for.
pub async fn authenticate(
    pool: &SqlitePool,
    headers: &axum::http::HeaderMap,
) -> Result<Principal, ServerError> {
    let presented = headers
        .get(BEARER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ServerError(ApiError::new(
                threadbase_shared::ErrorCode::Unauthorized,
                "send the token as `Authorization: Bearer <token>`",
            ))
        })?;
    let row: Option<TokenRow> = sqlx::query_as(
        "SELECT id, user_id, label, scopes FROM mcp_tokens
          WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(crate::auth_session::hash(presented))
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(ServerError(ApiError::new(
            threadbase_shared::ErrorCode::Unauthorized,
            "this token is not valid",
        )));
    };
    let user = auth::load_user_by_id(pool, &row.user_id)
        .await?
        .ok_or_else(|| {
            ServerError(ApiError::new(
                threadbase_shared::ErrorCode::Unauthorized,
                "the person behind this token is gone",
            ))
        })?;
    // A pending or disabled account keeps its restrictions: the token is another
    // client for the same person, not more access.
    if !matches!(
        user.access,
        threadbase_shared::UserAccess::Member | threadbase_shared::UserAccess::ReadOnly
    ) {
        return Err(ServerError(ApiError::forbidden(
            "this account cannot drive threads",
        )));
    }
    let _ = sqlx::query("UPDATE mcp_tokens SET last_used_at = ? WHERE id = ?")
        .bind(now_ms())
        .bind(&row.id)
        .execute(pool)
        .await;
    Ok(Principal {
        scopes: scopes_from_json(&row.scopes),
        label: row.label,
        user: CurrentUser {
            user,
            method: "mcp",
        },
    })
}

fn scopes_json(scopes: &[McpScope]) -> String {
    let names: Vec<&str> = scopes
        .iter()
        .map(|scope| match scope {
            McpScope::Read => "read",
            McpScope::Write => "write",
            McpScope::Approve => "approve",
        })
        .collect();
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".into())
}

fn scopes_from_json(raw: &str) -> Vec<McpScope> {
    let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    names
        .into_iter()
        .filter_map(|name| name.parse().ok())
        .collect()
}

// ---------------------------------------------------------------------------
// JSON-RPC
// ---------------------------------------------------------------------------

const PROTOCOL_VERSION: &str = "2025-06-18";

/// Handle one JSON-RPC message. Returns `None` for notifications.
pub async fn handle(state: &AppState, principal: &Principal, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = id?;
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "threadbase", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(state, principal, &params).await,
        other => Err(rpc_error(-32601, &format!("unknown method {other}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn rpc_error(code: i64, message: &str) -> Value {
    json!({ "code": code, "message": message })
}

fn tools() -> Value {
    json!([
        {
            "name": "threads_list",
            "description": "List recent threads on this machine, newest activity first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project id or name to filter by." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum threads to return (default 20)." }
                }
            }
        },
        {
            "name": "thread_read",
            "description": "Read one thread's metadata and transcript events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string" },
                    "after": { "type": "integer", "description": "Return events after this sequence (default 0, which returns the recent tail)." }
                },
                "required": ["thread_id"]
            }
        },
        {
            "name": "thread_prompt",
            "description": "Send a message to a thread as the token's owner. Starts a turn.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string" },
                    "text": { "type": "string" }
                },
                "required": ["thread_id", "text"]
            }
        },
        {
            "name": "thread_wait",
            "description": "Wait until a thread is idle or waiting on a person.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string" },
                    "until": { "type": "string", "enum": ["idle", "needs_you"], "description": "Default idle." },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "description": "Default 60, at most 300." }
                },
                "required": ["thread_id"]
            }
        },
        {
            "name": "approvals_list",
            "description": "List pending approvals on a thread.",
            "inputSchema": {
                "type": "object",
                "properties": { "thread_id": { "type": "string" } },
                "required": ["thread_id"]
            }
        },
        {
            "name": "approval_decide",
            "description": "Answer a pending approval. Needs the approve scope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "approval_id": { "type": "string" },
                    "decision": { "type": "string", "enum": ["allow_once", "allow_session", "allow_thread", "deny"] }
                },
                "required": ["approval_id", "decision"]
            }
        }
    ])
}

async fn call_tool(
    state: &AppState,
    principal: &Principal,
    params: &Value,
) -> Result<Value, Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let text = match name {
        "threads_list" => {
            require_scope(principal, McpScope::Read)?;
            threads_list(state, &arguments).await
        }
        "thread_read" => {
            require_scope(principal, McpScope::Read)?;
            thread_read(state, principal, &arguments).await
        }
        "thread_prompt" => {
            require_scope(principal, McpScope::Write)?;
            thread_prompt(state, principal, &arguments).await
        }
        "thread_wait" => {
            require_scope(principal, McpScope::Read)?;
            thread_wait(state, principal, &arguments).await
        }
        "approvals_list" => {
            require_scope(principal, McpScope::Read)?;
            approvals_list(state, principal, &arguments).await
        }
        "approval_decide" => {
            require_scope(principal, McpScope::Approve)?;
            approval_decide(state, principal, &arguments).await
        }
        other => {
            return Err(rpc_error(-32602, &format!("unknown tool {other}")));
        }
    };
    match text {
        Ok(value) => Ok(json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "isError": false,
        })),
        Err(error) => {
            let message = error.to_string();
            Ok(json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true,
            }))
        }
    }
}

fn require_scope(principal: &Principal, scope: McpScope) -> Result<(), Value> {
    if principal.allows(scope) {
        return Ok(());
    }
    Err(rpc_error(
        -32003,
        &format!(
            "this token has no {} scope",
            match scope {
                McpScope::Read => "read",
                McpScope::Write => "write",
                McpScope::Approve => "approve",
            }
        ),
    ))
}

fn arg_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, Value> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| rpc_error(-32602, &format!("missing required argument: {key}")))
}

fn arg_u64(arguments: &Value, key: &str) -> Option<u64> {
    arguments.get(key).and_then(Value::as_u64)
}

async fn threads_list(state: &AppState, arguments: &Value) -> Result<Value, ServerError> {
    let limit = arg_u64(arguments, "limit").unwrap_or(20).clamp(1, 100) as usize;
    let filter = arguments
        .get("project")
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    let projects = crate::projects::list(&state.pool).await?;
    let all = threads::list_all(&state.pool, false, &state.policy).await?;
    let out: Vec<Value> = all
        .into_iter()
        .filter(|thread| {
            let Some(filter) = filter.as_deref() else {
                return true;
            };
            let project = projects
                .iter()
                .find(|project| project.id.0 == thread.project_id.0);
            thread.project_id.0.to_lowercase() == filter
                || project.is_some_and(|project| project.name.to_lowercase().contains(filter))
        })
        .take(limit)
        .map(|thread| {
            json!({
                "thread_id": thread.id.0,
                "title": thread.title,
                "project_id": thread.project_id.0,
                "project": projects
                    .iter()
                    .find(|project| project.id == thread.project_id)
                    .map(|project| project.name.clone()),
                "status": serde_json::to_value(thread.status).unwrap_or(Value::Null),
                "section": serde_json::to_value(thread.section).unwrap_or(Value::Null),
                "agent": thread.agent,
                "model": thread.model,
                "updated_at": thread.updated_at,
                "pending_approvals": thread.pending_approvals,
            })
        })
        .collect();
    Ok(json!({ "threads": out }))
}

async fn thread_read(
    state: &AppState,
    principal: &Principal,
    arguments: &Value,
) -> Result<Value, ServerError> {
    let thread_id = arg_str(arguments, "thread_id")
        .map_err(|_| ServerError(ApiError::validation("thread_id is required")))?;
    let after = arg_u64(arguments, "after").unwrap_or(0);
    // `threads::get` enforces read access for the token's person, including
    // whether they are still a participant of this thread.
    let thread = threads::get(&state.pool, thread_id, &principal.user, &state.policy).await?;
    let page = state
        .events
        .page(&ThreadId::from(thread_id.to_owned()), after, 200)
        .await
        .map_err(|error| ServerError::internal(error.to_string()))?;
    Ok(json!({
        "thread": {
            "thread_id": thread.id.0,
            "title": thread.title,
            "status": serde_json::to_value(thread.status).unwrap_or(Value::Null),
            "agent": thread.agent,
            "model": thread.model,
            "branch": thread.branch,
        },
        "events": page.events,
        "last_seq": page.last_seq,
    }))
}

async fn thread_prompt(
    state: &AppState,
    principal: &Principal,
    arguments: &Value,
) -> Result<Value, ServerError> {
    let thread_id = arg_str(arguments, "thread_id")
        .map_err(|_| ServerError(ApiError::validation("thread_id is required")))?;
    let text = arg_str(arguments, "text")
        .map_err(|_| ServerError(ApiError::validation("text is required")))?;
    if text.trim().is_empty() {
        return Err(ServerError(ApiError::validation("text is required")));
    }
    // Same check the HTTP route does. A tool call is another way into the same
    // operation, so it must not rely on the token alone.
    crate::authz::require_write_role(&state.pool, thread_id, &principal.user).await?;
    note(
        state,
        thread_id,
        principal,
        format!(
            "{} sent this turn through MCP (token \"{}\").",
            display_name(&principal.user),
            principal.label
        ),
    )
    .await;
    let response = state
        .agents
        .send(thread_id, &principal.user, text, None)
        .await?;
    Ok(json!({
        "delivery": serde_json::to_value(response.delivery).unwrap_or(Value::Null),
        "message_id": response.message_id,
    }))
}

async fn thread_wait(
    state: &AppState,
    principal: &Principal,
    arguments: &Value,
) -> Result<Value, ServerError> {
    let thread_id = arg_str(arguments, "thread_id")
        .map_err(|_| ServerError(ApiError::validation("thread_id is required")))?;
    let until = arguments
        .get("until")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let timeout = arg_u64(arguments, "timeout_seconds")
        .unwrap_or(60)
        .clamp(1, MAX_WAIT_SECONDS);
    threads::get(&state.pool, thread_id, &principal.user, &state.policy).await?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    loop {
        let thread_state = state.projection.read(thread_id).await?;
        let busy = thread_state.pending_approvals > 0
            || thread_state.pending_elicitations > 0
            || thread_state.queued_inputs > 0
            || thread_state.active_subagents > 0
            || matches!(
                thread_state.status,
                threadbase_shared::ThreadStatus::Running
                    | threadbase_shared::ThreadStatus::Preparing
                    | threadbase_shared::ThreadStatus::RequiresAction
            );
        let reached = match until {
            "needs_you" => {
                thread_state.pending_approvals > 0
                    || thread_state.pending_elicitations > 0
                    || thread_state.status == threadbase_shared::ThreadStatus::RequiresAction
            }
            _ => !busy,
        };
        if reached || std::time::Instant::now() >= deadline {
            return Ok(json!({
                "reached": reached,
                "state": serde_json::to_value(&thread_state).unwrap_or(Value::Null),
            }));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn approvals_list(
    state: &AppState,
    principal: &Principal,
    arguments: &Value,
) -> Result<Value, ServerError> {
    let thread_id = arg_str(arguments, "thread_id")
        .map_err(|_| ServerError(ApiError::validation("thread_id is required")))?;
    crate::authz::require_read(&state.pool, thread_id, &principal.user).await?;
    let approvals = crate::approvals::list_pending_for_thread(&state.pool, thread_id).await?;
    Ok(serde_json::to_value(approvals).unwrap_or(Value::Null))
}

async fn approval_decide(
    state: &AppState,
    principal: &Principal,
    arguments: &Value,
) -> Result<Value, ServerError> {
    let approval_id = arg_str(arguments, "approval_id")
        .map_err(|_| ServerError(ApiError::validation("approval_id is required")))?;
    let decision: ApprovalDecision = arg_str(arguments, "decision")
        .map_err(|_| ServerError(ApiError::validation("decision is required")))?
        .parse()
        .map_err(|_| {
            ServerError(ApiError::validation(
                "decision must be allow_once, allow_session, allow_thread or deny",
            ))
        })?;
    let approval = crate::approvals::decide(
        &state.pool,
        &state.projection,
        &state.events,
        approval_id,
        &principal.user,
        decision,
        Some("decided through MCP"),
    )
    .await?;
    // The person pressing the button in the browser gets the same two steps, in
    // the same order: persist the decision, then tell the adapter.
    if let Err(error) = state
        .agents
        .decide(approval.thread_id.as_str(), approval_id, decision, None)
        .await
    {
        tracing::warn!(%error, approval_id, "could not deliver decision to agent");
    }
    auth::audit(
        &state.pool,
        Some(principal.user.user.id.as_str()),
        "mcp.approval_decided",
        Some("approval"),
        Some(approval_id),
        Some(&principal.label),
        None,
    )
    .await;
    Ok(serde_json::to_value(approval).unwrap_or(Value::Null))
}

/// Adds a visible line to the thread an outside agent touched.
async fn note(state: &AppState, thread_id: &str, principal: &Principal, message: String) {
    let _ = state
        .events
        .append(
            &ThreadId::from(thread_id.to_owned()),
            Actor::system(),
            ThreadEvent::SystemNotice {
                level: "info".to_owned(),
                message,
            },
            crate::events::EventMeta::default(),
        )
        .await;
    let _ = state.projection.publish(thread_id).await;
    auth::audit(
        &state.pool,
        Some(principal.user.user.id.as_str()),
        "mcp.prompt",
        Some("thread"),
        Some(thread_id),
        Some(&principal.label),
        None,
    )
    .await;
}

fn display_name(user: &CurrentUser) -> String {
    if user.user.display_name.is_empty() {
        user.user.username.clone()
    } else {
        user.user.display_name.clone()
    }
}
