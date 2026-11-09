//! Approval, elicitation and inbox routes.

use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::Json;
use axum::Router;
use threadbase_shared::api::{DecisionRequest, ElicitationResponseRequest, InboxReadRequest};
use threadbase_shared::{
    ApprovalResolvedResponse, ApprovalRulesResponse, ApprovalsPageDto, ElicitationResolvedResponse,
    InboxPageDto, OkResponse,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{thread_id}/approvals", get(list_approvals))
        .route("/api/approvals/{approval_id}/decide", post(decide_approval))
        .route("/api/approval-rules", get(list_rules))
        .route("/api/approval-rules/{rule_id}", delete(delete_rule))
        .route(
            "/api/elicitations/{elicitation_id}/respond",
            post(respond_elicitation),
        )
        .route("/api/inbox", get(list_inbox))
        .route("/api/inbox/read", post(read_inbox))
}

async fn list_approvals(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<ApprovalsPageDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(ApprovalsPageDto {
        approvals: crate::approvals::list_pending_for_thread(&state.pool, &thread_id).await?,
        elicitations: crate::approvals::list_elicitations(&state.pool, &thread_id).await?,
    }))
}

async fn decide_approval(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(approval_id): Path<String>,
    Json(request): Json<DecisionRequest>,
) -> Result<Json<ApprovalResolvedResponse>, ServerError> {
    Ok(Json(ApprovalResolvedResponse {
        approval: crate::approvals::decide(
            &state.pool,
            &state.events,
            &approval_id,
            &user,
            request.decision,
            request.note.as_deref(),
        )
        .await?,
    }))
}

async fn list_rules(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<ApprovalRulesResponse>, ServerError> {
    user.require_admin()?;
    Ok(Json(ApprovalRulesResponse {
        rules: crate::approvals::list_rules(&state.pool).await?,
    }))
}

async fn delete_rule(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(rule_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    user.require_admin()?;
    crate::approvals::delete_rule(&state.pool, &rule_id).await?;
    crate::auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "approval.rule_deleted",
        Some("approval_rule"),
        Some(&rule_id),
        None,
        None,
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn respond_elicitation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(elicitation_id): Path<String>,
    Json(request): Json<ElicitationResponseRequest>,
) -> Result<Json<ElicitationResolvedResponse>, ServerError> {
    Ok(Json(ElicitationResolvedResponse {
        elicitation: crate::approvals::respond_elicitation(
            &state.pool,
            &state.events,
            &elicitation_id,
            &user,
            request.action,
            request.values,
        )
        .await?,
    }))
}

async fn list_inbox(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<InboxPageDto>, ServerError> {
    Ok(Json(
        crate::approvals::list_inbox(&state.pool, &user).await?,
    ))
}

async fn read_inbox(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<InboxReadRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    crate::approvals::mark_inbox_read(
        &state.pool,
        &user,
        request.thread_id.as_ref().map(|id| id.as_str()),
        request.all.unwrap_or(false),
    )
    .await?;
    Ok(Json(OkResponse { ok: true }))
}
