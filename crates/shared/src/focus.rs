//! Presence and semantic focus types. These are ephemeral and never persisted.

use crate::ids::{ProjectId, ThreadId, UserId};
use crate::model::Panel;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Anchor {
    Message {
        message_id: String,
    },
    Turn {
        turn_id: String,
    },
    Diff {
        file: String,
        hunk_id: String,
        line: u32,
    },
    File {
        path: String,
        line: u32,
    },
    Draft {
        block_id: String,
    },
    Project {
        project_id: ProjectId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Focus {
    pub user_id: UserId,
    pub project_id: ProjectId,
    pub thread_id: Option<ThreadId>,
    pub panel: Panel,
    pub target: Option<Anchor>,
    pub typing: bool,
    pub hidden: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresenceSnapshot {
    pub project_id: ProjectId,
    pub focuses: Vec<Focus>,
    pub online_user_ids: Vec<UserId>,
}
