//! Generate service-manager files without installing them silently.

use crate::install::unit;
use anyhow::Context;

pub fn install(print_only: bool) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("resolve current binary")?;
    let exe = exe.display().to_string();
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").context("HOME is not set")?;
        let path =
            std::path::PathBuf::from(format!("{home}/Library/LaunchAgents/dev.threadbase.plist"));
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>dev.threadbase</string>
  <key>ProgramArguments</key><array><string>{exe}</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict></plist>
"#
        );
        if print_only {
            println!("{plist}");
        } else {
            unit::write(&path, &plist)?;
            println!("Wrote {}", path.display());
            println!("Run: launchctl load -w {}", path.display());
        }
    }
    #[cfg(target_os = "linux")]
    {
        let path = unit::path("threadbase")?;
        let contents = format!(
            "[Unit]\nDescription=Threadbase local agent workspace\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={exe} serve\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n"
        );
        if print_only {
            println!("{contents}");
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
