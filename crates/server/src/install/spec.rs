//! What every service manager needs, and the one file it is asked to write.

use anyhow::Context;
use std::path::{Path, PathBuf};
use threadbase_agent::profile::find_in_path;
use threadbase_workspace::path::clean_path;

/// Everything a platform renderer may read.
///
/// The values come from the installing shell (`PATH`, `SHELL`), the parsed
/// `Config` and the command line. No secret is ever part of it: both platforms
/// point the binary at [`ServiceSpec::env_file`] instead.
pub(crate) struct ServiceSpec<'a> {
    /// systemd unit name and launchd label; also names the generated files.
    pub(crate) label: &'a str,
    pub(crate) exe: &'a Path,
    pub(crate) home: &'a Path,
    pub(crate) data_dir: &'a Path,
    pub(crate) port: u16,
    pub(crate) public_origin: Option<&'a str>,
    pub(crate) admin: Option<&'a str>,
    pub(crate) path_env: Option<&'a str>,
    pub(crate) env_file: &'a Path,
    /// Login shell for the browser terminal.
    pub(crate) shell: Option<&'a str>,
    pub(crate) keep_awake: bool,
}

/// The service file: what `service-manager` writes, and what `--print` shows.
pub(crate) struct Artifact {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

/// Writes the service file atomically enough for a unit: create the directory,
/// then replace the contents. Returns whether the contents were already there,
/// so `install` can say "Unchanged" instead of claiming work it did not do.
pub(crate) fn write(artifact: &Artifact) -> anyhow::Result<bool> {
    if let Some(parent) = artifact.path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let unchanged = std::fs::read_to_string(&artifact.path)
        .map(|existing| existing == artifact.contents)
        .unwrap_or(false);
    std::fs::write(&artifact.path, &artifact.contents)
        .with_context(|| format!("write {}", artifact.path.display()))?;
    Ok(unchanged)
}

pub(crate) fn home() -> anyhow::Result<PathBuf> {
    crate::config::home_dir().context("cannot determine the home directory; set HOME")
}

/// PATH for the service process.
///
/// Service managers start with a minimal environment, so the agents installed
/// through mise, nvm or Homebrew only resolve if the installing shell's entries
/// are carried over. System directories are always appended, and `extra` holds
/// platform package roots that are easy to miss.
pub(crate) fn service_path(path_env: Option<&str>, extra: &[&str]) -> Vec<String> {
    let inherited = path_env.unwrap_or_default();
    let mut entries: Vec<String> = Vec::new();
    for entry in inherited
        .split(':')
        .filter(|entry| !entry.is_empty())
        .chain(extra.iter().copied())
        .chain(["/usr/bin", "/bin", "/usr/sbin", "/sbin"])
    {
        if !entries.iter().any(|seen| seen == entry) {
            entries.push(entry.to_owned());
        }
    }
    entries
}

/// The path this process was started from, so the unit points at the same file
/// the caller used rather than a resolved build artifact.
pub(crate) fn current_binary() -> PathBuf {
    let argv0 = std::env::args().next().unwrap_or_default();
    if argv0.contains('/') {
        let path = PathBuf::from(&argv0);
        if path.is_absolute() {
            return clean_path(&path);
        }
        if let Ok(cwd) = std::env::current_dir() {
            return clean_path(&cwd.join(path));
        }
    } else if !argv0.is_empty() {
        if let Some(path) = find_in_path(&argv0) {
            return path;
        }
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("threadbase"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_path_deduplicates_and_keeps_system_directories() {
        assert_eq!(
            service_path(Some("/opt/homebrew/bin:/usr/bin:/opt/homebrew/bin"), &[]),
            vec![
                "/opt/homebrew/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin"
            ]
        );
    }

    #[test]
    fn service_path_places_extra_roots_after_the_inherited_ones() {
        assert_eq!(
            service_path(Some("/Users/me/.local/bin:/bin"), &["/opt/homebrew/bin"]),
            vec![
                "/Users/me/.local/bin",
                "/bin",
                "/opt/homebrew/bin",
                "/usr/bin",
                "/usr/sbin",
                "/sbin"
            ]
        );
    }

    #[test]
    fn empty_path_still_yields_a_usable_environment() {
        assert_eq!(
            service_path(None, &[]),
            vec!["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        );
    }
}
