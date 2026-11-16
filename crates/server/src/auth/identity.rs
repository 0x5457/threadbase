//! The `users` table: identity upsert, administrator bootstrap, lookup and
//! administrator updates.
//!
//! Everything here is database work with no HTTP types, so route handlers and
//! the extractor share one definition of a user row and one set of rules.

use super::{audit, matches_login, Identity};
use crate::error::ServerError;
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use std::collections::{HashMap, HashSet};
use threadbase_shared::api::UpdateUserRequest;
use threadbase_shared::{now_ms, ApiError, UserAccess, UserBrief, UserDto};
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
            access: UserAccess::parse(&self.access),
            hidden_presence: self.hidden_presence != 0,
            created_at: self.created_at,
        }
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
///
/// GitHub identities are keyed by the immutable numeric id. Rows created before
/// numeric ids were stored are keyed by the mutable login, so a matching legacy
/// row is upgraded in place the first time that login signs in again.
pub async fn ensure_identity_user(
    pool: &SqlitePool,
    identity: &Identity,
    admins: &[String],
) -> Result<UserDto, ServerError> {
    let existing = load_user_by_identity(pool, &identity.key).await?;

    let user = match existing {
        Some(user) => {
            refresh_profile(pool, user.id.as_str(), identity).await?;
            user
        }
        None => match migrate_legacy_identity(pool, identity, admins).await? {
            Some(user) => user,
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
        },
    };

    if user.is_admin || !should_be_admin(pool, identity, admins).await? {
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

/// Finds a row keyed by the old `github:<login>` key and rewrites it to
/// `github:<id>` in one transaction.
///
/// The numeric id cannot be recovered from a legacy row, so the first sign-in
/// that presents the same login adopts it. The conditional update is the first
/// statement in the transaction, so concurrent sign-ins cannot both read the
/// legacy row and then race to rewrite it. If another sign-in wins anyway, the
/// unique index rejects the update and the row is reloaded by the new key.
async fn migrate_legacy_identity(
    pool: &SqlitePool,
    identity: &Identity,
    admins: &[String],
) -> Result<Option<UserDto>, ServerError> {
    if identity.source != "github" {
        return Ok(None);
    }
    let legacy_key = super::github_login_key(&identity.login);
    // Adopting an administrator row by mutable login is the one case where a
    // re-registered login could inherit elevated rights. On a locked-down
    // instance (an explicit admin list) only a listed login may adopt it; a
    // fresh instance has no other administrator, so the bootstrap admin must
    // still be preserved. Non-admin legacy rows keep sliding with their owner.
    let legacy_is_admin: Option<i64> =
        sqlx::query_scalar("SELECT is_admin FROM users WHERE identity_key = ?")
            .bind(&legacy_key)
            .fetch_optional(pool)
            .await?;
    if legacy_is_admin == Some(1)
        && !admins.is_empty()
        && !admins
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(identity.login.trim()))
    {
        tracing::warn!(
            login = %identity.login,
            "legacy admin identity is not on the configured admin list; not migrating it"
        );
        return Ok(None);
    }
    let display = identity
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE users SET
             identity_key = ?,
             display_name = COALESCE(?, display_name),
             avatar_url = COALESCE(?, avatar_url)
         WHERE identity_key = ?",
    )
    .bind(&identity.key)
    .bind(display)
    .bind(identity.avatar_url.as_deref())
    .bind(&legacy_key)
    .execute(&mut *tx)
    .await;
    let row = match updated {
        Ok(result) if result.rows_affected() == 0 => {
            tx.rollback().await?;
            return Ok(None);
        }
        Ok(_) => match load_user_row_by_identity(&mut tx, &identity.key).await? {
            Some(row) => row,
            None => {
                tx.rollback().await?;
                return Err(ServerError::internal("migrated user could not be loaded"));
            }
        },
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            // Another sign-in migrated or created the new key first.
            tx.rollback().await?;
            return load_user_by_identity(pool, &identity.key)
                .await?
                .map(Some)
                .ok_or_else(|| {
                    ServerError::internal("github identity migration race could not be resolved")
                });
        }
        Err(error) => {
            tx.rollback().await?;
            return Err(error.into());
        }
    };
    tx.commit().await?;

    audit(
        pool,
        Some(row.id.as_str()),
        "user.identity_migrated",
        Some("user"),
        Some(row.id.as_str()),
        Some(identity.login.as_str()),
        None,
    )
    .await;
    load_user_by_id(pool, &row.id)
        .await?
        .map(Some)
        .ok_or_else(|| ServerError::internal("migrated user could not be loaded"))
}

async fn refresh_profile(
    pool: &SqlitePool,
    user_id: &str,
    identity: &Identity,
) -> Result<(), ServerError> {
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
    .bind(user_id)
    .bind(display)
    .bind(identity.avatar_url.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}

async fn should_be_admin(
    pool: &SqlitePool,
    identity: &Identity,
    configured_admins: &[String],
) -> Result<bool, ServerError> {
    let configured = identity
        .key
        .strip_prefix("github:")
        .and_then(|id| id.parse::<i64>().ok())
        .map_or_else(
            || matches_login(configured_admins, &identity.login),
            |id| super::matches_identity(configured_admins, id, &identity.login),
        );
    if configured {
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

/// Creates a pending account for a GitHub login that has never signed in, or
/// returns the existing user untouched.
///
/// Adding someone as a thread member must not grant workspace access before
/// they sign in, and signing in later must not silently approve them: an
/// existing user's access is never modified here.
pub async fn ensure_pending_identity(
    pool: &SqlitePool,
    login: &str,
    display_name: Option<&str>,
) -> Result<UserDto, ServerError> {
    let identity = super::github_invited(login, display_name);
    if let Some(user) = load_user_by_identity(pool, &identity.key).await? {
        return Ok(user);
    }
    create_identity_user(pool, &identity, UserAccess::Pending).await
}

pub async fn load_user_by_identity(
    pool: &SqlitePool,
    identity_key: &str,
) -> Result<Option<UserDto>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    Ok(load_user_row_by_identity(&mut conn, identity_key)
        .await?
        .map(UserRow::into_dto))
}

async fn load_user_row_by_identity(
    conn: &mut sqlx::SqliteConnection,
    identity_key: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLUMNS} FROM users u WHERE u.id = (SELECT id FROM users WHERE identity_key = ?)"
    ))
    .bind(identity_key)
    .fetch_optional(conn)
    .await
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

/// Small public profile rows for presence surfaces (terminal viewers, session
/// avatars). Hidden-presence accounts are omitted: a visible count may still
/// include them, but their identity is not exposed.
pub async fn load_user_briefs(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<UserBrief>, sqlx::Error> {
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        if seen.insert(id.as_str()) {
            unique.push(id);
        }
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::new(
        "SELECT id, username, display_name, avatar_url FROM users
          WHERE hidden_presence = 0 AND id IN (",
    );
    let mut separated = query.separated(", ");
    for id in &unique {
        separated.push_bind(id.as_str());
    }
    separated.push_unseparated(")");
    #[derive(FromRow)]
    struct BriefRow {
        id: String,
        username: String,
        display_name: String,
        avatar_url: Option<String>,
    }
    let rows: Vec<BriefRow> = query.build_query_as().fetch_all(pool).await?;
    let users: HashMap<String, UserBrief> = rows
        .into_iter()
        .map(|row| {
            let user = UserBrief {
                id: row.id.clone().into(),
                username: row.username,
                display_name: row.display_name,
                avatar_url: row.avatar_url,
                color: threadbase_shared::user_color(&row.id).to_owned(),
            };
            (row.id, user)
        })
        .collect();
    Ok(unique
        .into_iter()
        .filter_map(|id| users.get(id).cloned())
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
         WHERE s.token_hash = ? AND {}",
        threadbase_core::state::AuthSessionMachine::live_predicate("s")
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
    use crate::auth::Identity;
    use crate::db;

    #[test]
    fn usernames_are_sanitized() {
        assert_eq!(
            sanitize_username("Ada.Lovelace@example.com"),
            "ada.lovelace"
        );
        assert_eq!(sanitize_username("a b c"), "a-b-c");
        assert!(sanitize_username("x").starts_with("user-"));
    }

    #[tokio::test]
    async fn legacy_login_identity_migrates_to_the_numeric_id() {
        let pool = db::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, avatar_url, is_admin, created_at, identity_source, identity_key)
             VALUES('u-legacy', 'octocat', 'Octocat', NULL, 1, 0, 'github', 'github:octocat')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let user = ensure_identity_user(
            &pool,
            &Identity::github(12345, "Octocat", Some("The Octocat"), None),
            &[],
        )
        .await
        .unwrap();

        assert_eq!(user.id.as_str(), "u-legacy");
        assert!(user.is_admin);
        assert_eq!(user.display_name, "The Octocat");
        let key: Option<String> =
            sqlx::query_scalar("SELECT identity_key FROM users WHERE id = 'u-legacy'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(key.as_deref(), Some("github:12345"));
        let detail: Option<String> = sqlx::query_scalar(
            "SELECT detail_json FROM audit_log WHERE action = 'user.identity_migrated'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(detail.as_deref(), Some("Octocat"));
    }

    #[tokio::test]
    async fn legacy_admin_is_not_adopted_by_an_unlisted_login() {
        let pool = db::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, avatar_url, is_admin, created_at, identity_source, identity_key)
             VALUES('u-legacy', 'octocat', 'Octocat', NULL, 1, 0, 'github', 'github:octocat')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // The instance is locked down to another administrator: a login that
        // merely matches the legacy row's old handle must not inherit it.
        let user = ensure_identity_user(
            &pool,
            &Identity::github(12345, "Octocat", None, None),
            &["someone-else".to_owned()],
        )
        .await
        .unwrap();

        assert_ne!(user.id.as_str(), "u-legacy");
        assert!(!user.is_admin);
        assert_eq!(user.access, threadbase_shared::UserAccess::Pending);
        let key: Option<String> =
            sqlx::query_scalar("SELECT identity_key FROM users WHERE id = 'u-legacy'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(key.as_deref(), Some("github:octocat"));
    }

    #[tokio::test]
    async fn legacy_admin_is_adopted_when_the_login_is_listed() {
        let pool = db::connect_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, avatar_url, is_admin, created_at, identity_source, identity_key)
             VALUES('u-legacy', 'octocat', 'Octocat', NULL, 1, 0, 'github', 'github:octocat')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let user = ensure_identity_user(
            &pool,
            &Identity::github(12345, "Octocat", None, None),
            &["OCTOCAT".to_owned()],
        )
        .await
        .unwrap();

        assert_eq!(user.id.as_str(), "u-legacy");
        assert!(user.is_admin);
    }

    #[tokio::test]
    async fn a_reclaimed_login_does_not_take_over_the_old_account() {
        let pool = db::connect_memory().await.unwrap();
        let original = ensure_identity_user(
            &pool,
            &Identity::github(12345, "octocat", Some("Octo"), None),
            &[],
        )
        .await
        .unwrap();

        // Someone else now registers the freed login: a different id is a
        // different account, even though the display login matches.
        let impostor = ensure_identity_user(
            &pool,
            &Identity::github(67890, "octocat", Some("New Octo"), None),
            &[],
        )
        .await
        .unwrap();
        assert_ne!(original.id, impostor.id);
        assert_ne!(original.username, impostor.username);
        assert_eq!(impostor.access, UserAccess::Pending);

        // A rename of the original account keeps the original row.
        let renamed = ensure_identity_user(
            &pool,
            &Identity::github(12345, "octo-renamed", Some("Octo"), None),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(renamed.id, original.id);
    }
}
