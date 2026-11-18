//! Workspace state, settings, agents and search routes.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use axum::extract::State;
use axum::routing::{get, patch};
use axum::Json;
use axum::Router;
use threadbase_shared::api::{AgentUpsertRequest, SettingsPatch};
use threadbase_shared::{AgentDto, MachineSpec, PermissionPolicyDto, StateResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/state", get(state))
        .route("/api/machines", get(machines))
        .route("/api/settings", patch(update_settings))
        .route("/api/agents", get(list_agents).post(add_agent))
        .route(
            "/api/agents/{agent_id}",
            patch(update_agent).delete(delete_agent),
        )
}

/// The lifecycle catalogue. Unauthenticated on purpose: it describes the
/// product, not any user's data, and agent tooling should be able to read it
/// before a session exists.
fn policy_of(app: &AppState) -> PermissionPolicyDto {
    PermissionPolicyDto {
        default: app.policy.default,
        cap: app.policy.cap,
    }
}

async fn machines() -> Json<Vec<MachineSpec>> {
    Json(threadbase_core::state::catalogue())
}

async fn state(
    State(app): State<AppState>,
    user: CurrentUser,
) -> Result<Json<StateResponse>, ServerError> {
    let instance = crate::settings::Instance::of(&app.config, policy_of(&app));
    Ok(Json(StateResponse {
        settings: crate::settings::load(&app.pool, &user).await?,
        user: user.user,
        permissions: instance.policy,
        terminal: instance.terminal,
        terminal_host: instance.terminal_host,
    }))
}

async fn update_settings(
    State(app): State<AppState>,
    user: CurrentUser,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<StateResponse>, ServerError> {
    Ok(Json(
        crate::settings::update(
            &app.pool,
            &user,
            patch,
            crate::settings::Instance::of(&app.config, policy_of(&app)),
        )
        .await?,
    ))
}

async fn list_agents(
    State(app): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<AgentDto>>, ServerError> {
    Ok(Json(crate::agents::catalog::list(&app.pool).await?))
}

async fn add_agent(
    State(app): State<AppState>,
    user: CurrentUser,
    Json(request): Json<AgentUpsertRequest>,
) -> Result<Json<AgentDto>, ServerError> {
    user.require_admin()?;
    Ok(Json(crate::agents::catalog::add(&app.pool, request).await?))
}

async fn update_agent(
    State(app): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
    Json(request): Json<AgentUpsertRequest>,
) -> Result<Json<AgentDto>, ServerError> {
    user.require_admin()?;
    Ok(Json(
        crate::agents::catalog::update(&app.pool, &agent_id, request).await?,
    ))
}

async fn delete_agent(
    State(app): State<AppState>,
    user: CurrentUser,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Result<Json<threadbase_shared::OkResponse>, ServerError> {
    user.require_admin()?;
    crate::agents::catalog::delete(&app.pool, &agent_id).await?;
    Ok(Json(threadbase_shared::OkResponse { ok: true }))
}
