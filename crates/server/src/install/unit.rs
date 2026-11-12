//! systemd user-unit paths and rendering.

use anyhow::Context;
use std::path::{Path, PathBuf};
use threadbase_agent::profile::find_in_path;
use threadbase_workspace::path::clean_path;

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
    unit.push_str(&format!(
        "ExecStart={} serve\n",
        quote_argument(&spec.exe.to_string_lossy())
    ));
    unit.push_str(&format!(
        "Environment=THREADBASE_BIND=127.0.0.1:{}\n",
        spec.port
    ));
    unit.push_str(&format!(
        "Environment=\"THREADBASE_DATA_DIR={}\"\n",
        escape_quoted(&spec.data_dir.to_string_lossy())
    ));
    if let Some(origin) = spec.public_origin {
        unit.push_str(&format!(
            "Environment=THREADBASE_PUBLIC_ORIGIN={}\n",
            environment_value(origin)
        ));
    }
    if let Some(admin) = spec.admin {
        unit.push_str(&format!(
            "Environment=THREADBASE_ADMINS={}\n",
            environment_value(admin)
        ));
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
        unit.push_str(&format!(
            "Environment=\"PATH={}\"\n",
            escape_quoted(&seen.join(":"))
        ));
    }
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=2\n");
    unit.push_str("NoNewPrivileges=yes\n");
    unit.push_str("PrivateTmp=no\n\n");
    unit.push_str("[Install]\n");
    unit.push_str("WantedBy=default.target\n");
    unit
}

/// `%` starts a systemd specifier even inside quotes, so literal text must use
/// `%%`.
fn escape_specifiers(value: &str) -> String {
    value.replace('%', "%%")
}

/// Escape a value that is placed inside double quotes in the unit file.
fn escape_quoted(value: &str) -> String {
    escape_specifiers(value)
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Quote one ExecStart argument, including the executable itself.
fn quote_argument(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '%' => quoted.push_str("%%"),
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '$' => quoted.push_str("$$"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

/// Plain environment assignments stay unquoted unless the value needs quoting.
fn environment_value(value: &str) -> String {
    if value
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '"' | '\\'))
    {
        format!("\"{}\"", escape_quoted(value))
    } else {
        escape_specifiers(value)
    }
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
    fn unit_escapes_specifiers_and_quotes_arguments() {
        let unit = render(&UnitSpec {
            exe: Path::new("/opt/Threadbase 100%/threadbase \"bin\""),
            data_dir: Path::new("/home/ada/100% data"),
            port: 7777,
            public_origin: Some("https://example.test/100%"),
            admin: Some("ada%admin"),
            path_env: Some("/opt/100%/bin:/usr/bin"),
        });
        assert!(
            unit.contains("ExecStart=\"/opt/Threadbase 100%%/threadbase \\\"bin\\\"\" serve"),
            "{unit}"
        );
        assert!(
            unit.contains("Environment=\"THREADBASE_DATA_DIR=/home/ada/100%% data\""),
            "{unit}"
        );
        assert!(
            unit.contains("THREADBASE_PUBLIC_ORIGIN=https://example.test/100%%"),
            "{unit}"
        );
        assert!(unit.contains("THREADBASE_ADMINS=ada%%admin"), "{unit}");
        assert!(unit.contains("\"PATH=/opt/100%%/bin:/usr/bin\""), "{unit}");
        assert!(unit.contains("EnvironmentFile=-%h/.config/threadbase/env"));
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
            clean_path(Path::new("/home/ada/./bin/../bin/threadbase")),
            PathBuf::from("/home/ada/bin/threadbase")
        );
    }
}
