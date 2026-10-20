// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `Room` view model: the open room, how loading its messages is going, and the commands to
//! open a room, to load older messages and to send.

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
        #[qproperty(QString, kind, READ, NOTIFY)]
        #[qproperty(bool, loading, READ, NOTIFY)]
        #[qproperty(bool, loading_older, cxx_name = "loadingOlder", READ, NOTIFY)]
        #[qproperty(bool, reached_start, cxx_name = "reachedStart", READ, NOTIFY)]
        #[qproperty(QString, error_text, cxx_name = "errorText", READ, NOTIFY)]
        type RoomVm = super::RoomVmRust;

        /// Shows the room's messages; the outcome shows in the properties and in `MessageList`.
        #[qinvokable]
        fn open(self: Pin<&mut RoomVm>, room_id: &QString);

        /// Fetches the messages before the oldest one shown. Ignored when there is nothing to
        /// load or a load is in progress, so QML can call it whenever the top is reached.
        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: Pin<&mut RoomVm>);

        /// Sends the text to the open room. It shows in `MessageList` at once as pending. Returns
        /// whether there was anything to send, so that the composer knows to clear itself.
        #[qinvokable]
        fn send(self: Pin<&mut RoomVm>, text: &QString) -> bool;

        /// Sends a message of the open room that failed again.
        #[qinvokable]
        fn retry(self: Pin<&mut RoomVm>, message_id: &QString);
    }

    impl cxx_qt::Threading for RoomVm {}
    impl cxx_qt::Initialize for RoomVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::hub;
use crate::room_view::{RoomState, outgoing};

/// Backing data of [`qobject::RoomVm`].
pub struct RoomVmRust {
    room_id: QString,
    name: QString,
    kind: QString,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error_text: QString,
}

impl Default for RoomVmRust {
    fn default() -> Self {
        Self::from(&RoomState::default())
    }
}

impl From<&RoomState> for RoomVmRust {
    fn from(state: &RoomState) -> Self {
        Self {
            room_id: QString::from(&state.room_id),
            name: QString::from(&state.name),
            kind: QString::from(&state.kind),
            loading: state.loading,
            loading_older: state.loading_older,
            reached_start: state.reached_start,
            error_text: QString::from(&state.error_text),
        }
    }
}

impl cxx_qt::Initialize for qobject::RoomVm {
    fn initialize(mut self: Pin<&mut Self>) {
        let state = hub::register_room(self.qt_thread());
        self.as_mut().show(&state);
    }
}

impl qobject::RoomVm {
    /// Copies `state` into the properties, then emits the change signal of each one that changed,
    /// so a handler sees the new values of all of them.
    pub fn show(mut self: Pin<&mut Self>, state: &RoomState) {
        let new = RoomVmRust::from(state);
        let room_id = self.room_id != new.room_id;
        let name = self.name != new.name;
        let kind = self.kind != new.kind;
        let loading = self.loading != new.loading;
        let loading_older = self.loading_older != new.loading_older;
        let reached_start = self.reached_start != new.reached_start;
        let error_text = self.error_text != new.error_text;
        *self.as_mut().rust_mut() = new;
        if room_id {
            self.as_mut().room_id_changed();
        }
        if name {
            self.as_mut().name_changed();
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
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn open(self: Pin<&mut Self>, room_id: &QString) {
        hub::open_room(room_id.to_string());
    }

    #[allow(clippy::unused_self)]
    fn load_older(self: Pin<&mut Self>) {
        hub::load_older();
    }

    fn send(self: Pin<&mut Self>, text: &QString) -> bool {
        let Some((room, text)) = outgoing(&self.room_id.to_string(), &text.to_string()) else {
            return false;
        };
        hub::send_message(room, &text);
        true
    }

    #[allow(clippy::unused_self)]
    fn retry(self: Pin<&mut Self>, message_id: &QString) {
        hub::retry_message(message_id.to_string());
    }
}
