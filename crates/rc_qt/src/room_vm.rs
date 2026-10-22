// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `Room` view model: the open room, how loading its messages is going, and the commands to
//! open a room, to load older messages, to send (quoting a message if asked), to edit and delete
//! the user's messages, and to react to messages.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Available in QML as the `Room` singleton.
        #[qobject]
        #[qml_element = "Room"]
        #[qml_singleton]
        #[qproperty(QString, room_id, cxx_name = "roomId", READ, NOTIFY)]
        #[qproperty(QString, name, READ, NOTIFY)]
        #[qproperty(QString, topic_html, cxx_name = "topicHtml", READ, NOTIFY)]
        #[qproperty(QString, kind, READ, NOTIFY)]
        #[qproperty(bool, loading, READ, NOTIFY)]
        #[qproperty(bool, loading_older, cxx_name = "loadingOlder", READ, NOTIFY)]
        #[qproperty(bool, reached_start, cxx_name = "reachedStart", READ, NOTIFY)]
        #[qproperty(QString, error_text, cxx_name = "errorText", READ, NOTIFY)]
        // `actionError`: why the last edit, deletion or reaction failed, to show until it is
        // dismissed. Cleared by the next one, by `dismissActionError` and by opening another room.
        #[qproperty(QString, action_error, cxx_name = "actionError", READ, NOTIFY)]
        // `typing`: who is typing in the open room, as a line for the room view ("alice is
        // typing…"), empty for nobody.
        #[qproperty(QString, typing, READ, NOTIFY)]
        type RoomVm = super::RoomVmRust;

        /// Shows the room's messages; the outcome shows in the properties and in `MessageList`.
        #[qinvokable]
        fn open(self: Pin<&mut RoomVm>, room_id: &QString);

        /// Fetches the messages before the oldest one shown. Ignored when there is nothing to
        /// load or a load is in progress, so QML can call it whenever the top is reached.
        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: Pin<&mut RoomVm>);

        /// Sends the text to the open room, quoting the message `quoted_id` (empty for none). It
        /// shows in `MessageList` at once as pending. Returns whether there was anything to send,
        /// so that the composer knows to clear itself.
        #[qinvokable]
        fn send(self: Pin<&mut RoomVm>, text: &QString, quoted_id: &QString) -> bool;

        /// Sends a message of the open room that failed again.
        #[qinvokable]
        fn retry(self: Pin<&mut RoomVm>, message_id: &QString);

        /// Replaces the text of a message of the user's in the open room. Always answered by
        /// `actionDone`, also when the core refuses it at once (not the user's message, blank
        /// text...). The message changes in `MessageList` before the answer comes.
        #[qinvokable]
        fn edit(self: Pin<&mut RoomVm>, message_id: &QString, text: &QString);

        /// Deletes a message of the user's in the open room. Answered by `actionDone`, as `edit`
        /// is. The row leaves `MessageList` before the answer comes.
        #[qinvokable]
        fn remove(self: Pin<&mut RoomVm>, message_id: &QString);

        /// Adds the user's reaction with the emoji (a shortcode, colons optional) to a message of
        /// the open room, or takes it back. Answered by `actionDone`, as `edit` is. The reaction
        /// shows in `MessageList` before the answer comes.
        #[qinvokable]
        fn react(self: Pin<&mut RoomVm>, message_id: &QString, emoji: &QString, reacted: bool);

        /// The id of the newest message of the open room that the user can edit now, or empty.
        #[qinvokable]
        #[cxx_name = "lastEditable"]
        fn last_editable(self: Pin<&mut RoomVm>) -> QString;

        /// The text of a message of the open room as its author wrote it, for the composer to
        /// edit. Empty if the message isn't there.
        #[qinvokable]
        #[cxx_name = "messageText"]
        fn message_text(self: Pin<&mut RoomVm>, message_id: &QString) -> QString;

        /// Who wrote a message of the open room, by display name else username, for the composer
        /// to say whom it quotes. Empty if the message isn't there.
        #[qinvokable]
        #[cxx_name = "messageAuthor"]
        fn message_author(self: Pin<&mut RoomVm>, message_id: &QString) -> QString;

        /// Clears `actionError`.
        #[qinvokable]
        #[cxx_name = "dismissActionError"]
        fn dismiss_action_error(self: Pin<&mut RoomVm>);

        /// An edit, a deletion or a reaction ended. `action` is `"edit"`, `"delete"` or `"react"`;
        /// `errorText` is empty if `ok`. A failure is in `actionError` too.
        #[qsignal]
        #[cxx_name = "actionDone"]
        fn action_done(
            self: Pin<&mut RoomVm>,
            message_id: &QString,
            action: &QString,
            ok: bool,
            error_text: &QString,
        );
    }

    impl cxx_qt::Threading for RoomVm {}
    impl cxx_qt::Initialize for RoomVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rc_core::policy::{ActionError, MessageAction};

use crate::hub;
use rc_view::room_view::{RoomState, action_name, outgoing};

/// Backing data of [`qobject::RoomVm`].
pub struct RoomVmRust {
    room_id: QString,
    name: QString,
    topic_html: QString,
    kind: QString,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error_text: QString,
    action_error: QString,
    typing: QString,
}

impl Default for RoomVmRust {
    fn default() -> Self {
        Self::new(&RoomState::default(), "")
    }
}

impl RoomVmRust {
    /// The values for `state`, whose topic is `topic_html` once rendered.
    fn new(state: &RoomState, topic_html: &str) -> Self {
        Self {
            room_id: QString::from(&state.room_id),
            name: QString::from(&state.name),
            topic_html: QString::from(topic_html),
            kind: QString::from(&state.kind),
            loading: state.loading,
            loading_older: state.loading_older,
            reached_start: state.reached_start,
            error_text: QString::from(&state.error_text),
            action_error: QString::default(),
            typing: QString::from(&state.typing),
        }
    }
}

impl cxx_qt::Initialize for qobject::RoomVm {
    fn initialize(mut self: Pin<&mut Self>) {
        let (state, topic_html) = hub::register_room(self.qt_thread());
        self.as_mut().show(&state, &topic_html);
    }
}

impl qobject::RoomVm {
    /// Copies `state` into the properties, with `topic_html` (its topic rendered) for the topic,
    /// then emits the change signal of each one that changed, so a handler sees the new values of
    /// all of them.
    pub fn show(mut self: Pin<&mut Self>, state: &RoomState, topic_html: &str) {
        let mut new = RoomVmRust::new(state, topic_html);
        let room_id = self.room_id != new.room_id;
        let name = self.name != new.name;
        let topic_html = self.topic_html != new.topic_html;
        let kind = self.kind != new.kind;
        let loading = self.loading != new.loading;
        let loading_older = self.loading_older != new.loading_older;
        let reached_start = self.reached_start != new.reached_start;
        let error_text = self.error_text != new.error_text;
        let typing = self.typing != new.typing;
        // What failed in one room means nothing in another. The state doesn't know of it.
        if !room_id {
            new.action_error.clone_from(&self.action_error);
        }
        let action_error = self.action_error != new.action_error;
        *self.as_mut().rust_mut() = new;
        if room_id {
            self.as_mut().room_id_changed();
        }
        if name {
            self.as_mut().name_changed();
        }
        if topic_html {
            self.as_mut().topic_html_changed();
        }
        if kind {
            self.as_mut().kind_changed();
        }
        if loading {
            self.as_mut().loading_changed();
        }
        if loading_older {
            self.as_mut().loading_older_changed();
        }
        if reached_start {
            self.as_mut().reached_start_changed();
        }
        if error_text {
            self.as_mut().error_text_changed();
        }
        if action_error {
            self.as_mut().action_error_changed();
        }
        if typing {
            self.as_mut().typing_changed();
        }
    }

    /// Takes the core's answer to an edit, a deletion or a reaction in: a failure becomes
    /// `actionError`.
    pub fn finish_action(
        mut self: Pin<&mut Self>,
        message_id: &str,
        action: MessageAction,
        outcome: &Result<(), ActionError>,
    ) {
        let error_text = outcome
            .as_ref()
            .err()
            .map_or_else(String::new, ToString::to_string);
        self.as_mut().set_action_error_text(&error_text);
        self.action_done(
            &QString::from(message_id),
            &QString::from(action_name(action)),
            outcome.is_ok(),
            &QString::from(&error_text),
        );
    }

    /// Sets `actionError`, announcing it if it changed.
    fn set_action_error_text(mut self: Pin<&mut Self>, text: &str) {
        let text = QString::from(text);
        if self.action_error != text {
            self.as_mut().rust_mut().action_error = text;
            self.action_error_changed();
        }
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn open(self: Pin<&mut Self>, room_id: &QString) {
        hub::open_room(room_id.to_string());
    }

    #[allow(clippy::unused_self)]
    fn load_older(self: Pin<&mut Self>) {
        hub::load_older();
    }

    fn send(self: Pin<&mut Self>, text: &QString, quoted_id: &QString) -> bool {
        let Some((room, text, quoted)) = outgoing(
            &self.room_id.to_string(),
            &text.to_string(),
            &quoted_id.to_string(),
        ) else {
            return false;
        };
        hub::send_message(room, &text, quoted);
        true
    }

    #[allow(clippy::unused_self)]
    fn retry(self: Pin<&mut Self>, message_id: &QString) {
        hub::retry_message(message_id.to_string());
    }

    fn edit(mut self: Pin<&mut Self>, message_id: &QString, text: &QString) {
        self.as_mut().set_action_error_text("");
        hub::edit_message(message_id.to_string(), &text.to_string());
    }

    fn remove(mut self: Pin<&mut Self>, message_id: &QString) {
        self.as_mut().set_action_error_text("");
        hub::delete_message(message_id.to_string());
    }

    fn react(mut self: Pin<&mut Self>, message_id: &QString, emoji: &QString, reacted: bool) {
        self.as_mut().set_action_error_text("");
        hub::react(message_id.to_string(), &emoji.to_string(), reacted);
    }

    #[allow(clippy::unused_self)]
    fn last_editable(self: Pin<&mut Self>) -> QString {
        QString::from(&hub::last_editable())
    }

    #[allow(clippy::unused_self)]
    fn message_text(self: Pin<&mut Self>, message_id: &QString) -> QString {
        QString::from(&hub::message_text(&message_id.to_string()))
    }

    #[allow(clippy::unused_self)]
    fn message_author(self: Pin<&mut Self>, message_id: &QString) -> QString {
        QString::from(&hub::message_author(&message_id.to_string()))
    }

    pub fn dismiss_action_error(self: Pin<&mut Self>) {
        self.set_action_error_text("");
    }
}
