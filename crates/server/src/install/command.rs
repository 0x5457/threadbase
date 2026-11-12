//! Small process helpers shared by the deployment commands.

use anyhow::{bail, Context};
use std::process::{Command, Stdio};

/// Trimmed stdout of a successful command, for probes such as `--version` or
/// `tailscale status --json`.
pub(crate) fn stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn run_checked(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("spawn {program}"))?;
    if !status.success() {
        bail!("`{program} {}` failed", args.join(" "));
    }
    Ok(())
}

pub(crate) fn run_quiet(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("spawn {program}"))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub(crate) fn ensure_systemctl() -> anyhow::Result<()> {
    if Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    bail!(
        "`systemctl --user` is unavailable. This usually means systemd is not \
         running or XDG_RUNTIME_DIR is unset; run install from a normal login \
         session on a systemd host."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_commands_have_no_output() {
        assert!(stdout("threadbase-command-that-does-not-exist", &[]).is_none());
    }
}
