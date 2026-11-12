//! systemd user-unit paths and rendering.

use anyhow::Context;
use std::path::{Component, Path, PathBuf};
use threadbase_agent::profile::find_in_path;

pub(crate) fn path(unit: &str) -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(path_in(Path::new(&home), unit))
}

fn path_in(home: &Path, unit: &str) -> PathBuf {
    home.join(".config/systemd/user")
        .join(format!("{unit}.service"))
}

pub(crate) fn write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

pub(crate) struct UnitSpec<'a> {
    pub(crate) exe: &'a Path,
    pub(crate) data_dir: &'a Path,
    pub(crate) port: u16,
    pub(crate) public_origin: Option<&'a str>,
    pub(crate) admin: Option<&'a str>,
    pub(crate) path_env: Option<&'a str>,
}

pub(crate) fn render(spec: &UnitSpec<'_>) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=Threadbase local agent workspace\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n");
    unit.push_str("StartLimitIntervalSec=0\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("ExecStart=\"{}\" serve\n", spec.exe.display()));
    unit.push_str(&format!(
        "Environment=THREADBASE_BIND=127.0.0.1:{}\n",
        spec.port
    ));
    unit.push_str(&format!(
        "Environment=\"THREADBASE_DATA_DIR={}\"\n",
        spec.data_dir.display()
    ));
    if let Some(origin) = spec.public_origin {
        unit.push_str(&format!("Environment=THREADBASE_PUBLIC_ORIGIN={origin}\n"));
    }
    if let Some(admin) = spec.admin {
        unit.push_str(&format!("Environment=THREADBASE_ADMINS={admin}\n"));
    }
    // Secrets stay out of the unit, which systemd reads as a world-readable file.
    unit.push_str("EnvironmentFile=-%h/.config/threadbase/env\n");
    // Agent adapters usually live in PATH entries the login shell adds.
    if let Some(path) = spec.path_env {
        let mut seen: Vec<&str> = Vec::new();
        for entry in path.split(':').filter(|entry| !entry.is_empty()) {
            if !seen.contains(&entry) {
                seen.push(entry);
            }
        }
        unit.push_str(&format!("Environment=\"PATH={}\"\n", seen.join(":")));
    }
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=2\n");
    unit.push_str("NoNewPrivileges=yes\n");
    unit.push_str("PrivateTmp=no\n\n");
    unit.push_str("[Install]\n");
    unit.push_str("WantedBy=default.target\n");
    unit
}

/// The path this process was started from, so the unit points at the same file
/// the caller used rather than a resolved build artifact.
pub(crate) fn current_binary() -> PathBuf {
    let argv0 = std::env::args().next().unwrap_or_default();
    if argv0.contains('/') {
        let path = PathBuf::from(&argv0);
        if path.is_absolute() {
            return normalize(&path);
        }
        if let Ok(cwd) = std::env::current_dir() {
            return normalize(&cwd.join(path));
        }
    } else if !argv0.is_empty() {
        if let Some(path) = find_in_path(&argv0) {
            return path;
        }
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("threadbase"))
}

/// Lexical cleanup so the unit reads `/opt/bin/threadbase`, not `/opt/./bin/x`.
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::RootDir | Component::Prefix(_) => {}
            Component::Normal(part) => parts.push(part.to_owned()),
        }
    }
    let mut output = PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
    for part in parts {
        output.push(part);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>() -> UnitSpec<'a> {
        UnitSpec {
            exe: Path::new("/home/ada/.local/bin/threadbase"),
            data_dir: Path::new("/home/ada/.local/share/threadbase"),
            port: 7777,
            public_origin: Some("https://machine.tailnet.ts.net:8443"),
            admin: Some("ada@example.com"),
            path_env: Some("/usr/local/bin:/usr/bin:/usr/local/bin"),
        }
    }

    #[test]
    fn unit_runs_the_current_binary_and_binds_loopback() {
        let unit = render(&spec());
        assert!(unit.contains("ExecStart=\"/home/ada/.local/bin/threadbase\" serve"));
        assert!(unit.contains("Environment=THREADBASE_BIND=127.0.0.1:7777"));
        assert!(unit.contains("THREADBASE_DATA_DIR=/home/ada/.local/share/threadbase"));
        assert!(unit.contains("THREADBASE_PUBLIC_ORIGIN=https://machine.tailnet.ts.net:8443"));
        assert!(unit.contains("THREADBASE_ADMINS=ada@example.com"));
        assert!(unit.contains("EnvironmentFile=-%h/.config/threadbase/env"));
        assert!(unit.contains("\"PATH=/usr/local/bin:/usr/bin\""), "{unit}");
        assert!(unit.contains("WantedBy=default.target"));
        assert!(
            !unit.contains("0.0.0.0"),
            "unit must never expose a public bind"
        );
    }

    #[test]
    fn unit_omits_optional_environment() {
        let unit = render(&UnitSpec {
            exe: Path::new("/bin/threadbase"),
            data_dir: Path::new("/tmp/data"),
            port: 7788,
            public_origin: None,
            admin: None,
            path_env: None,
        });
        assert!(!unit.contains("THREADBASE_PUBLIC_ORIGIN"));
        assert!(!unit.contains("THREADBASE_ADMINS"));
        assert!(!unit.contains("PATH="));
        assert!(unit.contains("THREADBASE_BIND=127.0.0.1:7788"));
    }

    #[test]
    fn unit_path_lives_under_the_user_config_dir() {
        assert_eq!(
            path_in(Path::new("/home/ada"), "threadbase-dev"),
            PathBuf::from("/home/ada/.config/systemd/user/threadbase-dev.service")
        );
    }

    #[test]
    fn paths_are_normalized() {
        assert_eq!(
            normalize(Path::new("/home/ada/./bin/../bin/threadbase")),
            PathBuf::from("/home/ada/bin/threadbase")
        );
    }
}
