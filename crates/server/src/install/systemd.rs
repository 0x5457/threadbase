//! systemd unit contents.
//!
//! `service-manager` owns every `systemctl` call; only the text is ours,
//! because the crate's template cannot express `KillMode=process` (tmux
//! servers must survive a restart) or a secrets file instead of inline values.

use crate::install::spec::{self, ServiceSpec};
use std::path::PathBuf;

/// Where `service-manager` puts user units, so `--print` shows the real path.
pub(crate) fn unit_path(label: &str) -> PathBuf {
    service_manager::systemd_user_dir_path()
        .unwrap_or_else(|_| PathBuf::from("~/.config/systemd/user"))
        .join(format!("{label}.service"))
}

pub(crate) fn notes(spec: &ServiceSpec<'_>) -> Vec<String> {
    if spec.keep_awake {
        return vec![
            "--keep-awake has no effect here: Linux hosts are not expected to suspend. \
             Mask sleep.target if this machine does."
                .to_owned(),
        ];
    }
    Vec::new()
}

pub(crate) fn activation_lines(spec: &ServiceSpec<'_>) -> Vec<String> {
    vec![
        format!(
            "systemctl --user enable {}",
            unit_path(spec.label).display()
        ),
        "systemctl --user daemon-reload".to_owned(),
        format!("systemctl --user start {}", spec.label),
    ]
}

pub(crate) fn uninstall_lines(spec: &ServiceSpec<'_>) -> Vec<String> {
    vec![
        format!("systemctl --user disable --now {}", spec.label),
        format!("rm {}", unit_path(spec.label).display()),
        "systemctl --user daemon-reload".to_owned(),
    ]
}

pub(crate) fn unit_file(spec: &ServiceSpec<'_>) -> String {
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
    // The binary loads this file itself, exactly like the launchd agent does.
    unit.push_str(&format!(
        "Environment=\"THREADBASE_ENV_FILE={}\"\n",
        escape_quoted(&spec.env_file.to_string_lossy())
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
    // Agent adapters usually live in PATH entries the login shell adds.
    let path = spec::service_path(spec.path_env, &[]);
    unit.push_str(&format!(
        "Environment=\"PATH={}\"\n",
        escape_quoted(&path.join(":"))
    ));
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=2\n");
    // Tmux servers are daemonized children. A normal restart must detach the
    // browser clients, not kill the shells; adapters are stopped explicitly by
    // the server before it exits.
    unit.push_str("KillMode=process\n");
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
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
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
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

/// Plain environment assignments stay unquoted unless the value needs quoting.
fn environment_value(value: &str) -> String {
    if value.chars().any(|character| {
        character.is_whitespace() || character.is_control() || matches!(character, '"' | '\\')
    }) {
        format!("\"{}\"", escape_quoted(value))
    } else {
        escape_specifiers(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn spec<'a>() -> ServiceSpec<'a> {
        ServiceSpec {
            label: "threadbase",
            exe: Path::new("/home/ada/.local/bin/threadbase"),
            home: Path::new("/home/ada"),
            data_dir: Path::new("/home/ada/.local/share/threadbase"),
            port: 7777,
            public_origin: Some("https://machine.tailnet.ts.net:8443"),
            admin: Some("ada@example.com"),
            path_env: Some("/usr/local/bin:/usr/bin:/usr/local/bin"),
            env_file: Path::new("/home/ada/.config/threadbase/env"),
            shell: Some("/bin/bash"),
            keep_awake: false,
        }
    }

    #[test]
    fn unit_runs_the_current_binary_and_binds_loopback() {
        let unit = unit_file(&spec());
        assert!(unit.contains("ExecStart=\"/home/ada/.local/bin/threadbase\" serve"));
        assert!(unit.contains("Environment=THREADBASE_BIND=127.0.0.1:7777"));
        assert!(unit.contains("THREADBASE_DATA_DIR=/home/ada/.local/share/threadbase"));
        assert!(unit.contains("THREADBASE_PUBLIC_ORIGIN=https://machine.tailnet.ts.net:8443"));
        assert!(unit.contains("THREADBASE_ADMINS=ada@example.com"));
        assert!(
            unit.contains("THREADBASE_ENV_FILE=/home/ada/.config/threadbase/env"),
            "{unit}"
        );
        assert!(
            unit.contains("\"PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin\""),
            "{unit}"
        );
        assert!(unit.contains("KillMode=process"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(
            !unit.contains("0.0.0.0"),
            "unit must never expose a public bind"
        );
        assert!(
            !unit.contains("CLIENT_SECRET"),
            "unit must never carry a secret"
        );
    }

    #[test]
    fn unit_name_drives_the_unit_path() {
        let path = unit_path("threadbase-dev");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("threadbase-dev.service")
        );
        assert!(path.to_string_lossy().contains("systemd/user"), "{path:?}");
    }

    #[test]
    fn unit_escapes_control_characters() {
        let unit = unit_file(&ServiceSpec {
            data_dir: Path::new("/tmp/data\nExecStartPre=/bin/false"),
            ..spec()
        });
        assert!(!unit.contains("\nExecStartPre="), "{unit}");
        assert!(unit.contains("\\nExecStartPre="), "{unit}");
    }

    #[test]
    fn unit_omits_optional_environment() {
        let unit = unit_file(&ServiceSpec {
            port: 7788,
            public_origin: None,
            admin: None,
            path_env: None,
            ..spec()
        });
        assert!(!unit.contains("THREADBASE_PUBLIC_ORIGIN"));
        assert!(!unit.contains("THREADBASE_ADMINS"));
        assert!(unit.contains("THREADBASE_BIND=127.0.0.1:7788"));
        assert!(
            unit.contains("\"PATH=/usr/bin:/bin:/usr/sbin:/sbin\""),
            "{unit}"
        );
    }

    #[test]
    fn unit_escapes_specifiers_and_quotes_arguments() {
        let unit = unit_file(&ServiceSpec {
            exe: Path::new("/opt/Threadbase 100%/threadbase \"bin\""),
            data_dir: Path::new("/home/ada/100% data"),
            public_origin: Some("https://example.test/100%"),
            admin: Some("ada%admin"),
            path_env: Some("/opt/100%/bin:/usr/bin"),
            ..spec()
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
        assert!(unit.contains("\"PATH=/opt/100%%/bin:/usr/bin"), "{unit}");
    }
}
