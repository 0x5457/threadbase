//! Shared identifiers and wire types used by the server, the domain layer and the web client.

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;
use ulid::Ulid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS,
        )]
        #[serde(transparent)]
        #[ts(export)]
        pub struct $name(pub String);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Ulid::new().to_string())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

id_type!(UserId);
id_type!(ProjectId);
id_type!(ThreadId);
id_type!(TurnId);
id_type!(MessageId);
id_type!(ToolCallId);
id_type!(SubagentId);
id_type!(ApprovalId);
id_type!(ElicitationId);
id_type!(CommentId);
id_type!(QueueItemId);
id_type!(InviteId);
id_type!(AttachmentId);
id_type!(DraftId);
id_type!(TerminalId);
id_type!(NotificationId);
