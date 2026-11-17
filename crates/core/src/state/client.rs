use threadbase_shared::ActionKind;
use threadbase_shared::{ConnectionPhase, DraftPhase, TerminalPhase};

use super::{illegal, Actions, States, TransitionError};

// ---------------------------------------------------------------------------
// Browser connection (live thread socket)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionAction {
    Connect,
    /// Socket opened; the first events page has not landed yet.
    Opened,
    /// First events page landed.
    Ready,
    Lost,
    Fail,
    Retry,
    Close,
}

pub struct ConnectionMachine;

impl ConnectionMachine {
    pub const ENTITY: &'static str = "connection";

    pub fn transition(
        current: ConnectionPhase,
        action: &ConnectionAction,
    ) -> Result<ConnectionPhase, TransitionError> {
        let next = match action {
            ConnectionAction::Connect => match current {
                ConnectionPhase::Idle | ConnectionPhase::Reconnecting | ConnectionPhase::Failed => {
                    ConnectionPhase::Loading
                }
                _ => return Err(illegal(Self::ENTITY, current, "connect")),
            },
            ConnectionAction::Opened => match current {
                ConnectionPhase::Loading => ConnectionPhase::Loading,
                _ => return Err(illegal(Self::ENTITY, current, "opened")),
            },
            ConnectionAction::Ready => match current {
                ConnectionPhase::Loading | ConnectionPhase::Reconnecting => ConnectionPhase::Live,
                ConnectionPhase::Live => ConnectionPhase::Live,
                _ => return Err(illegal(Self::ENTITY, current, "ready")),
            },
            ConnectionAction::Lost => match current {
                ConnectionPhase::Live | ConnectionPhase::Loading => ConnectionPhase::Reconnecting,
                _ => return Err(illegal(Self::ENTITY, current, "lost")),
            },
            ConnectionAction::Fail => ConnectionPhase::Failed,
            ConnectionAction::Retry => match current {
                ConnectionPhase::Failed | ConnectionPhase::Reconnecting => ConnectionPhase::Loading,
                _ => return Err(illegal(Self::ENTITY, current, "retry")),
            },
            ConnectionAction::Close => ConnectionPhase::Idle,
        };
        Ok(next)
    }
}

// ---------------------------------------------------------------------------
// Shared draft (y-websocket provider)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftAction {
    Connect,
    Connected,
    Synced,
    Lost,
    Close,
    Retry,
}

pub struct DraftMachine;

impl DraftMachine {
    pub const ENTITY: &'static str = "draft";

    pub fn transition(
        current: DraftPhase,
        action: &DraftAction,
    ) -> Result<DraftPhase, TransitionError> {
        let next = match action {
            DraftAction::Connect => match current {
                DraftPhase::Idle | DraftPhase::Offline => DraftPhase::Connecting,
                _ => return Err(illegal(Self::ENTITY, current, "connect")),
            },
            DraftAction::Connected => match current {
                DraftPhase::Connecting => DraftPhase::Syncing,
                _ => return Err(illegal(Self::ENTITY, current, "connected")),
            },
            DraftAction::Synced => match current {
                DraftPhase::Connecting | DraftPhase::Syncing | DraftPhase::Live => DraftPhase::Live,
                _ => return Err(illegal(Self::ENTITY, current, "synced")),
            },
            DraftAction::Lost => match current {
                DraftPhase::Connecting | DraftPhase::Syncing | DraftPhase::Live => {
                    DraftPhase::Offline
                }
                _ => return Err(illegal(Self::ENTITY, current, "lost")),
            },
            DraftAction::Close => DraftPhase::Idle,
            DraftAction::Retry => match current {
                DraftPhase::Offline => DraftPhase::Connecting,
                _ => return Err(illegal(Self::ENTITY, current, "retry")),
            },
        };
        Ok(next)
    }
}

// ---------------------------------------------------------------------------
// Terminal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalAction {
    Connect,
    Attached,
    Exit,
    Fail,
    Reconnect,
    /// Take the keyboard. Illegal unless the viewer is the controller.
    Drive,
}

pub struct TerminalMachine;

impl TerminalMachine {
    pub const ENTITY: &'static str = "terminal";

    pub fn transition(
        current: TerminalPhase,
        action: &TerminalAction,
    ) -> Result<TerminalPhase, TransitionError> {
        let next = match action {
            TerminalAction::Connect => match current {
                TerminalPhase::Connecting => TerminalPhase::Connecting,
                TerminalPhase::Exited | TerminalPhase::Error => TerminalPhase::Connecting,
                TerminalPhase::Live => {
                    return Err(illegal(Self::ENTITY, current, "connect"));
                }
            },
            TerminalAction::Attached => match current {
                TerminalPhase::Connecting => TerminalPhase::Live,
                _ => return Err(illegal(Self::ENTITY, current, "attached")),
            },
            TerminalAction::Exit => match current {
                TerminalPhase::Live => TerminalPhase::Exited,
                _ => return Err(illegal(Self::ENTITY, current, "exit")),
            },
            TerminalAction::Fail => match current {
                TerminalPhase::Connecting | TerminalPhase::Live => TerminalPhase::Error,
                _ => return Err(illegal(Self::ENTITY, current, "fail")),
            },
            TerminalAction::Reconnect => match current {
                TerminalPhase::Exited | TerminalPhase::Error => TerminalPhase::Connecting,
                _ => return Err(illegal(Self::ENTITY, current, "reconnect")),
            },
            TerminalAction::Drive => match current {
                TerminalPhase::Live => TerminalPhase::Live,
                _ => return Err(illegal(Self::ENTITY, current, "drive")),
            },
        };
        Ok(next)
    }
}

impl States for ConnectionPhase {
    fn all() -> &'static [Self] {
        &[
            Self::Idle,
            Self::Loading,
            Self::Live,
            Self::Reconnecting,
            Self::Failed,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Loading => "Loading",
            Self::Live => "Live",
            Self::Reconnecting => "Reconnecting",
            Self::Failed => "Failed",
        }
    }
}

impl Actions for ConnectionAction {
    type State = ConnectionPhase;

    const ALL: &'static [Self] = &[
        Self::Connect,
        Self::Opened,
        Self::Ready,
        Self::Lost,
        Self::Fail,
        Self::Retry,
        Self::Close,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Opened => "opened",
            Self::Ready => "ready",
            Self::Lost => "lost",
            Self::Fail => "fail",
            Self::Retry => "retry",
            Self::Close => "close",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Connect => "Connect",
            Self::Opened => "Socket open",
            Self::Ready => "Live",
            Self::Lost => "Connection lost",
            Self::Fail => "Failed",
            Self::Retry => "Reconnect",
            Self::Close => "Closed",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Retry => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    fn commands(state: ConnectionPhase) -> &'static [Self] {
        match state {
            ConnectionPhase::Failed | ConnectionPhase::Reconnecting => &[Self::Retry],
            ConnectionPhase::Idle | ConnectionPhase::Loading | ConnectionPhase::Live => &[],
        }
    }
}

impl States for DraftPhase {
    fn all() -> &'static [Self] {
        &[
            Self::Idle,
            Self::Connecting,
            Self::Syncing,
            Self::Live,
            Self::Offline,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Connecting => "Connecting",
            Self::Syncing => "Syncing",
            Self::Live => "Live",
            Self::Offline => "Offline",
        }
    }
}

impl Actions for DraftAction {
    type State = DraftPhase;

    const ALL: &'static [Self] = &[
        Self::Connect,
        Self::Connected,
        Self::Synced,
        Self::Lost,
        Self::Close,
        Self::Retry,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Connected => "connected",
            Self::Synced => "synced",
            Self::Lost => "lost",
            Self::Close => "close",
            Self::Retry => "retry",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Connect => "Connect",
            Self::Connected => "Connected",
            Self::Synced => "Synced",
            Self::Lost => "Connection lost",
            Self::Close => "Closed",
            Self::Retry => "Reconnect",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Retry => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    fn commands(state: DraftPhase) -> &'static [Self] {
        match state {
            DraftPhase::Offline => &[Self::Retry],
            DraftPhase::Idle | DraftPhase::Connecting | DraftPhase::Syncing | DraftPhase::Live => {
                &[]
            }
        }
    }
}

impl States for TerminalPhase {
    fn all() -> &'static [Self] {
        &[Self::Connecting, Self::Live, Self::Exited, Self::Error]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Live => "Live",
            Self::Exited => "Exited",
            Self::Error => "Error",
        }
    }

    fn terminal(self) -> bool {
        matches!(self, Self::Exited | Self::Error)
    }
}

impl Actions for TerminalAction {
    type State = TerminalPhase;

    const ALL: &'static [Self] = &[
        Self::Connect,
        Self::Attached,
        Self::Exit,
        Self::Fail,
        Self::Reconnect,
        Self::Drive,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Attached => "attached",
            Self::Exit => "exit",
            Self::Fail => "fail",
            Self::Reconnect => "reconnect",
            Self::Drive => "drive",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Connect => "Connect",
            Self::Attached => "Attached",
            Self::Exit => "Terminal exited",
            Self::Fail => "Failed",
            Self::Reconnect => "Reconnect",
            Self::Drive => "Take control",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Reconnect | Self::Drive => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    fn commands(state: TerminalPhase) -> &'static [Self] {
        match state {
            TerminalPhase::Connecting => &[],
            TerminalPhase::Live => &[Self::Drive],
            TerminalPhase::Exited | TerminalPhase::Error => &[Self::Reconnect],
        }
    }
}
