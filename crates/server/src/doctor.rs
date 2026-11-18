//! `threadbase doctor` environment diagnostics.

use crate::config::Config;
use crate::install::command;
use threadbase_agent::profile::{all_profiles, find_in_path, resolve};
use threadbase_shared::{AgentInfoDto, DoctorReport};

pub async fn report(config: &Config) -> DoctorReport {
    let mut warnings = Vec::new();
    let node = command::stdout("node", &["--version"]);
    if node.is_none() {
        warnings.push("Node is not installed; npx-based adapters will not start".to_owned());
    }
    let git = command::stdout("git", &["--version"]);
    if git.is_none() {
        warnings.push("git is not installed".to_owned());
    }
    let mut npx_fallback = Vec::new();
    let mut agents = Vec::with_capacity(all_profiles().len());
    for profile in all_profiles() {
        let resolved = resolve(profile);
        let installed = if resolved.via_npx {
            find_in_path("npx").is_some()
        } else {
            find_in_path(&resolved.program).is_some()
        };
        // npx works, but every adapter start pays its resolution cost and
        // leaves an npm process resident. The doctor says so out loud instead
        // of reporting a slow fallback as healthy.
        if resolved.via_npx && installed {
            npx_fallback.push(profile.command.clone());
        }
        let adapter = profile.npm_package.clone().map(|package| {
            format!(
                "{package}@{}",
                profile.npm_version.as_deref().unwrap_or("latest")
            )
        });
        agents.push(AgentInfoDto {
            id: profile.id.to_owned(),
            label: profile.label.to_owned(),
            installed,
            version: None,
            adapter,
            adapter_version: profile.npm_version.clone(),
            supports_steering: matches!(profile.id.as_str(), "codex" | "claude"),
            supports_subagents: matches!(profile.id.as_str(), "codex" | "claude" | "opencode"),
            supports_elicitation: matches!(profile.id.as_str(), "codex" | "claude"),
            identity: None,
        });
    }
    if agents.iter().all(|agent| !agent.installed) {
        warnings.push("No agent adapter is installed".to_owned());
    }
    if !npx_fallback.is_empty() {
        warnings.push(format!(
            "Started through npx instead of a local binary: {}. Installing it on PATH saves \
             about a second per adapter start and a resident npm process",
            npx_fallback.join(", ")
        ));
    }
    // Tailscale is the only ingress and the only identity. Without it nobody can
    // sign in, so say which half is missing instead of failing at the browser.
    if !crate::tailscale::running() {
        warnings.push(
            "Tailscale is not running; `tailscale serve` cannot proxy the tailnet to this \
             server and no one can sign in"
                .to_owned(),
        );
    } else if crate::tailscale::host().is_none() {
        warnings.push(
            "Could not read this machine's tailnet DNS name from `tailscale status --json`"
                .to_owned(),
        );
    }
    if config
        .public_origin
        .as_deref()
        .is_some_and(|origin| !origin.starts_with("https://"))
    {
        warnings.push("THREADBASE_PUBLIC_ORIGIN must be an https origin".to_owned());
    }
    let database_ok = match crate::db::connect(&config.db_path).await {
        Ok(pool) => {
            let _ = sqlx::query_scalar::<_, i64>("SELECT 1")
                .fetch_one(&pool)
                .await;
            true
        }
        Err(error) => {
            warnings.push(format!("Database cannot be opened: {error}"));
            false
        }
    };
    if !database_ok {
        warnings.push("Migrations may not have run yet".to_owned());
    }
    DoctorReport {
        node,
        git,
        agents,
        data_dir: config.data_dir.display().to_string(),
        database: config.db_path.display().to_string(),
        bind: config.bind.to_string(),
        warnings,
    }
}

pub fn print(report: &DoctorReport) {
    println!("Threadbase doctor");
    println!("  bind       {}", report.bind);
    println!("  data       {}", report.data_dir);
    println!("  database   {}", report.database);
    println!(
        "  node       {}",
        report.node.as_deref().unwrap_or("MISSING")
    );
    println!(
        "  git        {}",
        report.git.as_deref().unwrap_or("MISSING")
    );
    for agent in &report.agents {
        println!(
            "  agent      {:<9} {}  {}{}",
            agent.id,
            if agent.installed { "ready " } else { "missing" },
            agent.adapter.as_deref().unwrap_or(""),
            if agent.supports_steering {
                "  steering"
            } else {
                ""
            }
        );
    }
    if report.warnings.is_empty() {
        println!("  status     ok");
    } else {
        for warning in &report.warnings {
            println!("  warning    {warning}");
        }
    }
}
