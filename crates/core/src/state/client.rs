use threadbase_shared::ActionKind;
use threadbase_shared::{ConnectionPhase, DraftPhase, TerminalPhase};

use super::{illegal, TransitionError};

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

    pub fn commands(state: ConnectionPhase) -> &'static [ConnectionAction] {
        match state {
            ConnectionPhase::Failed | ConnectionPhase::Reconnecting => &[ConnectionAction::Retry],
            _ => &[],
        }
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

    pub fn commands(state: DraftPhase) -> &'static [DraftAction] {
        match state {
            DraftPhase::Offline => &[DraftAction::Retry],
            _ => &[],
        }
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

    pub fn commands(state: TerminalPhase) -> &'static [TerminalAction] {
        match state {
            TerminalPhase::Connecting => &[],
            TerminalPhase::Live => &[TerminalAction::Drive],
            TerminalPhase::Exited | TerminalPhase::Error => &[TerminalAction::Reconnect],
        }
    }
}

impl ConnectionAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
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

    #[must_use]
    pub fn label(&self) -> &'static str {
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

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Retry => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}

impl DraftAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Connected => "connected",
            Self::Synced => "synced",
            Self::Lost => "lost",
            Self::Close => "close",
            Self::Retry => "retry",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Connect => "Connect",
            Self::Connected => "Connected",
            Self::Synced => "Synced",
            Self::Lost => "Connection lost",
            Self::Close => "Closed",
            Self::Retry => "Reconnect",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Retry => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}

impl TerminalAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Attached => "attached",
            Self::Exit => "exit",
            Self::Fail => "fail",
            Self::Reconnect => "reconnect",
            Self::Drive => "drive",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Connect => "Connect",
            Self::Attached => "Attached",
            Self::Exit => "Terminal exited",
            Self::Fail => "Failed",
            Self::Reconnect => "Reconnect",
            Self::Drive => "Take control",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Reconnect | Self::Drive => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}
