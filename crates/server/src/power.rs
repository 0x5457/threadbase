//! Power assertions for a host that must stay reachable.

/// Prevents idle and explicit system sleep while it is alive.
///
/// macOS explains this as an IOKit assertion, the same mechanism `caffeinate`
/// uses. It is owned by the server process, so it cannot outlive the service
/// and needs no second launch agent.
#[cfg(target_os = "macos")]
pub struct Awake(#[allow(dead_code)] keepawake::KeepAwake);

#[cfg(target_os = "macos")]
pub fn hold(enabled: bool) -> anyhow::Result<Option<Awake>> {
    if !enabled {
        return Ok(None);
    }
    let awake = keepawake::Builder::default()
        // No `.display(true)`: the screen may sleep, the machine may not.
        .idle(true)
        .sleep(true)
        .reason("Threadbase agent sessions")
        .app_name("threadbase")
        .app_reverse_domain("dev.threadbase")
        .create()?;
    tracing::info!("holding a power assertion; the host will not idle-sleep");
    Ok(Some(Awake(awake)))
}

/// Linux hosts are not expected to suspend; logind already treats a running
/// service as a reason to stay up, so the flag only earns a warning.
#[cfg(not(target_os = "macos"))]
pub fn hold(enabled: bool) -> anyhow::Result<Option<()>> {
    if enabled {
        tracing::warn!(
            "THREADBASE_KEEP_AWAKE is macOS-only; mask sleep.target if this host suspends"
        );
    }
    Ok(None)
}
