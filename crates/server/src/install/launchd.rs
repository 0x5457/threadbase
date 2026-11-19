//! launchd plist contents.
//!
//! `service-manager` owns every `launchctl` call; the plist is built with the
//! `plist` crate, so escaping is never guessed. launchd has no
//! `EnvironmentFile=`, so the agent points at the same secrets file the systemd
//! unit does and the binary loads it at startup.

use crate::install::command;
use crate::install::spec::{self, ServiceSpec};
use plist::Value;
use std::path::{Path, PathBuf};

/// Where `service-manager` looks for user agents.
pub(crate) fn plist_path(home: &Path, label: &str) -> PathBuf {
    home.join("Library/LaunchAgents")
        .join(format!("{label}.plist"))
}

pub(crate) fn log_dir(home: &Path) -> PathBuf {
    home.join("Library/Logs/threadbase")
}

/// launchd creates the log files but not their directory.
pub(crate) fn prepare(home: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(log_dir(home))
        .map_err(|error| anyhow::anyhow!("create {}: {error}", log_dir(home).display()))
}

pub(crate) fn notes(spec: &ServiceSpec<'_>) -> Vec<String> {
    let mut notes = Vec::new();
    // A launch agent only runs inside a logged-in GUI session, so a reboot
    // without automatic login silently leaves the team without a server.
    if command::stdout(
        "defaults",
        &[
            "read",
            "/Library/Preferences/com.apple.loginwindow",
            "autoLoginUser",
        ],
    )
    .is_none()
    {
        notes.push(
            "automatic login is off: after a reboot the agent starts only once someone logs in \
             at the screen. Enable System Settings → Users & Groups → Automatically log in as."
                .to_owned(),
        );
    }
    if let Some(status) = command::stdout("fdesetup", &["status"]) {
        if status.contains("FileVault is On") {
            notes.push(
                "FileVault is on, which blocks automatic login. Keep FileVault off on a \
                 dedicated always-on Mac (or accept a manual login after every reboot)."
                    .to_owned(),
            );
        }
    }
    if !spec.keep_awake {
        notes.push(
            "system sleep will drop the tunnel for everyone; rerun with --keep-awake to hold \
             a power assertion."
                .to_owned(),
        );
    }
    notes
}

pub(crate) fn activation_lines(spec: &ServiceSpec<'_>) -> Vec<String> {
    vec![
        format!(
            "launchctl load {}",
            plist_path(spec.home, spec.label).display()
        ),
        format!("launchctl start {}", spec.label),
    ]
}

pub(crate) fn uninstall_lines(spec: &ServiceSpec<'_>) -> Vec<String> {
    vec![
        format!("launchctl remove {}", spec.label),
        format!("rm {}", plist_path(spec.home, spec.label).display()),
    ]
}

pub(crate) fn log_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        log_dir(home).join("launchd.err.log"),
        log_dir(home).join("launchd.out.log"),
    ]
}

pub(crate) fn plist_file(spec: &ServiceSpec<'_>) -> String {
    let mut root = plist::Dictionary::new();
    root.insert("Label".to_owned(), Value::String(spec.label.to_owned()));
    root.insert(
        "ProgramArguments".to_owned(),
        Value::Array(vec![
            Value::String(spec.exe.to_string_lossy().into_owned()),
            Value::String("serve".to_owned()),
        ]),
    );

    let mut environment = plist::Dictionary::new();
    let mut set = |key: &str, value: String| {
        environment.insert(key.to_owned(), Value::String(value));
    };
    set("THREADBASE_BIND", format!("127.0.0.1:{}", spec.port));
    set(
        "THREADBASE_DATA_DIR",
        spec.data_dir.to_string_lossy().into_owned(),
    );
    set(
        "THREADBASE_ENV_FILE",
        spec.env_file.to_string_lossy().into_owned(),
    );
    if let Some(origin) = spec.public_origin {
        set("THREADBASE_PUBLIC_ORIGIN", origin.to_owned());
    }
    if let Some(admin) = spec.admin {
        set("THREADBASE_ADMINS", admin.to_owned());
    }
    if spec.keep_awake {
        set("THREADBASE_KEEP_AWAKE", "1".to_owned());
    }
    if let Some(shell) = spec.shell {
        set("SHELL", shell.to_owned());
    }
    // launchd hands the job /usr/bin:/bin:/usr/sbin:/sbin only.
    let path = spec::service_path(
        spec.path_env,
        &["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"],
    );
    set("PATH", path.join(":"));
    drop(set);
    root.insert(
        "EnvironmentVariables".to_owned(),
        Value::Dictionary(environment),
    );

    root.insert("RunAtLoad".to_owned(), Value::Boolean(true));
    // Restart when the process fails, which covers a deploy: it installs the
    // new binary and exits 75 on purpose. A clean exit stays final.
    let mut keep_alive = plist::Dictionary::new();
    keep_alive.insert("SuccessfulExit".to_owned(), Value::Boolean(false));
    root.insert("KeepAlive".to_owned(), Value::Dictionary(keep_alive));
    // Without throttling a crash loop would respawn as fast as launchd can fork.
    root.insert("ThrottleInterval".to_owned(), Value::from(10u32));
    // launchd terminates the whole process group on exit; tmux servers are
    // daemonized children that must survive a restart, exactly like
    // `KillMode=process` on systemd.
    root.insert("AbandonProcessGroup".to_owned(), Value::Boolean(true));
    // Without this launchd applies App Nap to a long-running job.
    root.insert(
        "ProcessType".to_owned(),
        Value::String("Interactive".to_owned()),
    );
    // Give the server time for its graceful shutdown of agent children.
    root.insert("ExitTimeOut".to_owned(), Value::from(30u32));
    root.insert(
        "StandardOutPath".to_owned(),
        Value::String(
            log_dir(spec.home)
                .join("launchd.out.log")
                .to_string_lossy()
                .into_owned(),
        ),
    );
    root.insert(
        "StandardErrorPath".to_owned(),
        Value::String(
            log_dir(spec.home)
                .join("launchd.err.log")
                .to_string_lossy()
                .into_owned(),
        ),
    );

    let mut bytes = Vec::new();
    Value::Dictionary(root)
        .to_writer_xml(&mut bytes)
        .expect("a dictionary of strings always serializes");
    let mut contents = String::from_utf8(bytes).expect("plist output is UTF-8");
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>() -> ServiceSpec<'a> {
        ServiceSpec {
            label: "threadbase",
            exe: Path::new("/Applications/Thread & <Agent> 100%/threadbase"),
            home: Path::new("/Users/ada"),
            data_dir: Path::new("/Users/ada/Library/Application Support/threadbase"),
            port: 7777,
            public_origin: Some("https://ada-mac.tailnet.ts.net:8443"),
            admin: Some("ada@example.com"),
            path_env: Some("/Users/ada/.local/bin:/opt/homebrew/bin"),
            env_file: Path::new("/Users/ada/.config/threadbase/env"),
            shell: Some("/bin/zsh"),
            keep_awake: true,
        }
    }

    fn parse(contents: &str) -> plist::Dictionary {
        plist::Value::from_reader(std::io::Cursor::new(contents.as_bytes()))
            .unwrap()
            .as_dictionary()
            .unwrap()
            .clone()
    }

    #[test]
    fn plist_survives_a_round_trip_and_escapes_the_program_path() {
        let contents = plist_file(&spec());
        let dictionary = parse(&contents);
        let arguments = dictionary
            .get("ProgramArguments")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            arguments[0].as_string(),
            Some("/Applications/Thread & <Agent> 100%/threadbase")
        );
        assert_eq!(arguments[1].as_string(), Some("serve"));
        assert!(contents.contains("&amp;"), "{contents}");
    }

    #[test]
    fn plist_keeps_the_restart_and_process_group_contract() {
        let dictionary = parse(&plist_file(&spec()));
        assert_eq!(
            dictionary
                .get("KeepAlive")
                .unwrap()
                .as_dictionary()
                .unwrap()
                .get("SuccessfulExit")
                .unwrap()
                .as_boolean(),
            Some(false)
        );
        assert_eq!(
            dictionary.get("AbandonProcessGroup").unwrap().as_boolean(),
            Some(true)
        );
        assert_eq!(
            dictionary.get("RunAtLoad").unwrap().as_boolean(),
            Some(true)
        );
        assert_eq!(
            dictionary
                .get("ThrottleInterval")
                .unwrap()
                .as_unsigned_integer(),
            Some(10)
        );
        assert_eq!(
            dictionary.get("ProcessType").unwrap().as_string(),
            Some("Interactive")
        );
        assert!(dictionary.get("StandardErrorPath").is_some());
    }

    #[test]
    fn plist_environment_is_absolute_and_secret_free() {
        let dictionary = parse(&plist_file(&spec()));
        let environment = dictionary
            .get("EnvironmentVariables")
            .unwrap()
            .as_dictionary()
            .unwrap();
        let data_dir = environment
            .get("THREADBASE_DATA_DIR")
            .unwrap()
            .as_string()
            .unwrap();
        assert!(data_dir.starts_with('/'), "{data_dir}");
        assert!(
            !data_dir.contains('$'),
            "launchd does not expand variables: {data_dir}"
        );
        assert_eq!(
            environment
                .get("THREADBASE_KEEP_AWAKE")
                .unwrap()
                .as_string(),
            Some("1")
        );
        assert_eq!(
            environment.get("SHELL").unwrap().as_string(),
            Some("/bin/zsh")
        );
        let path = environment.get("PATH").unwrap().as_string().unwrap();
        assert!(path.contains("/opt/homebrew/bin"), "{path}");
        assert!(path.ends_with("/usr/bin:/bin:/usr/sbin:/sbin"), "{path}");
        assert!(
            !environment.contains_key("THREADBASE_WEBHOOK_URL"),
            "the plist must not carry secrets"
        );
    }

    #[test]
    fn optional_environment_is_omitted_when_unset() {
        let dictionary = parse(&plist_file(&ServiceSpec {
            public_origin: None,
            admin: None,
            shell: None,
            keep_awake: false,
            ..spec()
        }));
        let environment = dictionary
            .get("EnvironmentVariables")
            .unwrap()
            .as_dictionary()
            .unwrap();
        assert!(!environment.contains_key("THREADBASE_PUBLIC_ORIGIN"));
        assert!(!environment.contains_key("THREADBASE_ADMINS"));
        assert!(!environment.contains_key("THREADBASE_KEEP_AWAKE"));
    }

    #[test]
    fn files_live_where_launchd_looks_for_them() {
        assert_eq!(
            plist_path(Path::new("/Users/ada"), "threadbase"),
            PathBuf::from("/Users/ada/Library/LaunchAgents/threadbase.plist")
        );
        assert_eq!(
            log_dir(Path::new("/Users/ada")),
            PathBuf::from("/Users/ada/Library/Logs/threadbase")
        );
    }

    #[test]
    fn sleep_note_follows_the_flag() {
        let warnings = notes(&spec());
        assert!(
            !warnings.iter().any(|note| note.contains("--keep-awake")),
            "{warnings:?}"
        );
        let warnings = notes(&ServiceSpec {
            keep_awake: false,
            ..spec()
        });
        assert!(warnings.iter().any(|note| note.contains("--keep-awake")));
    }
}
