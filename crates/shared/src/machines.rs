//! Machine-readable lifecycle catalogue.
//!
//! The transition functions in `threadbase-core` are authoritative. They
//! publish this description, so everyone reads the same model:
//!
//! - the web client decides which controls to render,
//! - `threadbase machines` and `GET /api/machines` let a person or an agent
//!   ask what can happen without grepping three languages,
//! - the catalogue is exported to `web/src/api/state-machines.json` by a test,
//!   so drift is a failing test rather than a stale comment.
//!
//! `commands` are the actions a caller may choose. Facts (adapter reports such
//! as `turn_ended`) are deliberately excluded: nobody can invoke them, so they
//! must never appear as a button.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MachineSpec {
    /// Stable id, also the `machine` argument of `MachineSpec::allows`.
    pub id: String,
    pub title: String,
    /// What the machine governs, in one sentence.
    pub summary: String,
    pub states: Vec<MachineState>,
    pub actions: Vec<MachineAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MachineState {
    /// Wire value persisted in SQLite and sent over the API.
    pub id: String,
    /// Human label for menus, banners and tooltips.
    pub label: String,
    /// No outgoing transitions: the entity is finished.
    pub terminal: bool,
    /// User-invocable actions legal from this state, in display order.
    pub commands: Vec<MachineCommand>,
}

/// One command and the state it lands on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MachineCommand {
    pub id: String,
    /// State after the command. Believed by the same test that checks `id`
    /// against the transition functions, so the graph cannot drift.
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MachineAction {
    pub id: String,
    pub label: String,
    pub kind: ActionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActionKind {
    /// A caller may invoke it; it can appear as a control.
    Command,
    /// The world reports it; it is never user-invocable.
    Fact,
}

impl MachineSpec {
    /// True when `action` is legal in `state`. Unknown ids are rejected, so a
    /// typo becomes a missing control instead of a 409 at click time.
    #[must_use]
    pub fn allows(&self, state: &str, action: &str) -> bool {
        self.states
            .iter()
            .find(|candidate| candidate.id == state)
            .is_some_and(|candidate| {
                candidate
                    .commands
                    .iter()
                    .any(|command| command.id == action)
            })
    }

    /// Commands legal in `state`; empty for unknown states.
    #[must_use]
    pub fn commands(&self, state: &str) -> &[MachineCommand] {
        self.states
            .iter()
            .find(|candidate| candidate.id == state)
            .map_or(&[], |candidate| candidate.commands.as_slice())
    }

    /// State reached by `action`, or `None` when it is not legal here.
    #[must_use]
    pub fn next(&self, state: &str, action: &str) -> Option<&str> {
        self.commands(state)
            .iter()
            .find(|command| command.id == action)
            .map(|command| command.to.as_str())
    }

    /// Look one action up by id.
    #[must_use]
    pub fn action(&self, action: &str) -> Option<&MachineAction> {
        self.actions.iter().find(|candidate| candidate.id == action)
    }
}

/// Look a machine up by id in a published catalogue.
#[must_use]
pub fn find<'a>(catalogue: &'a [MachineSpec], id: &str) -> Option<&'a MachineSpec> {
    catalogue.iter().find(|machine| machine.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> MachineSpec {
        MachineSpec {
            id: "demo".to_owned(),
            title: "Demo".to_owned(),
            summary: "test fixture".to_owned(),
            states: vec![MachineState {
                id: "idle".to_owned(),
                label: "Idle".to_owned(),
                terminal: false,
                commands: vec![MachineCommand {
                    id: "prompt".to_owned(),
                    to: "running".to_owned(),
                }],
            }],
            actions: vec![MachineAction {
                id: "prompt".to_owned(),
                label: "Send a prompt".to_owned(),
                kind: ActionKind::Command,
            }],
        }
    }

    #[test]
    fn allows_only_declared_commands() {
        let spec = spec();
        assert!(spec.allows("idle", "prompt"));
        assert!(!spec.allows("idle", "cancel"));
        assert!(!spec.allows("missing", "prompt"));
        assert_eq!(spec.next("idle", "prompt"), Some("running"));
        assert_eq!(spec.next("idle", "cancel"), None);
        assert_eq!(spec.commands("idle")[0].id, "prompt");
        assert!(spec.commands("missing").is_empty());
    }

    #[test]
    fn lookup_by_id() {
        let catalogue = vec![spec()];
        assert_eq!(find(&catalogue, "demo").unwrap().title, "Demo");
        assert!(find(&catalogue, "nope").is_none());
    }
}
