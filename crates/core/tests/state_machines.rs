//! Export the lifecycle catalogue for the web client.
//!
//! `web/src/api/state-machines.json` is the same data `threadbase machines
//! --json` and `GET /api/machines` return. Regenerating it from the machines
//! (rather than hand-writing a TypeScript copy) is what keeps the UI's idea of
//! "what can I do now" honest.

use std::path::PathBuf;

#[test]
fn exports_the_catalogue_to_the_web_client() {
    let catalogue = threadbase_core::state::catalogue();
    let json = serde_json::to_string_pretty(&catalogue).expect("serialize catalogue");
    let json = format!("{json}\n");

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("web")
        .join("src")
        .join("api")
        .join("state-machines.json");

    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == json) {
        return;
    }
    std::fs::write(&path, json).expect("write web/src/api/state-machines.json");
    eprintln!("wrote {}", path.display());
}

/// Every machine the docs and UI refer to by name must be present, and no
/// state may advertise a command that is not declared as one.
#[test]
fn catalogue_is_complete() {
    let catalogue = threadbase_core::state::catalogue();
    for id in [
        "thread",
        "placement",
        "setup",
        "approval",
        "elicitation",
        "subagent",
        "queue",
        "comment",
        "watch",
        "turn",
        "auth_session",
        "push",
        "connection",
        "draft",
        "terminal",
    ] {
        assert!(
            catalogue.iter().any(|machine| machine.id == id),
            "missing machine {id}"
        );
    }
    for machine in &catalogue {
        for state in &machine.states {
            for command in &state.commands {
                let action = machine
                    .action(&command.id)
                    .unwrap_or_else(|| panic!("{} has no action {}", machine.id, command.id));
                assert_eq!(
                    action.kind,
                    threadbase_shared::ActionKind::Command,
                    "{}/{} is a fact, not a command",
                    machine.id,
                    command.id
                );
            }
        }
    }
}
