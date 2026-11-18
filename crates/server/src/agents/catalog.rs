//! Workspace-configured agent adapters on top of the built-in profiles.
//!
//! A custom agent is one launch command with literal arguments and optional
//! environment variables. The server spawns it without a shell, so no quoting
//! or expansion applies; that is also why the command must be a single token.

use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_agent::profile::{find_in_path, find_profile, resolve, AgentProfile};
use threadbase_shared::api::AgentUpsertRequest;
use threadbase_shared::{AgentDto, ApiError, EnvVarDto};

/// One persisted custom agent.
#[derive(Debug, Clone)]
struct Row {
    id: String,
    label: String,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

impl Row {
    fn profile(&self) -> AgentProfile {
        AgentProfile::custom(
            self.id.clone(),
            self.label.clone(),
            self.command.clone(),
            self.args.clone(),
            None,
        )
    }

    fn dto(&self) -> AgentDto {
        let resolved = resolve(&self.profile());
        AgentDto {
            id: self.id.clone(),
            label: self.label.clone(),
            command: self.command.clone(),
            args: self.args.clone(),
            env: self
                .env
                .iter()
                .map(|(key, value)| EnvVarDto {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
            custom: true,
            installed: status(&resolved).0,
            via_npx: status(&resolved).1,
            supports_steering: false,
            supports_subagents: false,
            supports_elicitation: false,
        }
    }
}

/// `(installed, via_npx)` for a resolved profile: a local binary wins, an npx
/// fallback counts only when npx exists, anything else is missing.
fn status(resolved: &threadbase_agent::profile::ResolvedProfile) -> (bool, bool) {
    if resolved.via_npx {
        (find_in_path("npx").is_some(), true)
    } else {
        (find_in_path(&resolved.program).is_some(), false)
    }
}

fn built_in_dto(profile: &AgentProfile) -> AgentDto {
    let resolved = resolve(profile);
    let (installed, via_npx) = status(&resolved);
    AgentDto {
        id: profile.id.clone(),
        label: profile.label.clone(),
        command: profile.command.clone(),
        args: profile.args.clone(),
        env: Vec::new(),
        custom: false,
        installed,
        via_npx,
        supports_steering: matches!(profile.id.as_str(), "codex" | "claude"),
        supports_subagents: matches!(profile.id.as_str(), "codex" | "claude" | "opencode"),
        supports_elicitation: matches!(profile.id.as_str(), "codex" | "claude"),
    }
}

async fn load_rows(pool: &SqlitePool) -> Result<Vec<Row>, ServerError> {
    let stored: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, label, command, args_json, env_json \
             FROM agent_profiles ORDER BY label COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(stored
        .into_iter()
        .map(|(id, label, command, args_json, env_json)| {
            // Rows are written only through validation below; a hand-edited
            // value should degrade to "no args" rather than fail the page.
            let args = serde_json::from_str(&args_json).unwrap_or_default();
            let env: Vec<(String, String)> = serde_json::from_str(&env_json).unwrap_or_default();
            Row {
                id,
                label,
                command,
                args,
                env,
            }
        })
        .collect())
}

/// The full catalogue every member picks agents from.
pub async fn list(pool: &SqlitePool) -> Result<Vec<AgentDto>, ServerError> {
    let mut agents: Vec<AgentDto> = threadbase_agent::all_profiles()
        .iter()
        .map(built_in_dto)
        .collect();
    for row in load_rows(pool).await? {
        agents.push(row.dto());
    }
    Ok(agents)
}

/// The profile one thread's agent id resolves to: a built-in, a configured
/// custom agent, or `None` for an id that is neither.
pub async fn resolve_profile(
    pool: &SqlitePool,
    agent_id: &str,
) -> Result<Option<AgentProfile>, ServerError> {
    if let Some(profile) = find_profile(agent_id) {
        return Ok(Some(profile.clone()));
    }
    let stored: Option<(String, String, String, String)> =
        sqlx::query_as("SELECT id, label, command, args_json FROM agent_profiles WHERE id = ?")
            .bind(agent_id)
            .fetch_optional(pool)
            .await?;
    Ok(stored.map(|(id, label, command, args_json)| {
        let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
        AgentProfile::custom(id, label, command, args, None)
    }))
}

/// Environment variables the adapter process is launched with. Custom agents
/// carry their own; the codex built-in is told where its own binary lives.
pub async fn launch_env(
    pool: &SqlitePool,
    agent_id: &str,
) -> Result<Vec<(String, String)>, ServerError> {
    let mut env = adapter_env(agent_id);
    if env.is_empty() {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT env_json FROM agent_profiles WHERE id = ?")
                .bind(agent_id)
                .fetch_optional(pool)
                .await?;
        if let Some(env_json) = stored {
            env = serde_json::from_str(&env_json).unwrap_or_default();
        }
    }
    Ok(env)
}

fn adapter_env(agent: &str) -> Vec<(String, String)> {
    if agent != "codex" {
        return Vec::new();
    }
    find_in_path("codex")
        .map(|path| vec![("CODEX_PATH".to_owned(), path.to_string_lossy().to_string())])
        .unwrap_or_default()
}

/// Slug an agent label into the id that threads persist: lowercase, letters and
/// digits kept, everything else folded into single hyphens.
fn slug(label: &str) -> String {
    let mut slug = String::new();
    let mut pending_hyphen = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    slug
}

fn validate(request: &AgentUpsertRequest) -> Result<Row, ApiError> {
    let label = request.label.trim();
    if label.is_empty() {
        return Err(ApiError::validation("name is required"));
    }
    if label.chars().count() > 40 {
        return Err(ApiError::validation("name must be 40 characters or fewer"));
    }
    let command = request.command.trim();
    if command.is_empty() {
        return Err(ApiError::validation("command is required"));
    }
    if command.chars().any(char::is_whitespace) {
        return Err(ApiError::validation(
            "command must be one executable name or path; put flags in the arguments list",
        ));
    }
    if command.chars().count() > 200 {
        return Err(ApiError::validation(
            "command must be 200 characters or fewer",
        ));
    }
    let mut args = Vec::new();
    for arg in &request.args {
        let arg = arg.trim();
        if arg.is_empty() {
            return Err(ApiError::validation("arguments cannot be blank"));
        }
        if arg.chars().count() > 200 {
            return Err(ApiError::validation(
                "arguments must be 200 characters or fewer",
            ));
        }
        args.push(arg.to_owned());
    }
    if args.len() > 50 {
        return Err(ApiError::validation(
            "an agent can take at most 50 arguments",
        ));
    }
    let mut env = Vec::new();
    for var in &request.env {
        let key = var.key.trim();
        if key.is_empty() {
            return Err(ApiError::validation(
                "environment variable names cannot be blank",
            ));
        }
        if !key.chars().enumerate().all(|(index, ch)| {
            ch.is_ascii_alphabetic() || ch == '_' || (index > 0 && ch.is_ascii_digit())
        }) {
            return Err(ApiError::validation(
                "environment variable names may only use letters, digits and underscores",
            ));
        }
        if var.value.chars().count() > 2000 {
            return Err(ApiError::validation(
                "environment variable values must be 2000 characters or fewer",
            ));
        }
        if env
            .iter()
            .any(|(existing, _): &(String, String)| existing == key)
        {
            return Err(ApiError::validation(format!("{key} is set more than once")));
        }
        env.push((key.to_owned(), var.value.clone()));
    }
    if env.len() > 50 {
        return Err(ApiError::validation(
            "an agent can take at most 50 environment variables",
        ));
    }
    Ok(Row {
        id: String::new(),
        label: label.to_owned(),
        command: command.to_owned(),
        args,
        env,
    })
}

/// Adds one custom agent. The id comes from the label, so a name that slugs to
/// a taken id — built-in or custom — is rejected instead of silently aliased.
pub async fn add(pool: &SqlitePool, request: AgentUpsertRequest) -> Result<AgentDto, ServerError> {
    let mut row = validate(&request).map_err(ServerError)?;
    row.id = slug(&row.label);
    if row.id.is_empty() {
        return Err(ServerError(ApiError::validation(
            "the name needs at least one letter or digit",
        )));
    }
    if find_profile(&row.id).is_some() {
        return Err(ServerError(ApiError::conflict(
            "an agent with this name is built in; pick another name",
        )));
    }
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM agent_profiles WHERE id = ?")
        .bind(&row.id)
        .fetch_optional(pool)
        .await?;
    if exists.is_some() {
        return Err(ServerError(ApiError::conflict(
            "an agent with this name already exists",
        )));
    }
    let args_json = serde_json::to_string(&row.args).expect("args serialize");
    let env_json = serde_json::to_string(&row.env).expect("env serialize");
    sqlx::query("INSERT INTO agent_profiles (id, label, command, args_json, env_json, created_at) VALUES (?,?,?,?,?,?)")
        .bind(&row.id)
        .bind(&row.label)
        .bind(&row.command)
        .bind(&args_json)
        .bind(&env_json)
        .bind(threadbase_shared::now_ms())
        .execute(pool)
        .await?;
    Ok(row.dto())
}

/// Replaces one custom agent. The id is fixed, so a renamed label stays
/// addressable by the threads already using it.
pub async fn update(
    pool: &SqlitePool,
    id: &str,
    request: AgentUpsertRequest,
) -> Result<AgentDto, ServerError> {
    let mut row = validate(&request).map_err(ServerError)?;
    let current: Option<String> = sqlx::query_scalar("SELECT id FROM agent_profiles WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if current.is_none() {
        return Err(ServerError(ApiError::not_found("no such agent")));
    }
    row.id = id.to_owned();
    let args_json = serde_json::to_string(&row.args).expect("args serialize");
    let env_json = serde_json::to_string(&row.env).expect("env serialize");
    sqlx::query(
        "UPDATE agent_profiles SET label = ?, command = ?, args_json = ?, env_json = ? WHERE id = ?",
    )
    .bind(&row.label)
    .bind(&row.command)
    .bind(&args_json)
    .bind(&env_json)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(row.dto())
}

/// Removes one custom agent. Threads that still name it keep working: they
/// fall back to the default profile at their next adapter start, exactly like
/// a thread whose agent id was never configured.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), ServerError> {
    if find_profile(id).is_some() {
        return Err(ServerError(ApiError::validation(
            "built-in agents cannot be removed",
        )));
    }
    let result = sqlx::query("DELETE FROM agent_profiles WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found("no such agent")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_folds_labels_into_ids() {
        assert_eq!(slug("DeepSeek Harness"), "deepseek-harness");
        assert_eq!(slug("  My -- Agent! "), "my-agent");
        assert_eq!(slug("。。。"), "");
    }

    #[test]
    fn validation_rejects_shell_pasting() {
        let request = AgentUpsertRequest {
            label: "dsh".to_owned(),
            command: "npx -y dsh".to_owned(),
            args: Vec::new(),
            env: Vec::new(),
        };
        let error = validate(&request).expect_err("must reject");
        assert!(error.message.contains("one executable"));
    }

    #[test]
    fn validation_rejects_bad_env_keys() {
        let request = AgentUpsertRequest {
            label: "dsh".to_owned(),
            command: "dsh".to_owned(),
            args: vec!["--profile".to_owned(), "acp".to_owned()],
            env: vec![EnvVarDto {
                key: "1BAD-KEY".to_owned(),
                value: "x".to_owned(),
            }],
        };
        assert!(validate(&request).is_err());
    }
}
