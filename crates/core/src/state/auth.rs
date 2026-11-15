use threadbase_shared::ActionKind;
use threadbase_shared::AuthSessionState;

use super::{illegal, TransitionError};

// ---------------------------------------------------------------------------
// Auth session
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSessionAction {
    Create,
    Touch,
    Revoke,
    Expire,
}

pub struct AuthSessionMachine;

impl AuthSessionMachine {
    pub const ENTITY: &'static str = "auth_session";

    /// Classify a row. This is the only place the two timestamps become a
    /// state, so callers stop hand-writing `revoked_at IS NULL AND ...`.
    pub fn classify(revoked_at: Option<i64>, expires_at: i64, now: i64) -> AuthSessionState {
        if revoked_at.is_some() {
            AuthSessionState::Revoked
        } else if expires_at <= now {
            AuthSessionState::Expired
        } else {
            AuthSessionState::Active
        }
    }

    /// SQL predicate for a live session, with `?` bound to `now`.
    pub fn live_predicate(alias: &str) -> String {
        format!("{alias}.revoked_at IS NULL AND {alias}.expires_at > ?")
    }

    pub fn transition(
        current: AuthSessionState,
        action: &AuthSessionAction,
    ) -> Result<AuthSessionState, TransitionError> {
        let next = match action {
            AuthSessionAction::Create => AuthSessionState::Active,
            AuthSessionAction::Touch => match current {
                AuthSessionState::Active => AuthSessionState::Active,
                _ => return Err(illegal(Self::ENTITY, current, "touch")),
            },
            AuthSessionAction::Revoke => match current {
                AuthSessionState::Active => AuthSessionState::Revoked,
                _ => return Err(illegal(Self::ENTITY, current, "revoke")),
            },
            AuthSessionAction::Expire => match current {
                AuthSessionState::Active => AuthSessionState::Expired,
                _ => return Err(illegal(Self::ENTITY, current, "expire")),
            },
        };
        Ok(next)
    }

    pub fn commands(state: AuthSessionState) -> &'static [AuthSessionAction] {
        match state {
            AuthSessionState::Active => &[AuthSessionAction::Touch, AuthSessionAction::Revoke],
            AuthSessionState::Expired | AuthSessionState::Revoked => &[],
        }
    }
}

impl AuthSessionAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Touch => "touch",
            Self::Revoke => "revoke",
            Self::Expire => "expire",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Create => "Signed in",
            Self::Touch => "Refresh session",
            Self::Revoke => "Sign out",
            Self::Expire => "Expired",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Touch | Self::Revoke => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}
