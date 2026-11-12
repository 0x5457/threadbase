//! Role and approval policy shared by HTTP handlers and the agent harness.

use std::path::Path;

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

/// Shell syntax that turns a command prefix into an unrelated command.
#[must_use]
pub fn command_is_reusable(command: &str) -> bool {
    !command.trim().is_empty()
        && !command.chars().any(|ch| {
            matches!(
                ch,
                ';' | '|'
                    | '&'
                    | '$'
                    | '`'
                    | '>'
                    | '<'
                    | '('
                    | ')'
                    | '\\'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '!'
                    | '\n'
                    | '\r'
                    | '\0'
            )
        })
}

/// A stored prefix only covers the same command or a longer command separated
/// by whitespace, never a renamed or compound command.
#[must_use]
pub fn command_matches_prefix(prefix: &str, command: &str) -> bool {
    if !command_is_reusable(prefix) || !command_is_reusable(command) {
        return false;
    }
    match command.strip_prefix(prefix) {
        Some("") => true,
        Some(rest) => rest.as_bytes().first().is_some_and(u8::is_ascii_whitespace),
        None => false,
    }
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
        Some(prefix) => command.is_some_and(|value| command_matches_prefix(prefix, value)),
    }
}

/// A thread-wide rule is not inherited when the child agent runs outside the
/// thread worktree root. Canonical paths make `root/../..` and string-prefix
/// siblings (`root-other`) fail the component-wise comparison.
#[must_use]
pub fn rule_inherits_to_child(rule_root: &str, child_cwd: &str) -> bool {
    let root = Path::new(rule_root);
    let child = Path::new(child_cwd);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let child = child.canonicalize().unwrap_or_else(|_| child.to_path_buf());
    child.starts_with(&root)
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
            Some("pnpm testing")
        ));
        assert!(!rule_matches(
            "codex",
            "execute",
            Some("pnpm test"),
            "codex",
            "execute",
            Some("pnpm test; rm -rf ~")
        ));
        assert!(!rule_matches(
            "codex",
            "execute",
            Some("pnpm test"),
            "codex",
            "execute",
            Some("pnpm test && x")
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
    fn shell_metacharacters_are_not_reusable() {
        assert!(!command_is_reusable(""));
        assert!(!command_is_reusable("   "));
        for command in [
            "pnpm test; rm -rf ~",
            "pnpm test && x",
            "pnpm test | sh",
            "pnpm test $HOME",
            "pnpm test `id`",
            "pnpm test > out",
            "pnpm test < in",
            "pnpm test (x)",
            "pnpm test \\",
            "pnpm test {x}",
            "pnpm test [x]",
            "pnpm test !x",
            "pnpm test\nrm -rf ~",
            "pnpm test\rrm -rf ~",
            "pnpm test\0rm -rf ~",
        ] {
            assert!(!command_is_reusable(command), "{command:?}");
            assert!(!command_matches_prefix("pnpm test", command), "{command:?}");
        }
        assert!(command_is_reusable("pnpm test -- --run"));
        assert!(command_matches_prefix("pnpm test", "pnpm test"));
        assert!(command_matches_prefix("pnpm test", "pnpm test -- --run"));
        assert!(!command_matches_prefix("pnpm test", "pnpm testing"));
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

    #[test]
    fn worktree_traversal_does_not_inherit() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "threadbase-permissions-traversal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("root");
        let nested = root.join("nested");
        let sibling = base.join("root-other");
        std::fs::create_dir_all(&nested).expect("create worktree");
        std::fs::create_dir_all(&sibling).expect("create sibling");
        let root = root.to_str().expect("utf-8 temp path");
        let traversal = base.join("root").join("..").join("root-other");
        assert!(rule_inherits_to_child(
            root,
            nested.to_str().expect("utf-8 nested path")
        ));
        assert!(!rule_inherits_to_child(
            root,
            sibling.to_str().expect("utf-8 sibling path")
        ));
        assert!(!rule_inherits_to_child(
            root,
            traversal.to_str().expect("utf-8 traversal path")
        ));
        std::fs::remove_dir_all(&base).expect("remove scratch directory");
    }
}
