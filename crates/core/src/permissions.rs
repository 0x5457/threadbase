//! Role and approval policy shared by HTTP handlers and the agent harness.

use threadbase_shared::{ApprovalScope, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("the current role cannot write to this thread")]
    ReadOnly,
    #[error("the current role cannot manage this thread")]
    NotOwner,
}

#[must_use]
pub fn can_write(role: Role) -> bool {
    role.can_write()
}

#[must_use]
pub fn can_manage(role: Role) -> bool {
    role.can_manage()
}

pub fn require_write(role: Role) -> Result<(), PolicyError> {
    if role.can_write() {
        Ok(())
    } else {
        Err(PolicyError::ReadOnly)
    }
}

pub fn require_manage(role: Role) -> Result<(), PolicyError> {
    if role.can_manage() {
        Ok(())
    } else {
        Err(PolicyError::NotOwner)
    }
}

/// Scopes that can be reused for later requests without prompting again.
#[must_use]
pub fn scope_is_reusable(scope: ApprovalScope) -> bool {
    matches!(scope, ApprovalScope::Session | ApprovalScope::Thread)
}

/// Thread-wide allow rules must match agent, tool and an optional command prefix.
#[must_use]
pub fn rule_matches(
    rule_agent: &str,
    rule_tool: &str,
    rule_prefix: Option<&str>,
    agent: &str,
    tool: &str,
    command: Option<&str>,
) -> bool {
    if rule_agent != agent || rule_tool != tool {
        return false;
    }
    match rule_prefix {
        None => true,
        Some(prefix) => command.is_some_and(|value| value.starts_with(prefix)),
    }
}

/// A thread-wide rule is not inherited when the child agent runs outside the
/// thread worktree root.
#[must_use]
pub fn rule_inherits_to_child(rule_root: &str, child_cwd: &str) -> bool {
    child_cwd == rule_root || child_cwd.starts_with(&format!("{rule_root}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewers_cannot_write() {
        assert!(require_write(Role::Viewer).is_err());
        assert!(require_write(Role::Editor).is_ok());
        assert!(require_write(Role::Owner).is_ok());
    }

    #[test]
    fn only_owners_manage() {
        assert!(require_manage(Role::Editor).is_err());
        assert!(require_manage(Role::Owner).is_ok());
    }

    #[test]
    fn command_prefix_rules_are_precise() {
        assert!(rule_matches(
            "codex",
            "execute",
            Some("pnpm test"),
            "codex",
            "execute",
            Some("pnpm test -- --run")
        ));
        assert!(!rule_matches(
            "codex",
            "execute",
            Some("pnpm test"),
            "codex",
            "execute",
            Some("rm -rf /")
        ));
    }

    #[test]
    fn child_outside_worktree_does_not_inherit() {
        assert!(rule_inherits_to_child(
            "/repo/.threadbase/wt",
            "/repo/.threadbase/wt/app"
        ));
        assert!(!rule_inherits_to_child(
            "/repo/.threadbase/wt",
            "/tmp/other"
        ));
    }
}
