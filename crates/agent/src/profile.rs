//! Built-in adapter profiles. Adding an ACP agent must not require changing
//! the thread event model or the UI.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub npm_package: Option<&'static str>,
    pub npm_version: Option<&'static str>,
    pub default_model: Option<&'static str>,
    pub modes: &'static [&'static str],
}

const CODEX_MODES: &[&str] = &["read-only", "workspace-write", "agent", "agent-full-access"];
const CLAUDE_MODES: &[&str] = &["default", "acceptEdits", "plan", "bypassPermissions"];

pub const CODEX: AgentProfile = AgentProfile {
    id: "codex",
    label: "Codex",
    command: "codex-acp",
    args: &[],
    npm_package: Some("@agentclientprotocol/codex-acp"),
    npm_version: Some("2.6.0"),
    default_model: None,
    modes: CODEX_MODES,
};

pub const CLAUDE: AgentProfile = AgentProfile {
    id: "claude",
    label: "Claude Code",
    command: "claude-agent-acp",
    args: &[],
    npm_package: Some("@agentclientprotocol/claude-agent-acp"),
    npm_version: Some("0.101.0"),
    default_model: None,
    modes: CLAUDE_MODES,
};

pub const OPENCODE: AgentProfile = AgentProfile {
    id: "opencode",
    label: "opencode",
    command: "opencode",
    args: &["acp"],
    npm_package: None,
    npm_version: None,
    default_model: None,
    modes: &[],
};

pub const PI: AgentProfile = AgentProfile {
    id: "pi",
    label: "pi",
    command: "pi-acp",
    args: &[],
    npm_package: Some("pi-acp"),
    npm_version: Some("0.0.34"),
    default_model: None,
    modes: &[],
};

#[must_use]
pub fn all_profiles() -> &'static [AgentProfile] {
    &[CODEX, CLAUDE, OPENCODE, PI]
}

#[must_use]
pub fn find_profile(id: &str) -> Option<&'static AgentProfile> {
    all_profiles().iter().find(|profile| profile.id == id)
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
                profile: *profile,
                program: command,
                args,
                via_npx: false,
            };
        }
    }
    if let Some(path) = find_in_path(profile.command) {
        return ResolvedProfile {
            profile: *profile,
            program: path.to_string_lossy().to_string(),
            args: profile.args.iter().map(|arg| (*arg).to_owned()).collect(),
            via_npx: false,
        };
    }
    if let (Some(package), Some(version)) = (profile.npm_package, profile.npm_version) {
        let mut args = vec!["--yes".to_owned(), format!("{package}@{version}")];
        args.extend(profile.args.iter().map(|arg| (*arg).to_owned()));
        return ResolvedProfile {
            profile: *profile,
            program: "npx".to_owned(),
            args,
            via_npx: true,
        };
    }
    ResolvedProfile {
        profile: *profile,
        program: profile.command.to_owned(),
        args: profile.args.iter().map(|arg| (*arg).to_owned()).collect(),
        via_npx: false,
    }
}

#[must_use]
pub fn find_in_path(command: &str) -> Option<PathBuf> {
    if command.contains('/') {
        let path = Path::new(command);
        return path.is_file().then(|| path.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = directory.join(format!("{command}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_local_binary() {
        let profile = AgentProfile {
            command: "sh",
            ..CODEX
        };
        let resolved = resolve(&profile);
        assert!(!resolved.via_npx);
        assert!(resolved.program.ends_with("sh"));
    }

    #[test]
    fn resolve_falls_back_to_pinned_npx() {
        let profile = AgentProfile {
            command: "definitely-not-installed-threadbase",
            ..CODEX
        };
        let resolved = resolve(&profile);
        assert!(resolved.via_npx);
        assert_eq!(resolved.args[1], "@agentclientprotocol/codex-acp@2.6.0");
    }
}
