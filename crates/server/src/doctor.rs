//! `threadbase doctor` environment diagnostics.

use crate::config::Config;
use std::process::Command;
use threadbase_agent::profile::{all_profiles, find_in_path, resolve};
use threadbase_shared::{AgentInfoDto, DoctorReport};

pub async fn report(config: &Config) -> DoctorReport {
    let mut warnings = Vec::new();
    let node = version("node", &["--version"]);
    if node.is_none() {
        warnings.push("Node is not installed; npx-based adapters will not start".to_owned());
    }
    let git = version("git", &["--version"]);
    if git.is_none() {
        warnings.push("git is not installed".to_owned());
    }
    let agents: Vec<AgentInfoDto> = all_profiles()
        .iter()
        .map(|profile| {
            let resolved = resolve(profile);
            let installed = if resolved.via_npx {
                find_in_path("npx").is_some()
            } else {
                find_in_path(&resolved.program).is_some()
            };
            let adapter = profile
                .npm_package
                .map(|package| format!("{package}@{}", profile.npm_version.unwrap_or("latest")));
            AgentInfoDto {
                id: profile.id.to_owned(),
                label: profile.label.to_owned(),
                installed,
                version: None,
                adapter,
                adapter_version: profile.npm_version.map(ToOwned::to_owned),
                supports_steering: matches!(profile.id, "codex" | "claude"),
                supports_subagents: matches!(profile.id, "codex" | "claude" | "opencode"),
                supports_elicitation: matches!(profile.id, "codex" | "claude"),
                identity: None,
            }
        })
        .collect();
    if agents.iter().all(|agent| !agent.installed) {
        warnings.push("No agent adapter is installed".to_owned());
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

fn version(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
