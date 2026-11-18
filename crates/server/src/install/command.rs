//! Small process helpers shared by the deployment commands.

use anyhow::{bail, Context};
use std::path::Path;
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

/// Print the tail of a log file. Both supported platforms ship `tail`, and it
/// streams instead of loading a multi-megabyte file into memory.
pub(crate) fn tail(path: &Path, lines: usize) -> anyhow::Result<()> {
    run_checked("tail", &["-n", &lines.to_string(), &path.to_string_lossy()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_commands_have_no_output() {
        assert!(stdout("threadbase-command-that-does-not-exist", &[]).is_none());
    }
}
