// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The commands of the app that act on messages: sending, retrying, editing, deleting and reacting,
//! what the menu of a message offers, and the lookups the composer needs. Split from `app.rs` to
//! keep it by concern.

use jiff::Timestamp;
use rc_core::domain::MessageId;
use rc_core::policy::can_react;
use rc_core::policy::{ActionError, MessageAction};
use rc_view::message_view::{can_delete, can_edit, can_quote};
use rc_view::room_view::outgoing;

use super::App;
use crate::ui::MessageMenu;

impl App {
    /// Sends `text` to the open room, quoting the message `quoted` (empty for none). False if there
    /// is nothing to send (no room is open or the text is blank), so that the composer keeps its
    /// text.
    pub(super) fn send(&self, text: &str, quoted: &str) -> bool {
        let room_id = self
            .switchboard
            .accounts()
            .current_cache()
            .room_state()
            .room_id;
        let Some((room, text, quote)) = outgoing(&room_id, text, quoted) else {
            return false;
        };
        self.current_account("send_message", |account| {
            account.send_message(room, &text, quote, None, false);
        });
        true
    }

    /// Sends a message that failed to send again.
    pub(super) fn retry(&self, id: &str) {
        self.current_account("retry_message", |account| {
            account.retry_message(MessageId::from(id.to_owned()));
        });
    }

    /// Shows why the last edit or deletion of a message failed, or nothing for the empty text.
    pub(super) fn show_room_action_error(&self, text: &str) {
        if let Some(window) = self.window.upgrade() {
            window.set_room_action_error(text.into());
        }
    }

    /// Takes the core's answer to an edit or a deletion in: a failure is shown, and the composer
    /// is told how an edit ended.
    pub(super) fn finish_action(
        &self,
        id: &MessageId,
        action: MessageAction,
        outcome: &Result<(), ActionError>,
    ) {
        let text = outcome
            .as_ref()
            .err()
            .map_or_else(String::new, ToString::to_string);
        self.show_room_action_error(&text);
        if action == MessageAction::Edit
            && let Some(window) = self.window.upgrade()
        {
            window.set_edit_answer_id(id.as_str().into());
            window.set_edit_answer_ok(outcome.is_ok());
            window.set_edit_answers(window.get_edit_answers().wrapping_add(1));
        }
    }

    /// What the menu of the message offers now: the policy has time limits, so this is asked when
    /// the menu opens and not worked out with the row.
    pub(super) fn menu_allowed(&self, id: &str) -> MessageMenu {
        let cache = self.switchboard.accounts().current_cache();
        let viewer = cache.viewer();
        let now = Timestamp::now();
        cache
            .held(id)
            .map_or_else(MessageMenu::default, |message| MessageMenu {
                can_quote: can_quote(message),
                can_react: can_react(message),
                can_edit: can_edit(message, &viewer, now),
                can_delete: can_delete(message, &viewer, now),
            })
    }

    /// The text of a message as written, empty if it isn't held.
    pub(super) fn message_text(&self, id: &str) -> String {
        self.switchboard
            .accounts()
            .current_cache()
            .message_text(id)
            .map_or_else(String::new, str::to_owned)
    }

    /// Who wrote a message, as the composer's quote bar names them; empty if it isn't held.
    pub(super) fn message_author(&self, id: &str) -> String {
        self.switchboard
            .accounts()
            .current_cache()
            .message_author(id)
            .map_or_else(String::new, str::to_owned)
    }

    /// The id of the newest message of the open room that the user can edit now, or empty.
    pub(super) fn last_editable(&self) -> String {
        self.switchboard
            .accounts()
            .current_cache()
            .last_editable(Timestamp::now())
            .map_or_else(String::new, |message| message.id.to_string())
    }

    /// Adds the user's reaction to a message, or takes it back. The answer is
    /// `Event::MessageAction`, which shows a failure.
    pub(super) fn react(&self, id: &str, emoji: &str, react: bool) {
        self.show_room_action_error("");
        self.current_account("react", |account| {
            account.react(MessageId::from(id.to_owned()), emoji, react);
        });
    }

    /// Saves the new text of a message. The answer is `Event::MessageAction`.
    pub(super) fn edit_message(&self, id: &str, text: &str) {
        self.show_room_action_error("");
        self.current_account("edit_message", |account| {
            account.edit_message(MessageId::from(id.to_owned()), text);
        });
    }

    /// Deletes a message. The answer is `Event::MessageAction`.
    pub(super) fn delete_message(&self, id: &str) {
        self.show_room_action_error("");
        self.current_account("delete_message", |account| {
            account.delete_message(MessageId::from(id.to_owned()));
        });
    }
}
