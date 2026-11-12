//! Generate service-manager files without installing them silently.

use crate::install::unit;
use anyhow::Context;
#[cfg(any(target_os = "macos", test))]
use std::path::Path;

pub fn install(print_only: bool) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("resolve current binary")?;
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").context("HOME is not set")?;
        let path =
            std::path::PathBuf::from(format!("{home}/Library/LaunchAgents/dev.threadbase.plist"));
        let contents = launchd_plist(&exe)?;
        if print_only {
            print!("{contents}");
        } else {
            unit::write(&path, &contents)?;
            println!("Wrote {}", path.display());
            println!("Run: launchctl load -w {}", path.display());
        }
    }
    #[cfg(target_os = "linux")]
    {
        let config = crate::config::Config::load().context("invalid configuration")?;
        let path = unit::path("threadbase")?;
        let contents = unit::render(&unit::UnitSpec {
            exe: &exe,
            data_dir: &config.data_dir,
            port: config.bind.port(),
            public_origin: None,
            admin: None,
            path_env: std::env::var("PATH").ok().as_deref(),
        });
        if print_only {
            print!("{contents}");
        } else {
            unit::write(&path, &contents)?;
            println!("Wrote {}", path.display());
            println!(
                "Run: systemctl --user daemon-reload && systemctl --user enable --now threadbase"
            );
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (print_only, exe);
        anyhow::bail!("service install is supported on macOS and Linux only");
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn launchd_plist(exe: &Path) -> anyhow::Result<String> {
    let mut dictionary = plist::Dictionary::new();
    dictionary.insert(
        "Label".to_owned(),
        plist::Value::String("dev.threadbase".to_owned()),
    );
    dictionary.insert(
        "ProgramArguments".to_owned(),
        plist::Value::Array(vec![
            plist::Value::String(exe.to_string_lossy().into_owned()),
            plist::Value::String("serve".to_owned()),
        ]),
    );
    dictionary.insert("RunAtLoad".to_owned(), plist::Value::Boolean(true));
    dictionary.insert("KeepAlive".to_owned(), plist::Value::Boolean(true));

    let mut bytes = Vec::new();
    plist::Value::Dictionary(dictionary)
        .to_writer_xml(&mut bytes)
        .context("render launchd plist")?;
    let mut contents = String::from_utf8(bytes).context("launchd plist is not UTF-8")?;
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchd_plist_escapes_program_arguments() {
        let exe = Path::new("/Applications/Thread & <Agent> 100%/threadbase\"x\"");
        let contents = launchd_plist(exe).unwrap();
        assert!(contents.contains("&amp;"), "{contents}");
        assert!(contents.contains("&lt;"), "{contents}");
        assert!(contents.contains("100%"), "{contents}");

        let value = plist::Value::from_reader(std::io::Cursor::new(contents.as_bytes())).unwrap();
        let dictionary = value.as_dictionary().unwrap();
        let arguments = dictionary
            .get("ProgramArguments")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            arguments[0].as_string(),
            Some(exe.to_string_lossy().as_ref())
        );
        assert_eq!(arguments[1].as_string(), Some("serve"));
        assert_eq!(
            dictionary.get("RunAtLoad").unwrap().as_boolean(),
            Some(true)
        );
        assert_eq!(
            dictionary.get("KeepAlive").unwrap().as_boolean(),
            Some(true)
        );
    }
}
