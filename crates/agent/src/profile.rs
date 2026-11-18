//! Adapter profiles. The built-ins are constants; the server also builds
//! profiles from workspace-configured custom agents, so adding an ACP agent
//! requires no new code and no thread event model or UI change.

use std::path::PathBuf;
use std::sync::LazyLock;
use threadbase_shared::PermissionMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub npm_package: Option<String>,
    pub npm_version: Option<String>,
    pub default_model: Option<String>,
    pub modes: &'static [&'static str],
    /// Provider session-mode id for each of our permission modes.
    ///
    /// `None` means the provider has no equivalent, so the server's own policy
    /// answers the requests instead. Guessing an id here would silently give a
    /// thread more or less autonomy than its mode says.
    pub permission_modes: &'static [(PermissionMode, Option<&'static str>)],
}

impl AgentProfile {
    /// A workspace-configured agent. It has no provider-side session modes,
    /// so the server's policy answers every permission request itself.
    #[must_use]
    pub fn custom(
        id: String,
        label: String,
        command: String,
        args: Vec<String>,
        default_model: Option<String>,
    ) -> Self {
        Self {
            id,
            label,
            command,
            args,
            npm_package: None,
            npm_version: None,
            default_model,
            modes: &[],
            permission_modes: &[],
        }
    }
}

const CODEX_MODES: &[&str] = &["read-only", "workspace-write", "agent", "agent-full-access"];
const CLAUDE_MODES: &[&str] = &["default", "acceptEdits", "plan", "bypassPermissions"];

/// Codex has no "accept edits but ask for commands" step: `agent` asks, and
/// `agent-full-access` does not. Auto-accept edits therefore falls back to the
/// server policy, which answers edit requests itself.
const CODEX_PERMISSION_MODES: &[(PermissionMode, Option<&str>)] = &[
    (PermissionMode::Supervised, Some("agent")),
    (PermissionMode::AutoEdit, None),
    (PermissionMode::FullAccess, Some("agent-full-access")),
];

const CLAUDE_PERMISSION_MODES: &[(PermissionMode, Option<&str>)] = &[
    (PermissionMode::Supervised, Some("default")),
    (PermissionMode::AutoEdit, Some("acceptEdits")),
    (PermissionMode::FullAccess, Some("bypassPermissions")),
];

fn built_in(
    id: &str,
    label: &str,
    command: &str,
    args: &[&str],
    npm: Option<(&str, &str)>,
    modes: &'static [&'static str],
    permission_modes: &'static [(PermissionMode, Option<&'static str>)],
) -> AgentProfile {
    AgentProfile {
        id: id.to_owned(),
        label: label.to_owned(),
        command: command.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        npm_package: npm.map(|(package, _)| package.to_owned()),
        npm_version: npm.map(|(_, version)| version.to_owned()),
        default_model: None,
        modes,
        permission_modes,
    }
}

static PROFILES: LazyLock<Vec<AgentProfile>> = LazyLock::new(|| {
    vec![
        built_in(
            "codex",
            "Codex",
            "codex-acp",
            &[],
            Some(("@agentclientprotocol/codex-acp", "2.6.0")),
            CODEX_MODES,
            CODEX_PERMISSION_MODES,
        ),
        built_in(
            "claude",
            "Claude Code",
            "claude-agent-acp",
            &[],
            Some(("@agentclientprotocol/claude-agent-acp", "0.101.0")),
            CLAUDE_MODES,
            CLAUDE_PERMISSION_MODES,
        ),
        built_in("opencode", "opencode", "opencode", &["acp"], None, &[], &[]),
        built_in(
            "pi",
            "pi",
            "pi-acp",
            &[],
            Some(("pi-acp", "0.0.34")),
            &[],
            &[],
        ),
    ]
});

#[must_use]
pub fn all_profiles() -> &'static [AgentProfile] {
    &PROFILES
}

#[must_use]
pub fn find_profile(id: &str) -> Option<&'static AgentProfile> {
    all_profiles().iter().find(|profile| profile.id == id)
}

/// The provider's own mode id for one of our permission modes.
#[must_use]
pub fn provider_mode(profile: &AgentProfile, mode: PermissionMode) -> Option<&'static str> {
    profile
        .permission_modes
        .iter()
        .find(|(ours, _)| *ours == mode)
        .and_then(|(_, provider)| *provider)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProfile {
    pub profile: AgentProfile,
    pub program: String,
    pub args: Vec<String>,
    /// True when the adapter is launched through `npx` rather than a local binary.
    pub via_npx: bool,
}

/// Prefer a locally installed adapter; fall back to a pinned npx package.
#[must_use]
pub fn resolve(profile: &AgentProfile) -> ResolvedProfile {
    if let Ok(command) = std::env::var("THREADBASE_ACP_COMMAND") {
        if !command.trim().is_empty() {
            let args = std::env::var("THREADBASE_ACP_ARGS")
                .map(|value| value.split_whitespace().map(ToOwned::to_owned).collect())
                .unwrap_or_default();
            return ResolvedProfile {
                profile: profile.clone(),
                program: command,
                args,
                via_npx: false,
            };
        }
    }
    if let Some(path) = find_in_path(&profile.command) {
        return ResolvedProfile {
            profile: profile.clone(),
            program: path.to_string_lossy().to_string(),
            args: profile.args.clone(),
            via_npx: false,
        };
    }
    if let (Some(package), Some(version)) = (&profile.npm_package, &profile.npm_version) {
        let mut args = vec!["--yes".to_owned(), format!("{package}@{version}")];
        args.extend(profile.args.iter().cloned());
        return ResolvedProfile {
            profile: profile.clone(),
            program: "npx".to_owned(),
            args,
            via_npx: true,
        };
    }
    ResolvedProfile {
        profile: profile.clone(),
        program: profile.command.clone(),
        args: profile.args.clone(),
        via_npx: false,
    }
}

#[must_use]
pub fn find_in_path(command: &str) -> Option<PathBuf> {
    which::which(command).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_id() {
        let mut ids = std::collections::HashSet::new();
        for profile in all_profiles() {
            assert!(ids.insert(profile.id.as_str()));
        }
    }

    #[test]
    fn profiles_cover_documented_agents() {
        for id in ["codex", "claude", "opencode", "pi"] {
            assert!(find_profile(id).is_some(), "missing profile {id}");
        }
    }

    #[test]
    fn custom_profiles_carry_no_provider_modes() {
        let profile = AgentProfile::custom(
            "my-agent".to_owned(),
            "My agent".to_owned(),
            "my-agent".to_owned(),
            vec!["acp".to_owned()],
            None,
        );
        assert_eq!(profile.modes.len(), 0);
        assert_eq!(provider_mode(&profile, PermissionMode::FullAccess), None);
    }

    fn codex_with(command: &str) -> AgentProfile {
        AgentProfile {
            command: command.to_owned(),
            ..find_profile("codex").expect("codex profile").clone()
        }
    }

    #[test]
    fn resolve_prefers_local_binary() {
        let resolved = resolve(&codex_with("sh"));
        assert!(!resolved.via_npx);
        assert!(resolved.program.ends_with("sh"));
    }

    #[test]
    fn resolve_falls_back_to_pinned_npx() {
        let resolved = resolve(&codex_with("definitely-not-installed-threadbase"));
        assert!(resolved.via_npx);
        assert_eq!(resolved.args[1], "@agentclientprotocol/codex-acp@2.6.0");
    }
}
