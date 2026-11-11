//! Pure event reduction used for snapshots and server-side tests.
//!
//! The web client has an equivalent reducer. Keeping this one pure makes the
//! replay semantics testable without a database or a browser.

use serde::{Deserialize, Serialize};
use threadbase_shared::model::{ApprovalDto, ChangeStats, ElicitationDto, QueueItemDto};
use threadbase_shared::{
    ApprovalStatus, CommentDto, EventEnvelope, MessageDto, MessageRole, PlanEntry, SubagentDto,
    ThreadEvent, ToolCallData, TurnId, Usage,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingBlock {
    pub message_id: String,
    pub text: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineState {
    pub last_seq: u64,
    pub title: Option<String>,
    pub pinned: bool,
    pub messages: Vec<MessageDto>,
    pub thinking: Vec<ThinkingBlock>,
    pub tools: Vec<ToolCallData>,
    pub plan: Vec<PlanEntry>,
    pub approvals: Vec<ApprovalDto>,
    pub elicitations: Vec<ElicitationDto>,
    pub subagents: Vec<SubagentDto>,
    pub queue: Vec<QueueItemDto>,
    pub comments: Vec<CommentDto>,
    pub usage: Option<Usage>,
    pub changes: Option<ChangeStats>,
    pub current_turn: Option<TurnId>,
    pub notices: Vec<String>,
}

impl TimelineState {
    /// Apply one event. Returns `false` for an already-applied sequence number.
    pub fn apply(&mut self, envelope: &EventEnvelope) -> bool {
        if envelope.seq <= self.last_seq {
            return false;
        }
        self.apply_payload(&envelope.payload, envelope.thread_id.clone(), envelope.ts);
        self.last_seq = envelope.seq;
        true
    }

    fn apply_payload(
        &mut self,
        payload: &ThreadEvent,
        thread_id: threadbase_shared::ThreadId,
        ts: i64,
    ) {
        match payload {
            ThreadEvent::TurnStarted {
                turn_id,
                message_id,
                author,
                authors,
                prompt,
            } => {
                self.current_turn = Some(turn_id.clone());
                if !self
                    .messages
                    .iter()
                    .any(|message| &message.id == message_id)
                {
                    self.messages.push(MessageDto {
                        id: message_id.clone(),
                        thread_id,
                        turn_id: Some(turn_id.clone()),
                        role: MessageRole::User,
                        author: Some(author.clone()),
                        authors: if authors.is_empty() {
                            vec![author.clone()]
                        } else {
                            authors.clone()
                        },
                        text: prompt.clone(),
                        model: None,
                        streaming: false,
                        created_at: ts,
                    });
                }
            }
            ThreadEvent::MessageDelta {
                message_id,
                role,
                text,
                author,
            } => {
                if let Some(message) = self.messages.iter_mut().find(|item| &item.id == message_id)
                {
                    message.text.push_str(text);
                    message.streaming = true;
                    if message.author.is_none() {
                        message.author.clone_from(author);
                    }
                    return;
                }
                let mut authors = Vec::new();
                if let Some(author) = author {
                    authors.push(author.clone());
                }
                self.messages.push(MessageDto {
                    id: message_id.clone(),
                    thread_id,
                    turn_id: self.current_turn.clone(),
                    role: *role,
                    author: author.clone(),
                    authors,
                    text: text.clone(),
                    model: None,
                    streaming: true,
                    created_at: ts,
                });
            }
            ThreadEvent::MessageCompleted { message } => {
                if let Some(existing) = self.messages.iter_mut().find(|item| item.id == message.id)
                {
                    *existing = message.clone();
                } else {
                    self.messages.push(message.clone());
                }
            }
            ThreadEvent::ThinkingDelta { message_id, text } => {
                if let Some(block) = self
                    .thinking
                    .iter_mut()
                    .find(|item| item.message_id == message_id.0)
                {
                    block.text.push_str(text);
                } else {
                    self.thinking.push(ThinkingBlock {
                        message_id: message_id.0.clone(),
                        text: text.clone(),
                        completed: false,
                    });
                }
            }
            ThreadEvent::ThinkingCompleted { message_id } => {
                if let Some(block) = self
                    .thinking
                    .iter_mut()
                    .find(|item| item.message_id == message_id.0)
                {
                    block.completed = true;
                }
            }
            ThreadEvent::ToolCall { call } | ThreadEvent::ToolCallUpdate { call } => {
                if let Some(existing) = self.tools.iter_mut().find(|item| item.id == call.id) {
                    *existing = call.clone();
                } else {
                    self.tools.push(call.clone());
                }
            }
            ThreadEvent::PlanUpdated { entries } => {
                self.plan.clone_from(entries);
            }
            ThreadEvent::PermissionRequested { approval } => {
                upsert_approval(&mut self.approvals, approval.clone());
            }
            ThreadEvent::PermissionResolved {
                approval_id,
                decision,
                scope,
                decided_by,
            } => {
                if let Some(approval) = self
                    .approvals
                    .iter_mut()
                    .find(|item| &item.id == approval_id)
                {
                    approval.decision = Some(*decision);
                    approval.scope = *scope;
                    approval.decided_by.clone_from(decided_by);
                    approval.status =
                        if matches!(decision, threadbase_shared::ApprovalDecision::Deny) {
                            ApprovalStatus::Denied
                        } else {
                            ApprovalStatus::Allowed
                        };
                }
            }
            ThreadEvent::PermissionCancelled { approval_id } => {
                if let Some(approval) = self
                    .approvals
                    .iter_mut()
                    .find(|item| &item.id == approval_id)
                {
                    approval.status = ApprovalStatus::Cancelled;
                    approval.decision = None;
                    approval.scope = None;
                    approval.decided_by = None;
                }
            }
            ThreadEvent::ElicitationRequested { elicitation } => {
                upsert_elicitation(&mut self.elicitations, elicitation.clone());
            }
            ThreadEvent::ElicitationResolved {
                elicitation_id,
                action,
                response,
                responded_by,
            } => {
                if let Some(elicitation) = self
                    .elicitations
                    .iter_mut()
                    .find(|item| &item.id == elicitation_id)
                {
                    elicitation.status = match action {
                        threadbase_shared::ElicitationAction::Accept => {
                            threadbase_shared::ElicitationStatus::Accepted
                        }
                        threadbase_shared::ElicitationAction::Decline => {
                            threadbase_shared::ElicitationStatus::Declined
                        }
                        threadbase_shared::ElicitationAction::Cancel => {
                            threadbase_shared::ElicitationStatus::Cancelled
                        }
                    };
                    elicitation.response.clone_from(response);
                    elicitation.responded_by.clone_from(responded_by);
                }
            }
            ThreadEvent::SubagentUpdated { subagent } => {
                upsert_subagent(&mut self.subagents, subagent.clone());
            }
            ThreadEvent::UsageUpdated { usage } => {
                self.usage = Some(usage.clone());
            }
            ThreadEvent::TurnEnded { .. } => {
                self.current_turn = None;
                for message in &mut self.messages {
                    if message.role == MessageRole::Agent {
                        message.streaming = false;
                    }
                }
            }
            ThreadEvent::TurnError { message, .. } => {
                self.current_turn = None;
                self.notices.push(message.clone());
            }
            ThreadEvent::ProcessExited { message, .. } => {
                self.current_turn = None;
                self.notices.push(message.clone());
            }
            ThreadEvent::ChangesUpdated { stats } => {
                self.changes = Some(stats.clone());
            }
            ThreadEvent::CommentAdded { comment } | ThreadEvent::CommentUpdated { comment } => {
                upsert_comment(&mut self.comments, comment.clone());
            }
            ThreadEvent::QueueUpdated { items } => {
                self.queue.clone_from(items);
            }
            ThreadEvent::ThreadUpdated { title, pinned, .. } => {
                if title.is_some() {
                    self.title.clone_from(title);
                }
                if let Some(pinned) = pinned {
                    self.pinned = *pinned;
                }
            }
            ThreadEvent::ContextCompacted { summary } => {
                self.notices.push(
                    summary
                        .clone()
                        .unwrap_or_else(|| "Context compacted".to_owned()),
                );
            }
            ThreadEvent::SystemNotice { message, .. } => {
                self.notices.push(message.clone());
            }
            ThreadEvent::MembersChanged { .. } | ThreadEvent::AgentMessage { .. } => {}
        }
    }
}

fn upsert_approval(items: &mut Vec<ApprovalDto>, value: ApprovalDto) {
    if let Some(existing) = items.iter_mut().find(|item| item.id == value.id) {
        *existing = value;
    } else {
        items.push(value);
    }
}

fn upsert_elicitation(items: &mut Vec<ElicitationDto>, value: ElicitationDto) {
    if let Some(existing) = items.iter_mut().find(|item| item.id == value.id) {
        *existing = value;
    } else {
        items.push(value);
    }
}

fn upsert_subagent(items: &mut Vec<SubagentDto>, value: SubagentDto) {
    if let Some(existing) = items.iter_mut().find(|item| item.id == value.id) {
        *existing = value;
    } else {
        items.push(value);
    }
}

fn upsert_comment(items: &mut Vec<CommentDto>, value: CommentDto) {
    if let Some(existing) = items.iter_mut().find(|item| item.id == value.id) {
        *existing = value;
    } else {
        items.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_shared::{
        now_ms, Actor, ActorKind, EventEnvelope, MessageId, Origin, ThreadEvent, ThreadId, TurnId,
        UserBrief, UserId,
    };

    fn envelope(seq: u64, payload: ThreadEvent) -> EventEnvelope {
        EventEnvelope {
            seq,
            thread_id: ThreadId::from("t1"),
            ts: now_ms(),
            actor: Actor {
                kind: ActorKind::User,
                id: Some(UserId::from("u1").0),
                name: Some("Ada".to_owned()),
            },
            origin: Origin::Local,
            subagent_id: None,
            parent_subagent_id: None,
            tool_call_id: None,
            delivery: None,
            payload,
        }
    }

    fn user() -> UserBrief {
        UserBrief {
            id: UserId::from("u1"),
            username: "ada".to_owned(),
            display_name: "Ada".to_owned(),
            avatar_url: None,
            color: "#f00".to_owned(),
        }
    }

    #[test]
    fn duplicate_sequences_are_ignored() {
        let message = MessageDto {
            id: MessageId::from("m1"),
            thread_id: ThreadId::from("t1"),
            turn_id: None,
            role: MessageRole::Agent,
            author: None,
            authors: Vec::new(),
            text: "hello".to_owned(),
            model: None,
            streaming: false,
            created_at: 1,
        };
        let event = envelope(
            1,
            ThreadEvent::MessageCompleted {
                message: message.clone(),
            },
        );
        let mut state = TimelineState::default();
        assert!(state.apply(&event));
        assert!(!state.apply(&event));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "hello");
    }

    #[test]
    fn turn_started_keeps_multiple_authors() {
        let mut state = TimelineState::default();
        let bob = UserBrief {
            id: UserId::from("u2"),
            username: "bob".to_owned(),
            display_name: "Bob".to_owned(),
            avatar_url: None,
            color: "#0f0".to_owned(),
        };
        state.apply(&envelope(
            1,
            ThreadEvent::TurnStarted {
                turn_id: TurnId::from("turn1"),
                message_id: MessageId::from("m0"),
                author: user(),
                authors: vec![user(), bob],
                prompt: "shared draft".to_owned(),
            },
        ));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].authors.len(), 2);
        assert_eq!(state.messages[0].authors[1].display_name, "Bob");
    }

    #[test]
    fn turn_started_falls_back_to_primary_author() {
        let mut state = TimelineState::default();
        state.apply(&envelope(
            1,
            ThreadEvent::TurnStarted {
                turn_id: TurnId::from("turn1"),
                message_id: MessageId::from("m0"),
                author: user(),
                authors: vec![],
                prompt: "hello".to_owned(),
            },
        ));
        assert_eq!(state.messages[0].authors.len(), 1);
        assert_eq!(state.messages[0].authors[0].display_name, "Ada");
    }

    #[test]
    fn deltas_accumulate_and_complete() {
        let mut state = TimelineState::default();
        state.apply(&envelope(
            1,
            ThreadEvent::TurnStarted {
                turn_id: TurnId::from("turn1"),
                message_id: MessageId::from("m0"),
                author: user(),
                authors: vec![user()],
                prompt: "fix it".to_owned(),
            },
        ));
        state.apply(&envelope(
            2,
            ThreadEvent::MessageDelta {
                message_id: MessageId::from("m1"),
                role: MessageRole::Agent,
                text: "hel".to_owned(),
                author: None,
            },
        ));
        state.apply(&envelope(
            3,
            ThreadEvent::MessageDelta {
                message_id: MessageId::from("m1"),
                role: MessageRole::Agent,
                text: "lo".to_owned(),
                author: None,
            },
        ));
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[1].text, "hello");
        state.apply(&envelope(
            4,
            ThreadEvent::TurnEnded {
                turn_id: TurnId::from("turn1"),
                stop_reason: "end_turn".to_owned(),
            },
        ));
    }
}
