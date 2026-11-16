//! Per-user settings persisted as JSON.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_shared::api::SettingsPatch;
use threadbase_shared::{SettingsDto, StateResponse};

pub async fn load(pool: &SqlitePool, user: &CurrentUser) -> Result<SettingsDto, ServerError> {
    let stored: String = sqlx::query_scalar("SELECT settings_json FROM users WHERE id = ?")
        .bind(user.user.id.as_str())
        .fetch_one(pool)
        .await?;
    let mut settings: SettingsDto = serde_json::from_str(&stored).unwrap_or_default();
    settings.hidden_presence = user.user.hidden_presence;
    Ok(settings)
}

pub async fn update(
    pool: &SqlitePool,
    user: &CurrentUser,
    patch: SettingsPatch,
    terminal: bool,
    terminal_host: bool,
) -> Result<StateResponse, ServerError> {
    let mut settings = load(pool, user).await?;
    if let Some(theme) = patch.theme {
        if !matches!(theme.as_str(), "dark" | "light") {
            return Err(ServerError(threadbase_shared::ApiError::validation(
                "theme must be dark or light",
            )));
        }
        settings.theme = theme;
    }
    if let Some(width) = patch.text_width {
        if !matches!(width.as_str(), "default" | "full" | "custom") {
            return Err(ServerError(threadbase_shared::ApiError::validation(
                "invalid text width",
            )));
        }
        settings.text_width = width;
    }
    if let Some(value) = patch.subthreads_per_thread {
        settings.subthreads_per_thread = value.clamp(1, 100);
    }
    if let Some(value) = patch.send_with_modifier {
        settings.send_with_modifier = value;
    }
    if let Some(value) = patch.default_agent {
        settings.default_agent = value;
    }
    if patch.default_model.is_some() {
        settings.default_model = patch.default_model;
    }
    if let Some(value) = patch.default_baseline {
        settings.default_baseline = value;
    }
    if let Some(value) = patch.hidden_presence {
        settings.hidden_presence = value;
    }
    if let Some(value) = patch.webhook_url {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let valid = url::Url::parse(trimmed)
                .ok()
                .is_some_and(|url| url.scheme() == "https");
            if !valid {
                return Err(ServerError(threadbase_shared::ApiError::validation(
                    "webhook URL must be a valid https URL",
                )));
            }
        }
        settings.webhook_url = Some(value);
    }
    if let Some(value) = patch.browser_notifications {
        settings.browser_notifications = value;
    }
    sqlx::query("UPDATE users SET settings_json = ?, hidden_presence = ? WHERE id = ?")
        .bind(serde_json::to_string(&settings).unwrap_or_else(|_| "{}".into()))
        .bind(i64::from(settings.hidden_presence))
        .bind(user.user.id.as_str())
        .execute(pool)
        .await?;
    let refreshed = crate::auth::load_user_by_id(pool, user.user.id.as_str())
        .await?
        .ok_or_else(|| ServerError::internal("user disappeared"))?;
    Ok(StateResponse {
        settings,
        user: refreshed,
        terminal,
        terminal_host,
    })
}
