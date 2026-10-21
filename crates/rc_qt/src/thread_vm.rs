// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `Thread` view model: the open thread, how loading its replies is going, and the commands to
//! open and close a thread, to load older replies and to reply. Its messages are in
//! `ThreadMessageList`. Editing, deleting and reacting to a message of the thread go through
//! `Room`, which finds a message in either list.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Available in QML as the `Thread` singleton.
        #[qobject]
        #[qml_element = "Thread"]
        #[qml_singleton]
        // The id of the message the thread belongs to, empty while none is open.
        #[qproperty(QString, parent_id, cxx_name = "parentId", READ, NOTIFY)]
        // Whether a thread is open. Not `open`, which is the command.
        #[qproperty(bool, is_open, cxx_name = "isOpen", READ, NOTIFY)]
        #[qproperty(bool, loading, READ, NOTIFY)]
        #[qproperty(bool, loading_older, cxx_name = "loadingOlder", READ, NOTIFY)]
        #[qproperty(bool, reached_start, cxx_name = "reachedStart", READ, NOTIFY)]
        #[qproperty(QString, error_text, cxx_name = "errorText", READ, NOTIFY)]
        // The name of the open room, for the pane's heading and its check box.
        #[qproperty(QString, room_name, cxx_name = "roomName", READ, NOTIFY)]
        type ThreadVm = super::ThreadVmRust;

        /// Shows the replies of a message of the open room beside it; the outcome shows in the
        /// properties and in `ThreadMessageList`. Nothing happens for a message that can't start
        /// a thread or that the room doesn't hold.
        #[qinvokable]
        fn open(self: Pin<&mut ThreadVm>, message_id: &QString);

        /// Closes the open thread. Ignored when none is open.
        #[qinvokable]
        fn close(self: Pin<&mut ThreadVm>);

        /// Fetches the replies before the oldest one shown. Ignored when there is nothing to load
        /// or a load is in progress, so QML can call it whenever the top is reached.
        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: Pin<&mut ThreadVm>);

        /// Sends the text as a reply in the open thread, quoting the message `quoted_id` (empty
        /// for none), and in the room too if `also_to_channel`. It shows in `ThreadMessageList`
        /// at once as pending. Returns whether there was anything to send, so that the composer
        /// knows to clear itself.
        #[qinvokable]
        fn send(
            self: Pin<&mut ThreadVm>,
            text: &QString,
            quoted_id: &QString,
            also_to_channel: bool,
        ) -> bool;

        /// The id of the newest message of the open thread that the user can edit now, or empty.
        #[qinvokable]
        #[cxx_name = "lastEditable"]
        fn last_editable(self: Pin<&mut ThreadVm>) -> QString;

        /// The text of a message of the open room or thread as its author wrote it, for the
        /// composer to edit. Empty if the message isn't there.
        #[qinvokable]
        #[cxx_name = "messageText"]
        fn message_text(self: Pin<&mut ThreadVm>, message_id: &QString) -> QString;

        /// Who wrote a message of the open room or thread, by display name else username, for the
        /// composer to say whom it quotes. Empty if the message isn't there.
        #[qinvokable]
        #[cxx_name = "messageAuthor"]
        fn message_author(self: Pin<&mut ThreadVm>, message_id: &QString) -> QString;
    }

    impl cxx_qt::Threading for ThreadVm {}
    impl cxx_qt::Initialize for ThreadVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::hub;
use crate::thread_view::{ThreadState, outgoing_reply};

/// Backing data of [`qobject::ThreadVm`].
#[allow(clippy::struct_excessive_bools)] // A property each.
pub struct ThreadVmRust {
    parent_id: QString,
    is_open: bool,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error_text: QString,
    room_name: QString,
}

impl Default for ThreadVmRust {
    fn default() -> Self {
        Self::from(&ThreadState::default())
    }
}

impl From<&ThreadState> for ThreadVmRust {
    fn from(state: &ThreadState) -> Self {
        Self {
            parent_id: QString::from(&state.parent_id),
            is_open: !state.parent_id.is_empty(),
            loading: state.loading,
            loading_older: state.loading_older,
            reached_start: state.reached_start,
            error_text: QString::from(&state.error_text),
            room_name: QString::from(&state.room_name),
        }
    }
}

impl cxx_qt::Initialize for qobject::ThreadVm {
    fn initialize(mut self: Pin<&mut Self>) {
        let state = hub::register_thread(self.qt_thread());
        self.as_mut().show(&state);
    }
}

impl qobject::ThreadVm {
    /// Copies `state` into the properties, then emits the change signal of each one that changed,
    /// so a handler sees the new values of all of them.
    pub fn show(mut self: Pin<&mut Self>, state: &ThreadState) {
        let new = ThreadVmRust::from(state);
        let parent_id = self.parent_id != new.parent_id;
        let is_open = self.is_open != new.is_open;
        let loading = self.loading != new.loading;
        let loading_older = self.loading_older != new.loading_older;
        let reached_start = self.reached_start != new.reached_start;
        let error_text = self.error_text != new.error_text;
        let room_name = self.room_name != new.room_name;
        *self.as_mut().rust_mut() = new;
        if parent_id {
            self.as_mut().parent_id_changed();
        }
        if is_open {
            self.as_mut().is_open_changed();
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
        if room_name {
            self.as_mut().room_name_changed();
        }
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn open(self: Pin<&mut Self>, message_id: &QString) {
        hub::open_thread(message_id.to_string());
    }

    #[allow(clippy::unused_self)]
    fn close(self: Pin<&mut Self>) {
        hub::close_thread();
    }

    #[allow(clippy::unused_self)]
    fn load_older(self: Pin<&mut Self>) {
        hub::load_older_thread();
    }

    #[allow(clippy::unused_self)]
    fn send(
        self: Pin<&mut Self>,
        text: &QString,
        quoted_id: &QString,
        also_to_channel: bool,
    ) -> bool {
        let Some((room, text, quoted, parent)) = outgoing_reply(
            &hub::thread_state(),
            &text.to_string(),
            &quoted_id.to_string(),
        ) else {
            return false;
        };
        hub::send_reply(room, &text, quoted, parent, also_to_channel);
        true
    }

    #[allow(clippy::unused_self)]
    fn last_editable(self: Pin<&mut Self>) -> QString {
        QString::from(&hub::last_editable_in_thread())
    }

    #[allow(clippy::unused_self)]
    fn message_text(self: Pin<&mut Self>, message_id: &QString) -> QString {
        QString::from(&hub::message_text(&message_id.to_string()))
    }

    #[allow(clippy::unused_self)]
    fn message_author(self: Pin<&mut Self>, message_id: &QString) -> QString {
        QString::from(&hub::message_author(&message_id.to_string()))
    }
}
