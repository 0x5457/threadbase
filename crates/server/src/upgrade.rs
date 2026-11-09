//! `threadbase upgrade`: download the matching release binary and verify SHA256.

use anyhow::Context;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

const DEFAULT_BASE: &str = "https://github.com/threadbase/threadbase/releases/latest/download";

pub async fn run(check_only: bool) -> anyhow::Result<()> {
    let target = target_triple()?;
    let base = std::env::var("THREADBASE_RELEASE_BASE").unwrap_or_else(|_| DEFAULT_BASE.into());
    let base = base.trim_end_matches('/');
    let binary_url = format!("{base}/threadbase-{target}");
    let checksum_url = format!("{binary_url}.sha256");
    let client = reqwest::Client::builder()
        .user_agent("threadbase-upgrade")
        .build()?;
    let checksum_text = client
        .get(&checksum_url)
        .send()
        .await?
        .error_for_status()
        .context("download checksum")?
        .text()
        .await?;
    let expected = checksum_text
        .split_whitespace()
        .next()
        .context("checksum file is empty")?
        .to_lowercase();
    println!("Latest target: {target}");
    println!("SHA256: {expected}");
    if check_only {
        return Ok(());
    }
    let bytes = client
        .get(&binary_url)
        .send()
        .await?
        .error_for_status()
        .context("download binary")?
        .bytes()
        .await?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != expected {
        anyhow::bail!("checksum mismatch: expected {expected}, got {actual}");
    }
    let current = std::env::current_exe().context("resolve current binary")?;
    let temp = current.with_extension("upgrade");
    {
        let mut file =
            std::fs::File::create(&temp).with_context(|| format!("create {}", temp.display()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))?;
    }
    replace(&temp, &current)?;
    println!("Upgraded {}", current.display());
    Ok(())
}

fn replace(temp: &Path, current: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(temp, current)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::copy(temp, current)?;
        let _ = std::fs::remove_file(temp);
    }
    Ok(())
}

fn target_triple() -> anyhow::Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Ok(match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        _ => anyhow::bail!("unsupported platform {os}/{arch}"),
    }
    .to_owned())
}
