mod tests {
    use crate::state::*;
    use threadbase_shared::{
        ApprovalStatus, CommentStatus, ElicitationStatus, QueueStatus, SubagentStatus, ThreadStatus,
    };

    // ----- thread -----

    #[test]
    fn thread_create_is_preparing() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::Create).unwrap(),
            ThreadStatus::Preparing
        );
    }

    #[test]
    fn thread_prompt_from_recoverable_states() {
        for current in [
            ThreadStatus::Idle,
            ThreadStatus::Running,
            ThreadStatus::RequiresAction,
            ThreadStatus::Error,
            ThreadStatus::Interrupted,
        ] {
            assert_eq!(
                ThreadMachine::transition(current, &ThreadAction::Prompt).unwrap(),
                ThreadStatus::Running
            );
        }
    }

    #[test]
    fn thread_prompt_blocked_when_preparing_or_archived() {
        assert!(ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::Prompt).is_err());
        assert!(ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Prompt).is_err());
    }

    #[test]
    fn thread_pending_only_from_active_states() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::ApprovalPending)
                .unwrap(),
            ThreadStatus::RequiresAction
        );
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ElicitationPending
            )
            .unwrap(),
            ThreadStatus::RequiresAction
        );
        assert!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::ApprovalPending)
                .is_err()
        );
        assert!(ThreadMachine::transition(
            ThreadStatus::Archived,
            &ThreadAction::ElicitationPending
        )
        .is_err());
    }

    #[test]
    fn thread_resolved_restores_running_or_idle() {
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ApprovalResolved {
                    turn_active: true,
                    remaining_pending: 0
                }
            )
            .unwrap(),
            ThreadStatus::Running
        );
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ApprovalResolved {
                    turn_active: false,
                    remaining_pending: 0
                }
            )
            .unwrap(),
            ThreadStatus::Idle
        );
    }

    #[test]
    fn thread_resolved_keeps_waiting_when_pending_remains() {
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ElicitationResolved {
                    turn_active: true,
                    remaining_pending: 2
                }
            )
            .unwrap(),
            ThreadStatus::RequiresAction
        );
    }

    #[test]
    fn thread_archived_is_frozen() {
        for action in [
            ThreadAction::Prompt,
            ThreadAction::ApprovalPending,
            ThreadAction::TurnEnded,
            ThreadAction::TurnError,
            ThreadAction::ProcessExited,
            ThreadAction::AgentFailed,
            ThreadAction::Cancel,
            ThreadAction::Restart,
        ] {
            assert!(
                ThreadMachine::transition(ThreadStatus::Archived, &action).is_err(),
                "archived must reject {action:?}"
            );
        }
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Archive).unwrap(),
            ThreadStatus::Archived
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Unarchive).unwrap(),
            ThreadStatus::Idle
        );
    }

    #[test]
    fn thread_cancel_clears_waiting() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::RequiresAction, &ThreadAction::Cancel).unwrap(),
            ThreadStatus::Idle
        );
        assert!(ThreadMachine::check_cancel_allowed(ThreadStatus::Error).is_err());
    }

    #[test]
    fn thread_restart_and_start() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Error, &ThreadAction::Restart).unwrap(),
            ThreadStatus::Preparing
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::StartSucceeded)
                .unwrap(),
            ThreadStatus::Idle
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::StartFailed).unwrap(),
            ThreadStatus::Error
        );
        assert!(
            ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::StartSucceeded).is_err()
        );
    }

    #[test]
    fn thread_turn_terminal_states() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::TurnEnded).unwrap(),
            ThreadStatus::Idle
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::TurnError).unwrap(),
            ThreadStatus::Error
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::ProcessExited).unwrap(),
            ThreadStatus::Interrupted
        );
        assert!(ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::TurnError).is_ok());
        assert!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::TurnEnded).is_err()
        );
    }

    #[test]
    fn thread_agent_failure_marks_error() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::AgentFailed).unwrap(),
            ThreadStatus::Error
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::AgentFailed).unwrap(),
            ThreadStatus::Error
        );
        assert!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::AgentFailed).is_err()
        );
    }

    // ----- approval -----

    #[test]
    fn approval_create_and_decide() {
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Create).unwrap(),
            ApprovalStatus::Pending
        );
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Allow).unwrap(),
            ApprovalStatus::Allowed
        );
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Deny).unwrap(),
            ApprovalStatus::Denied
        );
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Cancel).unwrap(),
            ApprovalStatus::Cancelled
        );
    }

    #[test]
    fn approval_decided_is_terminal() {
        for current in [
            ApprovalStatus::Allowed,
            ApprovalStatus::Denied,
            ApprovalStatus::Cancelled,
        ] {
            for action in [
                ApprovalAction::Allow,
                ApprovalAction::Deny,
                ApprovalAction::Cancel,
            ] {
                assert!(
                    ApprovalMachine::transition(current, &action).is_err(),
                    "{current:?} must reject {action:?}"
                );
            }
        }
    }

    // ----- elicitation -----

    #[test]
    fn elicitation_create_and_respond() {
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Create)
                .unwrap(),
            ElicitationStatus::Pending
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Accept)
                .unwrap(),
            ElicitationStatus::Accepted
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Decline)
                .unwrap(),
            ElicitationStatus::Declined
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Cancel)
                .unwrap(),
            ElicitationStatus::Cancelled
        );
    }

    #[test]
    fn elicitation_resolved_is_terminal() {
        for current in [
            ElicitationStatus::Accepted,
            ElicitationStatus::Declined,
            ElicitationStatus::Cancelled,
        ] {
            assert!(ElicitationMachine::transition(current, &ElicitationAction::Accept).is_err());
        }
    }

    // ----- subagent -----

    #[test]
    fn subagent_lifecycle() {
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Create).unwrap(),
            SubagentStatus::Idle
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Run).unwrap(),
            SubagentStatus::Running
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Idle).unwrap(),
            SubagentStatus::Idle
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::RequireAction)
                .unwrap(),
            SubagentStatus::RequiresAction
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::RequiresAction, &SubagentAction::Resolve)
                .unwrap(),
            SubagentStatus::Running
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Fail).unwrap(),
            SubagentStatus::Error
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Cancel).unwrap(),
            SubagentStatus::Cancelled
        );
    }

    #[test]
    fn subagent_terminal_and_guards() {
        assert!(SubagentMachine::is_terminal(SubagentStatus::Error));
        assert!(SubagentMachine::is_terminal(SubagentStatus::Cancelled));
        assert!(SubagentMachine::transition(SubagentStatus::Error, &SubagentAction::Run).is_err());
        assert!(
            SubagentMachine::transition(SubagentStatus::Cancelled, &SubagentAction::Run).is_err()
        );
        assert!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Resolve).is_err()
        );
        assert!(SubagentMachine::can_cancel(SubagentStatus::Running));
        assert!(!SubagentMachine::can_cancel(SubagentStatus::Error));
    }

    // ----- queue -----

    #[test]
    fn queue_lifecycle() {
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Enqueue).unwrap(),
            QueueStatus::Queued
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Dispatch).unwrap(),
            QueueStatus::Sending
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Sending, &QueueAction::PromptSucceeded).unwrap(),
            QueueStatus::Sent
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Sending, &QueueAction::PromptFailed).unwrap(),
            QueueStatus::Dropped
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Sending, &QueueAction::Release).unwrap(),
            QueueStatus::Queued
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Supersede).unwrap(),
            QueueStatus::Superseded
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Delete).unwrap(),
            QueueStatus::Dropped
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Dropped, &QueueAction::Restore).unwrap(),
            QueueStatus::Queued
        );
    }

    #[test]
    fn queue_terminal_and_guards() {
        assert!(QueueMachine::is_terminal(QueueStatus::Sent));
        assert!(QueueMachine::is_terminal(QueueStatus::Dropped));
        assert!(!QueueMachine::is_terminal(QueueStatus::Superseded));
        assert!(QueueMachine::transition(QueueStatus::Sent, &QueueAction::Dispatch).is_err());
        assert!(QueueMachine::transition(QueueStatus::Superseded, &QueueAction::Delete).is_err());
        assert!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::PromptSucceeded).is_err()
        );
    }

    // ----- agent session -----

    #[test]
    fn comment_lifecycle() {
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::CreatePending)
                .unwrap(),
            CommentStatus::Pending
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::CreateDelivered)
                .unwrap(),
            CommentStatus::Delivered
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::Deliver).unwrap(),
            CommentStatus::Delivered
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Delivered, &CommentAction::Resolve).unwrap(),
            CommentStatus::Resolved
        );
    }

    #[test]
    fn comment_guards() {
        assert!(
            CommentMachine::transition(CommentStatus::Resolved, &CommentAction::Resolve).is_err()
        );
        assert!(
            CommentMachine::transition(CommentStatus::Delivered, &CommentAction::Deliver).is_err()
        );
    }

    #[test]
    fn subagent_idempotent_actions() {
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Idle).unwrap(),
            SubagentStatus::Idle
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Run).unwrap(),
            SubagentStatus::Running
        );
        assert!(SubagentMachine::transition(
            SubagentStatus::RequiresAction,
            &SubagentAction::RequireAction
        )
        .is_err());
    }

    #[test]
    fn queue_and_comment_idempotent_actions() {
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Enqueue).unwrap(),
            QueueStatus::Queued
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::CreatePending)
                .unwrap(),
            CommentStatus::Pending
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Delivered, &CommentAction::CreateDelivered)
                .unwrap(),
            CommentStatus::Delivered
        );
        assert!(
            CommentMachine::transition(CommentStatus::Resolved, &CommentAction::Resolve).is_err()
        );
    }

    #[test]
    fn thread_idempotent_actions() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::Create).unwrap(),
            ThreadStatus::Preparing
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Archive).unwrap(),
            ThreadStatus::Archived
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::RequiresAction, &ThreadAction::ApprovalPending)
                .unwrap(),
            ThreadStatus::RequiresAction
        );
    }

    #[test]
    fn approval_idempotent_actions() {
        // Create is idempotent: it always yields Pending.
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Create).unwrap(),
            ApprovalStatus::Pending
        );
        // Decisions are terminal: repeating them is rejected.
        assert!(
            ApprovalMachine::transition(ApprovalStatus::Allowed, &ApprovalAction::Allow).is_err()
        );
    }

    #[test]
    fn elicitation_idempotent_actions() {
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Create)
                .unwrap(),
            ElicitationStatus::Pending
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Cancel)
                .unwrap(),
            ElicitationStatus::Cancelled
        );
    }

    #[test]
    fn read_rules() {
        assert!(ReadRules::inbox_unread(None));
        assert!(!ReadRules::inbox_unread(Some(1)));
        assert!(ReadRules::seen(Some("abc"), "abc"));
        assert!(!ReadRules::seen(Some("abc"), "def"));
        assert!(!ReadRules::seen(None, "abc"));
    }
}

mod string_mapping_tests {
    use crate::state::*;
    use threadbase_shared::{ApprovalDecision, Delivery, ThreadStatus};

    #[test]
    fn persisted_status_strings_round_trip() {
        for status in [
            ThreadStatus::Preparing,
            ThreadStatus::Idle,
            ThreadStatus::Running,
            ThreadStatus::RequiresAction,
            ThreadStatus::Interrupted,
            ThreadStatus::Error,
            ThreadStatus::Archived,
        ] {
            assert_eq!(parse_thread_status(thread_status_str(status)), Some(status));
        }
        assert_eq!(
            thread_status_str(ThreadStatus::RequiresAction),
            "requires_action"
        );
        assert_eq!(parse_thread_status("unknown"), None);
        assert_eq!(delivery_str(Delivery::StoppedAndSent), "stopped_and_sent");
        assert_eq!(parse_delivery("unknown"), None);
        assert_eq!(
            approval_decision_str(ApprovalDecision::AllowOnce),
            "allow_once"
        );
    }
}

mod machine_tests {
    use crate::state::*;
    use threadbase_shared::{
        ActionKind, ApprovalStatus, AuthSessionState, CommentStatus, ConnectionPhase, DraftPhase,
        ElicitationStatus, MachineSpec, PushStatus, QueueStatus, SetupStatus, SubagentStatus,
        TerminalPhase, ThreadStatus, TurnState, WatchState,
    };

    fn machine<'a>(catalogue: &'a [MachineSpec], id: &str) -> &'a MachineSpec {
        catalogue
            .iter()
            .find(|machine| machine.id == id)
            .unwrap_or_else(|| panic!("missing machine {id}"))
    }

    /// The catalogue is what the web client and `threadbase machines` read, so
    /// its per-state command lists and targets must be exactly the legal ones.
    /// Labels and summary text may differ; legality and targets may not.
    #[test]
    fn catalogue_commands_match_the_machines() {
        macro_rules! check {
            ($machine:expr, $state:expr, $commands:expr, $next:expr) => {{
                let expected: Vec<(String, String)> = $commands
                    .iter()
                    .map(|action| (action.id().to_owned(), $next(action).to_owned()))
                    .collect();
                let actual: Vec<(String, String)> = $machine
                    .commands($state)
                    .iter()
                    .map(|command| (command.id.clone(), command.to.clone()))
                    .collect();
                assert_eq!(actual, expected, "{}/{}", $machine.id, $state);
            }};
        }

        let catalogue = catalogue();
        let thread = machine(&catalogue, "thread");
        for status in [
            ThreadStatus::Preparing,
            ThreadStatus::Idle,
            ThreadStatus::Running,
            ThreadStatus::RequiresAction,
            ThreadStatus::Interrupted,
            ThreadStatus::Error,
            ThreadStatus::Archived,
        ] {
            check!(
                thread,
                thread_status_str(status),
                ThreadAction::commands(status),
                |action| thread_status_str(
                    ThreadMachine::transition(status, action).expect("legal command")
                )
            );
            let expected: Vec<String> = ThreadAction::commands(status)
                .iter()
                .map(|action| action.id().to_owned())
                .collect();
            assert_eq!(thread_commands(status), expected);
        }

        let setup = machine(&catalogue, "setup");
        for status in [
            SetupStatus::None,
            SetupStatus::Pending,
            SetupStatus::Running,
            SetupStatus::Succeeded,
            SetupStatus::Failed,
        ] {
            check!(
                setup,
                setup_status_str(status),
                SetupMachine::commands(status),
                |action| setup_status_str(
                    SetupMachine::transition(status, action).expect("legal command")
                )
            );
        }

        let approval = machine(&catalogue, "approval");
        for status in [
            ApprovalStatus::Pending,
            ApprovalStatus::Allowed,
            ApprovalStatus::Denied,
            ApprovalStatus::Cancelled,
        ] {
            check!(
                approval,
                approval_status_str(status),
                ApprovalAction::commands(status),
                |action| approval_status_str(
                    ApprovalMachine::transition(status, action).expect("legal command")
                )
            );
        }

        let elicitation = machine(&catalogue, "elicitation");
        for status in [
            ElicitationStatus::Pending,
            ElicitationStatus::Accepted,
            ElicitationStatus::Declined,
            ElicitationStatus::Cancelled,
        ] {
            check!(
                elicitation,
                elicitation_status_str(status),
                ElicitationAction::commands(status),
                |action| elicitation_status_str(
                    ElicitationMachine::transition(status, action).expect("legal command")
                )
            );
        }

        let subagent = machine(&catalogue, "subagent");
        for status in [
            SubagentStatus::Idle,
            SubagentStatus::Running,
            SubagentStatus::RequiresAction,
            SubagentStatus::Cancelled,
            SubagentStatus::Error,
        ] {
            check!(
                subagent,
                subagent_status_str(status),
                SubagentAction::commands(status),
                |action| subagent_status_str(
                    SubagentMachine::transition(status, action).expect("legal command")
                )
            );
        }

        let queue = machine(&catalogue, "queue");
        for status in [
            QueueStatus::Queued,
            QueueStatus::Sending,
            QueueStatus::Sent,
            QueueStatus::Superseded,
            QueueStatus::Dropped,
        ] {
            check!(
                queue,
                queue_status_str(status),
                QueueAction::commands(status),
                |action| queue_status_str(
                    QueueMachine::transition(status, action).expect("legal command")
                )
            );
        }

        let comment = machine(&catalogue, "comment");
        for status in [
            CommentStatus::Pending,
            CommentStatus::Delivered,
            CommentStatus::Resolved,
        ] {
            check!(
                comment,
                comment_status_str(status),
                CommentAction::commands(status),
                |action| comment_status_str(
                    CommentMachine::transition(status, action).expect("legal command")
                )
            );
        }

        let watch = machine(&catalogue, "watch");
        for status in [
            WatchState::NotWatching,
            WatchState::Watching,
            WatchState::Muted,
        ] {
            check!(
                watch,
                status.as_str(),
                WatchMachine::commands(status),
                |action: &WatchAction| WatchMachine::transition(status, *action)
                    .expect("legal command")
                    .as_str()
            );
        }

        let turn = machine(&catalogue, "turn");
        for status in [TurnState::Empty, TurnState::Starting, TurnState::Running] {
            check!(
                turn,
                status.as_str(),
                TurnMachine::commands(status),
                |action: &TurnAction| TurnMachine::transition(status, action)
                    .expect("legal command")
                    .as_str()
            );
        }

        let session = machine(&catalogue, "auth_session");
        for status in [
            AuthSessionState::Active,
            AuthSessionState::Expired,
            AuthSessionState::Revoked,
        ] {
            check!(
                session,
                status.as_str(),
                AuthSessionMachine::commands(status),
                |action: &AuthSessionAction| AuthSessionMachine::transition(status, action)
                    .expect("legal command")
                    .as_str()
            );
        }

        let push = machine(&catalogue, "push");
        for status in [
            PushStatus::Pending,
            PushStatus::Sent,
            PushStatus::Failed,
            PushStatus::Skipped,
        ] {
            check!(
                push,
                status.as_str(),
                PushMachine::commands(status),
                |action: &PushAction| PushMachine::transition(status, action)
                    .expect("legal command")
                    .as_str()
            );
        }

        let connection = machine(&catalogue, "connection");
        for status in [
            ConnectionPhase::Idle,
            ConnectionPhase::Loading,
            ConnectionPhase::Live,
            ConnectionPhase::Reconnecting,
            ConnectionPhase::Failed,
        ] {
            check!(
                connection,
                status.as_str(),
                ConnectionMachine::commands(status),
                |action: &ConnectionAction| ConnectionMachine::transition(status, action)
                    .expect("legal command")
                    .as_str()
            );
        }

        let draft = machine(&catalogue, "draft");
        for status in [
            DraftPhase::Idle,
            DraftPhase::Connecting,
            DraftPhase::Syncing,
            DraftPhase::Live,
            DraftPhase::Offline,
        ] {
            check!(
                draft,
                status.as_str(),
                DraftMachine::commands(status),
                |action: &DraftAction| DraftMachine::transition(status, action)
                    .expect("legal command")
                    .as_str()
            );
        }

        let terminal = machine(&catalogue, "terminal");
        for status in [
            TerminalPhase::Connecting,
            TerminalPhase::Live,
            TerminalPhase::Exited,
            TerminalPhase::Error,
        ] {
            check!(
                terminal,
                status.as_str(),
                TerminalMachine::commands(status),
                |action: &TerminalAction| TerminalMachine::transition(status, action)
                    .expect("legal command")
                    .as_str()
            );
        }
    }

    /// Every catalogued command must be declared a command and must actually be
    /// legal in the state that lists it, otherwise the UI renders a button that
    /// is guaranteed to fail.
    #[test]
    fn every_catalogued_command_transitions() {
        for machine in catalogue() {
            for state in &machine.states {
                for command in &state.commands {
                    let action = machine
                        .action(&command.id)
                        .unwrap_or_else(|| panic!("{} has no action {}", machine.id, command.id));
                    assert_eq!(
                        action.kind,
                        ActionKind::Command,
                        "{}/{} is a fact",
                        machine.id,
                        command.id
                    );
                }
            }
        }

        for status in [
            ThreadStatus::Preparing,
            ThreadStatus::Idle,
            ThreadStatus::Running,
            ThreadStatus::RequiresAction,
            ThreadStatus::Interrupted,
            ThreadStatus::Error,
            ThreadStatus::Archived,
        ] {
            for action in ThreadAction::commands(status) {
                assert!(
                    ThreadMachine::transition(status, action).is_ok(),
                    "thread/{status:?} lists {action:?} but rejects it"
                );
            }
        }
        for status in [
            SetupStatus::None,
            SetupStatus::Pending,
            SetupStatus::Running,
            SetupStatus::Succeeded,
            SetupStatus::Failed,
        ] {
            for action in SetupMachine::commands(status) {
                assert!(
                    SetupMachine::transition(status, action).is_ok(),
                    "setup/{status} lists {action:?} but rejects it"
                );
            }
        }
        for action in ApprovalAction::commands(ApprovalStatus::Pending) {
            assert!(ApprovalMachine::transition(ApprovalStatus::Pending, action).is_ok());
        }
        for action in ElicitationAction::commands(ElicitationStatus::Pending) {
            assert!(ElicitationMachine::transition(ElicitationStatus::Pending, action).is_ok());
        }
        for status in [
            SubagentStatus::Idle,
            SubagentStatus::Running,
            SubagentStatus::RequiresAction,
        ] {
            for action in SubagentAction::commands(status) {
                assert!(SubagentMachine::transition(status, action).is_ok());
            }
        }
        for status in [QueueStatus::Queued, QueueStatus::Dropped] {
            for action in QueueAction::commands(status) {
                assert!(QueueMachine::transition(status, action).is_ok());
            }
        }
        for status in [CommentStatus::Pending, CommentStatus::Delivered] {
            for action in CommentAction::commands(status) {
                assert!(CommentMachine::transition(status, action).is_ok());
            }
        }
        for status in [
            WatchState::NotWatching,
            WatchState::Watching,
            WatchState::Muted,
        ] {
            for action in WatchMachine::commands(status) {
                assert!(WatchMachine::transition(status, *action).is_ok());
            }
        }
        for status in [TurnState::Empty, TurnState::Starting, TurnState::Running] {
            for action in TurnMachine::commands(status) {
                assert!(TurnMachine::transition(status, action).is_ok());
            }
        }
        for status in [
            AuthSessionState::Active,
            AuthSessionState::Expired,
            AuthSessionState::Revoked,
        ] {
            for action in AuthSessionMachine::commands(status) {
                assert!(AuthSessionMachine::transition(status, action).is_ok());
            }
        }
        for status in [
            PushStatus::Pending,
            PushStatus::Sent,
            PushStatus::Failed,
            PushStatus::Skipped,
        ] {
            for action in PushMachine::commands(status) {
                assert!(PushMachine::transition(status, action).is_ok());
            }
        }
        for status in [
            ConnectionPhase::Idle,
            ConnectionPhase::Loading,
            ConnectionPhase::Live,
            ConnectionPhase::Reconnecting,
            ConnectionPhase::Failed,
        ] {
            for action in ConnectionMachine::commands(status) {
                assert!(ConnectionMachine::transition(status, action).is_ok());
            }
        }
        for status in [
            DraftPhase::Idle,
            DraftPhase::Connecting,
            DraftPhase::Syncing,
            DraftPhase::Live,
            DraftPhase::Offline,
        ] {
            for action in DraftMachine::commands(status) {
                assert!(DraftMachine::transition(status, action).is_ok());
            }
        }
        for status in [
            TerminalPhase::Connecting,
            TerminalPhase::Live,
            TerminalPhase::Exited,
            TerminalPhase::Error,
        ] {
            for action in TerminalMachine::commands(status) {
                assert!(TerminalMachine::transition(status, action).is_ok());
            }
        }
    }

    /// A fact is never invocable: it must not be reachable as a command.
    #[test]
    fn facts_are_never_commands() {
        for machine in catalogue() {
            let command_ids: Vec<&str> = machine
                .states
                .iter()
                .flat_map(|state| state.commands.iter().map(|command| command.id.as_str()))
                .collect();
            for action in &machine.actions {
                if action.kind == ActionKind::Fact {
                    assert!(
                        !command_ids.contains(&action.id.as_str()),
                        "{}/{} is a fact but is offered as a command",
                        machine.id,
                        action.id
                    );
                }
            }
        }
    }

    #[test]
    fn machine_ids_and_action_ids_are_unique() {
        let catalogue = catalogue();
        let mut ids: Vec<&str> = catalogue
            .iter()
            .map(|machine| machine.id.as_str())
            .collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate machine ids");
        for machine in &catalogue {
            let mut actions: Vec<&str> = machine.actions.iter().map(|a| a.id.as_str()).collect();
            actions.sort_unstable();
            let before = actions.len();
            actions.dedup();
            assert_eq!(
                before,
                actions.len(),
                "{} has duplicate actions",
                machine.id
            );
        }
    }

    // ----- setup -----

    #[test]
    fn setup_runs_once_per_worktree() {
        assert_eq!(
            SetupMachine::transition(SetupStatus::None, &SetupAction::Start).unwrap(),
            SetupStatus::Pending
        );
        assert!(
            SetupMachine::transition(SetupStatus::Pending, &SetupAction::Start).is_err(),
            "a queued setup cannot start twice"
        );
        assert!(
            SetupMachine::transition(SetupStatus::Running, &SetupAction::Start).is_err(),
            "a running setup cannot start twice"
        );
        assert_eq!(
            SetupMachine::transition(SetupStatus::Pending, &SetupAction::Started).unwrap(),
            SetupStatus::Running
        );
        assert_eq!(
            SetupMachine::transition(SetupStatus::Running, &SetupAction::Finish).unwrap(),
            SetupStatus::Succeeded
        );
        assert_eq!(
            SetupMachine::transition(SetupStatus::Running, &SetupAction::Fail).unwrap(),
            SetupStatus::Failed
        );
    }

    #[test]
    fn setup_can_retry_after_success_or_failure() {
        for current in [SetupStatus::Succeeded, SetupStatus::Failed] {
            assert_eq!(
                SetupMachine::transition(current, &SetupAction::Start).unwrap(),
                SetupStatus::Pending
            );
        }
    }

    #[test]
    fn setup_interrupted_by_restart_fails_instead_of_hanging() {
        for current in [SetupStatus::Pending, SetupStatus::Running] {
            assert_eq!(
                SetupMachine::transition(current, &SetupAction::Interrupt).unwrap(),
                SetupStatus::Failed
            );
        }
        assert!(SetupMachine::transition(SetupStatus::Succeeded, &SetupAction::Interrupt).is_err());
        assert!(!SetupMachine::is_busy(SetupStatus::Failed));
        assert!(SetupMachine::is_busy(SetupStatus::Running));
    }

    // ----- watch -----

    #[test]
    fn watch_is_one_value_not_two_flags() {
        assert_eq!(
            WatchMachine::transition(WatchState::NotWatching, WatchAction::Watch).unwrap(),
            WatchState::Watching
        );
        assert_eq!(
            WatchMachine::transition(WatchState::Watching, WatchAction::Mute).unwrap(),
            WatchState::Muted
        );
        // Muting an unwatched thread used to silently start watching. The
        // machine rejects it, so the UI has to offer "Watch thread" first.
        assert!(WatchMachine::transition(WatchState::NotWatching, WatchAction::Mute).is_err());
        assert_eq!(
            WatchMachine::transition(WatchState::Muted, WatchAction::Unmute).unwrap(),
            WatchState::Watching
        );
        assert_eq!(
            WatchMachine::transition(WatchState::Watching, WatchAction::Unwatch).unwrap(),
            WatchState::NotWatching
        );
    }

    #[test]
    fn watch_target_resolves_to_the_right_command() {
        assert_eq!(
            WatchMachine::action_for(WatchState::NotWatching, WatchState::Watching).unwrap(),
            WatchAction::Watch
        );
        assert_eq!(
            WatchMachine::action_for(WatchState::Watching, WatchState::Muted).unwrap(),
            WatchAction::Mute
        );
        assert_eq!(
            WatchMachine::action_for(WatchState::Muted, WatchState::Watching).unwrap(),
            WatchAction::Unmute
        );
        assert_eq!(
            WatchMachine::action_for(WatchState::Watching, WatchState::NotWatching).unwrap(),
            WatchAction::Unwatch
        );
        assert!(WatchMachine::action_for(WatchState::NotWatching, WatchState::Muted).is_err());
    }

    // ----- turn -----

    #[test]
    fn only_one_turn_can_start() {
        assert_eq!(
            TurnMachine::transition(TurnState::Empty, &TurnAction::Claim).unwrap(),
            TurnState::Starting
        );
        assert!(TurnMachine::transition(TurnState::Starting, &TurnAction::Claim).is_err());
        assert!(TurnMachine::transition(TurnState::Running, &TurnAction::Claim).is_err());
        assert_eq!(
            TurnMachine::transition(TurnState::Starting, &TurnAction::Install).unwrap(),
            TurnState::Running
        );
        assert!(TurnMachine::transition(TurnState::Empty, &TurnAction::Install).is_err());
    }

    #[test]
    fn a_turn_that_finishes_while_starting_is_not_installed() {
        assert_eq!(
            TurnMachine::transition(TurnState::Starting, &TurnAction::Complete).unwrap(),
            TurnState::Starting,
            "the claim survives the completion so the starter notices"
        );
        assert_eq!(
            TurnMachine::transition(TurnState::Starting, &TurnAction::InstallAfterFinish).unwrap(),
            TurnState::Empty
        );
        assert_eq!(
            TurnMachine::transition(TurnState::Starting, &TurnAction::End).unwrap(),
            TurnState::Starting
        );
    }

    #[test]
    fn cancelling_a_starting_turn_is_remembered() {
        assert_eq!(
            TurnMachine::transition(TurnState::Starting, &TurnAction::Cancel).unwrap(),
            TurnState::Starting
        );
        assert_eq!(
            TurnMachine::transition(TurnState::Running, &TurnAction::Cancel).unwrap(),
            TurnState::Empty
        );
    }

    #[test]
    fn stale_running_slots_are_dropped() {
        assert_eq!(
            TurnMachine::transition(TurnState::Running, &TurnAction::Sync { running: false })
                .unwrap(),
            TurnState::Empty
        );
        assert_eq!(
            TurnMachine::transition(TurnState::Running, &TurnAction::Sync { running: true })
                .unwrap(),
            TurnState::Running
        );
    }

    // ----- auth session -----

    #[test]
    fn session_state_is_derived_from_timestamps() {
        assert_eq!(
            AuthSessionMachine::classify(None, 100, 50),
            AuthSessionState::Active
        );
        assert_eq!(
            AuthSessionMachine::classify(None, 50, 50),
            AuthSessionState::Expired
        );
        assert_eq!(
            AuthSessionMachine::classify(Some(1), 100, 50),
            AuthSessionState::Revoked
        );
    }

    #[test]
    fn a_finished_session_cannot_be_touched_or_revoked() {
        assert_eq!(
            AuthSessionMachine::transition(AuthSessionState::Active, &AuthSessionAction::Touch)
                .unwrap(),
            AuthSessionState::Active
        );
        for state in [AuthSessionState::Expired, AuthSessionState::Revoked] {
            assert!(AuthSessionMachine::transition(state, &AuthSessionAction::Touch).is_err());
            assert!(AuthSessionMachine::transition(state, &AuthSessionAction::Revoke).is_err());
        }
    }

    #[test]
    fn live_predicate_is_shared() {
        assert_eq!(
            AuthSessionMachine::live_predicate("s"),
            "s.revoked_at IS NULL AND s.expires_at > ?"
        );
    }

    // ----- push -----

    #[test]
    fn a_failed_push_retries_instead_of_reporting_success() {
        assert_eq!(
            PushMachine::transition(PushStatus::Pending, &PushAction::Fail).unwrap(),
            PushStatus::Pending,
            "a transport failure keeps the row retryable"
        );
        assert_eq!(
            PushMachine::transition(PushStatus::Pending, &PushAction::GiveUp).unwrap(),
            PushStatus::Failed
        );
        assert_eq!(
            PushMachine::transition(PushStatus::Failed, &PushAction::Retry).unwrap(),
            PushStatus::Pending
        );
        assert!(
            PushMachine::transition(PushStatus::Sent, &PushAction::Retry).is_err(),
            "delivered rows are not replayed"
        );
    }

    #[test]
    fn skipped_push_is_terminal_but_retryable() {
        assert_eq!(
            PushMachine::transition(PushStatus::Pending, &PushAction::Skip).unwrap(),
            PushStatus::Skipped
        );
        assert!(PushMachine::is_terminal(PushStatus::Skipped));
        assert_eq!(
            PushMachine::commands(PushStatus::Skipped),
            [PushAction::Retry]
        );
    }

    // ----- client machines -----

    #[test]
    fn connection_phases_only_move_forward() {
        use ConnectionAction as A;
        use ConnectionPhase as P;
        assert_eq!(
            ConnectionMachine::transition(P::Idle, &A::Connect).unwrap(),
            P::Loading
        );
        assert_eq!(
            ConnectionMachine::transition(P::Loading, &A::Ready).unwrap(),
            P::Live
        );
        assert_eq!(
            ConnectionMachine::transition(P::Live, &A::Lost).unwrap(),
            P::Reconnecting
        );
        assert_eq!(
            ConnectionMachine::transition(P::Reconnecting, &A::Connect).unwrap(),
            P::Loading
        );
        assert!(ConnectionMachine::transition(P::Live, &A::Connect).is_err());
        assert_eq!(ConnectionMachine::commands(P::Reconnecting), [A::Retry]);
    }

    #[test]
    fn draft_requires_sync_before_it_is_live() {
        use DraftAction as A;
        use DraftPhase as P;
        assert_eq!(
            DraftMachine::transition(P::Idle, &A::Connect).unwrap(),
            P::Connecting
        );
        assert_eq!(
            DraftMachine::transition(P::Connecting, &A::Connected).unwrap(),
            P::Syncing
        );
        assert_eq!(
            DraftMachine::transition(P::Syncing, &A::Synced).unwrap(),
            P::Live
        );
        assert_eq!(
            DraftMachine::transition(P::Live, &A::Lost).unwrap(),
            P::Offline
        );
        assert_eq!(
            DraftMachine::transition(P::Offline, &A::Retry).unwrap(),
            P::Connecting
        );
        assert!(DraftMachine::transition(P::Live, &A::Synced).is_ok());
    }

    #[test]
    fn terminal_drive_requires_a_live_session() {
        use TerminalAction as A;
        use TerminalPhase as P;
        assert_eq!(
            TerminalMachine::transition(P::Connecting, &A::Attached).unwrap(),
            P::Live
        );
        assert_eq!(
            TerminalMachine::transition(P::Live, &A::Drive).unwrap(),
            P::Live
        );
        assert!(TerminalMachine::transition(P::Connecting, &A::Drive).is_err());
        assert_eq!(
            TerminalMachine::transition(P::Live, &A::Exit).unwrap(),
            P::Exited
        );
        assert!(TerminalMachine::transition(P::Exited, &A::Attached).is_err());
        assert_eq!(
            TerminalMachine::commands(P::Exited),
            [A::Reconnect],
            "a dead session can only be replaced"
        );
    }

    #[test]
    fn watch_state_maps_rows_to_one_value() {
        assert_eq!(WatchState::from_row(None), WatchState::NotWatching);
        assert_eq!(WatchState::from_row(Some(0)), WatchState::Watching);
        assert_eq!(WatchState::from_row(Some(1)), WatchState::Muted);
        assert_eq!(WatchState::NotWatching.muted_value(), None);
        assert_eq!(WatchState::Watching.muted_value(), Some(false));
        assert_eq!(WatchState::Muted.muted_value(), Some(true));
    }
}
