//! `threadbase upgrade`: download the matching release binary and verify SHA256.

use anyhow::Context;
use sha2::{Digest, Sha256};

const DEFAULT_BASE: &str = "https://github.com/threadbase/threadbase/releases/latest/download";

/// Replace this binary with the published release for this platform.
pub async fn run(check_only: bool) -> anyhow::Result<()> {
    let target = target_triple()?;
    let base = std::env::var("THREADBASE_RELEASE_BASE").unwrap_or_else(|_| DEFAULT_BASE.into());
    let base = base.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .user_agent("threadbase-upgrade")
        .build()?;

    let expected = checksum(&client, &format!("{base}/threadbase-{target}.sha256")).await?;
    println!("Latest target: {target}");
    println!("SHA256: {expected}");
    if check_only {
        return Ok(());
    }

    let bytes = client
        .get(format!("{base}/threadbase-{target}"))
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
    install(&bytes).await?;
    println!("Upgraded {}", std::env::current_exe()?.display());
    Ok(())
}

/// Published digest: the first token of the `.sha256` sidecar.
async fn checksum(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    let text = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .context("download checksum")?
        .text()
        .await?;
    let digest = text
        .split_whitespace()
        .next()
        .context("checksum file is empty")?
        .to_lowercase();
    Ok(digest)
}

/// Stage the verified bytes beside the running binary and swap it in place.
/// `self_replace` keeps the old file's permissions and mode, so the result is
/// still executable, and it follows the install path when it is a symlink.
async fn install(bytes: &[u8]) -> anyhow::Result<()> {
    let current = std::env::current_exe().context("resolve current binary")?;
    let staged = current.with_extension("upgrade");
    tokio::fs::write(&staged, bytes)
        .await
        .with_context(|| format!("write {}", staged.display()))?;
    self_replace::self_replace(&staged).context("replace the running binary")?;
    let _ = std::fs::remove_file(&staged);
    Ok(())
}

fn target_triple() -> anyhow::Result<&'static str> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Ok(match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        _ => anyhow::bail!("unsupported platform {os}/{arch}"),
    })
}
