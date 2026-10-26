// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What users may do with messages: the server's settings and the rules built on them.

use std::time::Duration;

use crate::domain::{Message, MessageKind, UserId};
use crate::ports::ApiError;

/// Something the user does to a message: change or delete one of their own, or react to any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageAction {
    Edit,
    Delete,
    React,
}

/// Why a [`MessageAction`] didn't happen. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    /// Refused without asking the server: the message isn't the user's (for an edit or a
    /// deletion), isn't sent, isn't text, isn't in the open room, or the settings don't allow it
    /// (any more).
    #[error("This message can't be {}", match .0 {
        MessageAction::Edit => "edited",
        MessageAction::Delete => "deleted",
        MessageAction::React => "reacted to",
    })]
    NotAllowed(MessageAction),
    #[error("A message can't be empty")]
    Empty,
    #[error("Can't reach the server")]
    Unreachable,
    /// Any other error, with its message.
    #[error("The server reported an error: {0}")]
    Server(String),
}

impl From<&ApiError> for ActionError {
    fn from(error: &ApiError) -> Self {
        match error {
            ApiError::Unreachable(_) => Self::Unreachable,
            other => Self::Server(other.to_string()),
        }
    }
}

/// Whether the server lets users do one kind of change to their messages, and for how long after
/// sending them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowance {
    pub allowed: bool,
    /// How long after a message was sent it can still be changed. `None` is no limit.
    pub within: Option<Duration>,
}

impl Allowance {
    /// The server's settings give minutes, where 0 is no limit.
    #[must_use]
    pub fn from_settings(allowed: bool, minutes: u32) -> Self {
        Self {
            allowed,
            within: (minutes > 0).then(|| Duration::from_secs(u64::from(minutes) * 60)),
        }
    }

    /// Whether a message sent at `sent` can still be changed at `now`. The limit itself is still
    /// within it.
    #[must_use]
    pub fn permits(&self, sent: jiff::Timestamp, now: jiff::Timestamp) -> bool {
        if !self.allowed {
            return false;
        }
        let Some(within) = self.within else {
            return true;
        };
        let elapsed = now.duration_since(sent);
        jiff::SignedDuration::try_from(within).map_or(true, |limit| elapsed <= limit)
    }
}

impl Default for Allowance {
    /// What a server without the settings does: allowed, no limit.
    fn default() -> Self {
        Self {
            allowed: true,
            within: None,
        }
    }
}

/// What the server lets users do with their own messages. Fetched once per sync cycle. Changing
/// the messages of others takes permissions that this client doesn't look at: the server refuses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessagePolicy {
    pub editing: Allowance,
    pub deleting: Allowance,
}

impl From<rc_api::MessageSettings> for MessagePolicy {
    fn from(settings: rc_api::MessageSettings) -> Self {
        Self {
            editing: Allowance::from_settings(settings.editing_allowed, settings.edit_minutes),
            deleting: Allowance::from_settings(settings.deleting_allowed, settings.delete_minutes),
        }
    }
}

impl MessagePolicy {
    /// Whether `me` can edit the message at `now`: it is theirs, the server has it, it says
    /// something (not a system message), and the settings allow it.
    #[must_use]
    pub fn can_edit(&self, message: &Message, me: &UserId, now: jiff::Timestamp) -> bool {
        matches!(message.kind, MessageKind::Text(_))
            && Self::own_and_sent(message, me)
            && self.editing.permits(message.ts, now)
    }

    /// Whether `me` can delete the message at `now`: it is theirs, the server has it, it is not a
    /// system message, and the settings allow it.
    #[must_use]
    pub fn can_delete(&self, message: &Message, me: &UserId, now: jiff::Timestamp) -> bool {
        matches!(message.kind, MessageKind::Text(_))
            && Self::own_and_sent(message, me)
            && self.deleting.permits(message.ts, now)
    }

    fn own_and_sent(message: &Message, me: &UserId) -> bool {
        &message.author.id == me && !message.is_unsent()
    }
}

/// Whether users can react to the message: the server has it and it says something (not a system
/// message). Whoever wrote it; the server refuses a reaction the user has no right to.
#[must_use]
pub fn can_react(message: &Message) -> bool {
    matches!(message.kind, MessageKind::Text(_)) && !message.is_unsent()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Author, Delivery, SystemEvent};

    fn at(secs: i64) -> jiff::Timestamp {
        jiff::Timestamp::from_second(secs).unwrap()
    }

    fn me() -> UserId {
        "me".to_owned().into()
    }

    fn message(author: &str, delivery: Delivery, kind: MessageKind) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: author.to_owned().into(),
                username: author.to_owned(),
                display_name: None,
            },
            ts: at(1_000),
            kind,
            delivery,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
            thread_followers: Vec::new(),
        }
    }

    fn own() -> Message {
        message("me", Delivery::Sent, MessageKind::Text("hi".into()))
    }

    fn limited(minutes: u32) -> MessagePolicy {
        MessagePolicy {
            editing: Allowance::from_settings(true, minutes),
            deleting: Allowance::from_settings(true, minutes),
        }
    }

    // ACT-001, ACT-002
    #[test]
    fn the_default_policy_lets_users_change_their_own_sent_messages() {
        let policy = MessagePolicy::default();
        assert!(policy.can_edit(&own(), &me(), at(1_000_000)));
        assert!(policy.can_delete(&own(), &me(), at(1_000_000)));
    }

    // ACT-001, ACT-002
    #[test]
    fn messages_of_others_unsent_ones_and_system_messages_cannot_be_changed() {
        let policy = MessagePolicy::default();
        let text = || MessageKind::Text("hi".into());
        let others = message("bob", Delivery::Sent, text());
        let pending = message("me", Delivery::Pending, text());
        let failed = message("me", Delivery::Failed, text());
        let system = message(
            "me",
            Delivery::Sent,
            MessageKind::System(SystemEvent::Joined),
        );
        for message in [others, pending, failed, system] {
            assert!(!policy.can_edit(&message, &me(), at(2_000)), "{message:?}");
            assert!(
                !policy.can_delete(&message, &me(), at(2_000)),
                "{message:?}"
            );
        }
    }

    // ACT-003
    #[test]
    fn sent_text_messages_can_be_reacted_to_whoever_wrote_them() {
        let text = || MessageKind::Text("hi".into());
        assert!(can_react(&own()));
        assert!(can_react(&message("bob", Delivery::Sent, text())));
    }

    // ACT-003
    #[test]
    fn unsent_messages_and_system_messages_cannot_be_reacted_to() {
        let text = || MessageKind::Text("hi".into());
        let pending = message("me", Delivery::Pending, text());
        let failed = message("bob", Delivery::Failed, text());
        let system = message(
            "bob",
            Delivery::Sent,
            MessageKind::System(SystemEvent::Joined),
        );
        for message in [pending, failed, system] {
            assert!(!can_react(&message), "{message:?}");
        }
    }

    // ACT-001
    #[test]
    fn a_server_that_forbids_editing_still_lets_users_delete() {
        let policy = MessagePolicy {
            editing: Allowance::from_settings(false, 0),
            deleting: Allowance::default(),
        };
        assert!(!policy.can_edit(&own(), &me(), at(2_000)));
        assert!(policy.can_delete(&own(), &me(), at(2_000)));
    }

    // ACT-002
    #[test]
    fn a_server_that_forbids_deleting_still_lets_users_edit() {
        let policy = MessagePolicy {
            editing: Allowance::default(),
            deleting: Allowance::from_settings(false, 0),
        };
        assert!(policy.can_edit(&own(), &me(), at(2_000)));
        assert!(!policy.can_delete(&own(), &me(), at(2_000)));
    }

    // ACT-001, ACT-002
    #[test]
    fn a_time_limit_counts_from_the_servers_time_and_its_end_is_still_within() {
        let policy = limited(5);
        let sent = 1_000;
        assert!(policy.can_edit(&own(), &me(), at(sent + 299)));
        assert!(policy.can_edit(&own(), &me(), at(sent + 300)));
        assert!(!policy.can_edit(&own(), &me(), at(sent + 301)));
        assert!(policy.can_delete(&own(), &me(), at(sent + 300)));
        assert!(!policy.can_delete(&own(), &me(), at(sent + 301)));
    }

    // ACT-001
    #[test]
    fn zero_minutes_is_no_limit_and_a_clock_behind_the_server_is_within() {
        assert_eq!(Allowance::from_settings(true, 0), Allowance::default());
        assert!(limited(5).can_edit(&own(), &me(), at(500)));
    }

    // ACT-001, ACT-002
    #[test]
    fn the_policy_comes_from_the_servers_settings() {
        let settings = rc_api::MessageSettings {
            editing_allowed: true,
            edit_minutes: 10,
            deleting_allowed: false,
            delete_minutes: 0,
            ..rc_api::MessageSettings::default()
        };
        assert_eq!(
            MessagePolicy::from(settings),
            MessagePolicy {
                editing: Allowance {
                    allowed: true,
                    within: Some(Duration::from_secs(600)),
                },
                deleting: Allowance {
                    allowed: false,
                    within: None,
                },
            }
        );
        assert_eq!(
            MessagePolicy::from(rc_api::MessageSettings::default()),
            MessagePolicy::default()
        );
    }

    // ACT-001, ACT-002
    #[test]
    fn errors_have_user_facing_texts() {
        assert_eq!(
            ActionError::NotAllowed(MessageAction::Edit).to_string(),
            "This message can't be edited"
        );
        assert_eq!(
            ActionError::NotAllowed(MessageAction::Delete).to_string(),
            "This message can't be deleted"
        );
        assert_eq!(
            ActionError::NotAllowed(MessageAction::React).to_string(),
            "This message can't be reacted to"
        );
        assert_eq!(
            ActionError::from(&ApiError::Unreachable("refused".into())).to_string(),
            "Can't reach the server"
        );
        let server = ApiError::Server {
            status: 400,
            message: "not allowed".into(),
        };
        assert_eq!(
            ActionError::from(&server).to_string(),
            "The server reported an error: the server answered 400: not allowed"
        );
    }
}
