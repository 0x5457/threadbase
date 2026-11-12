//! Loopback health polling after the service starts.

use anyhow::bail;
use std::time::Duration;

pub(crate) async fn check(port: u16) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    for attempt in 0..20 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                println!("Service is healthy on {url}");
                return Ok(());
            }
            Ok(response) => anyhow::bail!("{url} returned {}", response.status()),
            Err(_) => continue,
        }
    }
    bail!("service did not answer on {url}; check `journalctl --user -u` output")
}
