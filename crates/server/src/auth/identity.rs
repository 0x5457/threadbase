//! The `users` table: identity upsert, administrator bootstrap, lookup and
//! administrator updates.
//!
//! Everything here is database work with no HTTP types, so route handlers and
//! the extractor share one definition of a user row and one set of rules.

use super::{audit, matches_login, Identity};
use crate::error::ServerError;
use sqlx::{FromRow, SqlitePool};
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{now_ms, ApiError, UserAccess, UserDto};
use ulid::Ulid;

#[derive(Debug, FromRow)]
struct UserRow {
    id: String,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
    is_admin: i64,
    access: String,
    hidden_presence: i64,
    created_at: i64,
}

impl UserRow {
    fn into_dto(self) -> UserDto {
        UserDto {
            id: self.id.into(),
            username: self.username,
            display_name: self.display_name,
            avatar_url: self.avatar_url,
            is_admin: self.is_admin != 0,
            access: parse_user_access(&self.access),
            hidden_presence: self.hidden_presence != 0,
            created_at: self.created_at,
        }
    }
}

fn parse_user_access(value: &str) -> UserAccess {
    match value {
        "member" => UserAccess::Member,
        "read_only" => UserAccess::ReadOnly,
        "disabled" => UserAccess::Disabled,
        _ => UserAccess::Pending,
    }
}

const USER_COLUMNS: &str =
    "u.id, u.username, u.display_name, u.avatar_url, u.is_admin, u.access, u.hidden_presence, u.created_at";

/// True when no identity-backed administrator exists yet, so the instance has
/// no owner and the next sign-in bootstraps one.
pub async fn needs_admin(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE is_admin = 1 AND identity_key IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(admins == 0)
}

/// Creates or refreshes the local user for a verified identity.
pub async fn ensure_identity_user(
    pool: &SqlitePool,
    identity: &Identity,
    admins: &[String],
) -> Result<UserDto, ServerError> {
    let existing = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.id = (
             SELECT id FROM users WHERE identity_key = ?
         )"
    ))
    .bind(&identity.key)
    .fetch_optional(pool)
    .await?;

    let user = match existing {
        Some(row) => {
            let display = identity
                .display_name
                .as_deref()
                .filter(|value| !value.trim().is_empty());
            sqlx::query(
                "UPDATE users SET
                     display_name = COALESCE(?, display_name),
                     avatar_url = COALESCE(?, avatar_url)
                 WHERE id = ? AND (
                     (display_name <> COALESCE(?, display_name)) OR
                     (avatar_url IS NOT ?)
                 )",
            )
            .bind(display)
            .bind(identity.avatar_url.as_deref())
            .bind(&row.id)
            .bind(display)
            .bind(identity.avatar_url.as_deref())
            .execute(pool)
            .await?;
            row.into_dto()
        }
        None => {
            // The first identity on a fresh instance becomes the administrator
            // and is active immediately. Development identities are trusted
            // local loops. Every other new account waits for approval.
            let first_admin = needs_admin(pool).await?;
            let access = if first_admin || identity.source == "dev" {
                UserAccess::Member
            } else {
                UserAccess::Pending
            };
            create_identity_user(pool, identity, access).await?
        }
    };

    if user.is_admin || !should_be_admin(pool, &identity.login, admins).await? {
        return Ok(user);
    }

    sqlx::query("UPDATE users SET is_admin = 1, access = 'member' WHERE id = ?")
        .bind(user.id.as_str())
        .execute(pool)
        .await?;
    audit(
        pool,
        Some(user.id.as_str()),
        "user.admin_bootstrapped",
        Some("user"),
        Some(user.id.as_str()),
        Some(identity.source),
        None,
    )
    .await;
    load_user_by_id(pool, user.id.as_str())
        .await?
        .ok_or_else(|| ServerError::internal("promoted user could not be loaded"))
}

async fn should_be_admin(
    pool: &SqlitePool,
    login: &str,
    configured_admins: &[String],
) -> Result<bool, ServerError> {
    if matches_login(configured_admins, login) {
        return Ok(true);
    }
    if !configured_admins.is_empty() {
        return Ok(false);
    }
    // With no configured admins the first identity to sign in becomes the
    // administrator so the instance is never unmanageable.
    Ok(needs_admin(pool).await?)
}

async fn create_identity_user(
    pool: &SqlitePool,
    identity: &Identity,
    access: UserAccess,
) -> Result<UserDto, ServerError> {
    let now = now_ms();
    let base = sanitize_username(&identity.login);
    let display = identity
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| identity.login.clone());
    for attempt in 0..8 {
        let username = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{}", &Ulid::new().to_string()[20..])
        };
        let id = Ulid::new().to_string();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO users(id, username, display_name, avatar_url, is_admin, access, created_at, identity_source, identity_key)
             VALUES(?, ?, ?, ?, 0, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&username)
        .bind(&display)
        .bind(identity.avatar_url.as_deref())
        .bind(access.as_str())
        .bind(now)
        .bind(identity.source)
        .bind(&identity.key)
        .execute(pool)
        .await?;
        if result.rows_affected() == 1 {
            return load_user_by_id(pool, &id)
                .await?
                .ok_or_else(|| ServerError::internal("created user could not be loaded"));
        }
        let raced: Option<UserDto> = load_user_by_identity(pool, &identity.key).await?;
        if let Some(user) = raced {
            return Ok(user);
        }
    }
    Err(ServerError::internal("could not allocate a username"))
}

pub async fn load_user_by_identity(
    pool: &SqlitePool,
    identity_key: &str,
) -> Result<Option<UserDto>, sqlx::Error> {
    Ok(sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.id = (SELECT id FROM users WHERE identity_key = ?)"
    ))
    .bind(identity_key)
    .fetch_optional(pool)
    .await?
    .map(UserRow::into_dto))
}

pub async fn load_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<UserDto>, sqlx::Error> {
    Ok(
        sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLUMNS} FROM users u WHERE id = ?"))
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(UserRow::into_dto),
    )
}

pub async fn load_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<UserDto>, sqlx::Error> {
    Ok(sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.username = ? COLLATE NOCASE"
    ))
    .bind(username)
    .fetch_optional(pool)
    .await?
    .map(UserRow::into_dto))
}

pub async fn list_users(pool: &SqlitePool) -> Result<Vec<UserDto>, sqlx::Error> {
    Ok(sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u ORDER BY u.created_at ASC"
    ))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(UserRow::into_dto)
    .collect())
}

pub async fn update_user(
    pool: &SqlitePool,
    actor_id: &str,
    target_id: &str,
    request: &UpdateUserRequest,
) -> Result<UserDto, ServerError> {
    let target = load_user_by_id(pool, target_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))?;
    let loses_admin =
        target.is_admin && (!request.is_admin || request.access != UserAccess::Member);
    if loses_admin {
        let others: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE is_admin = 1 AND id <> ? AND access = 'member'",
        )
        .bind(target_id)
        .fetch_one(pool)
        .await?;
        if others == 0 {
            return Err(ServerError(ApiError::conflict(
                "at least one active administrator must remain",
            )));
        }
    }
    if actor_id == target_id && request.access != UserAccess::Member {
        return Err(ServerError(ApiError::conflict(
            "you cannot reduce your own access; ask another administrator",
        )));
    }
    sqlx::query("UPDATE users SET is_admin = ?, access = ? WHERE id = ?")
        .bind(i64::from(request.is_admin))
        .bind(request.access.as_str())
        .bind(target_id)
        .execute(pool)
        .await?;
    // Revoking access must end live sessions; otherwise a disabled account
    // keeps reading until the cookie happens to expire.
    if !request.access.can_read() {
        crate::auth_session::revoke_all_for_user(pool, target_id).await?;
    }
    load_user_by_id(pool, target_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))
}

/// A live session joined with its user row, so the extractor needs one query
/// instead of a session lookup followed by a user lookup.
#[derive(Debug)]
pub(crate) struct SessionUser {
    pub(crate) user: UserDto,
    pub(crate) method: String,
    pub(crate) expires_at: i64,
    pub(crate) last_seen_at: Option<i64>,
}

#[derive(Debug, FromRow)]
struct SessionUserRow {
    method: String,
    expires_at: i64,
    last_seen_at: Option<i64>,
    #[sqlx(flatten)]
    user: UserRow,
}

pub(crate) async fn load_session_user(
    pool: &SqlitePool,
    token_hash: &str,
    now: i64,
) -> Result<Option<SessionUser>, sqlx::Error> {
    let row = sqlx::query_as::<_, SessionUserRow>(&format!(
        "SELECT s.method, s.expires_at, s.last_seen_at, {USER_COLUMNS}
         FROM auth_sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.revoked_at IS NULL AND s.expires_at > ?"
    ))
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| SessionUser {
        user: row.user.into_dto(),
        method: row.method,
        expires_at: row.expires_at,
        last_seen_at: row.last_seen_at,
    }))
}

fn sanitize_username(login: &str) -> String {
    let base = login.split('@').next().unwrap_or(login);
    let mut output: String = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    output = output.trim_matches('-').to_owned();
    if output.len() < 3 {
        output = format!("user-{output}");
    }
    output.truncate(24);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_are_sanitized() {
        assert_eq!(
            sanitize_username("Ada.Lovelace@example.com"),
            "ada.lovelace"
        );
        assert_eq!(sanitize_username("a b c"), "a-b-c");
        assert!(sanitize_username("x").starts_with("user-"));
    }
}
