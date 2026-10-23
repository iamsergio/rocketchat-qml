// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `ThreadList` view model: whether the open room's threads are listed, how loading them is
//! going, what the pane beside the room shows, and the commands that move around in the pane
//! (open and close the list, go back from a thread, toggle the pane). The threads themselves are
//! in `ThreadListModel`; the open thread is `Thread`'s. What the pane shows, and what each command
//! does, is decided here, not in QML.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Available in QML as the `ThreadList` singleton.
        #[qobject]
        #[qml_element = "ThreadList"]
        #[qml_singleton]
        // What the pane beside the room shows: `thread` while a thread is open, else `list` while
        // the list is open, else `none`.
        #[qproperty(QString, pane, READ, NOTIFY)]
        // Whether the pane shows anything: the list or a thread.
        #[qproperty(bool, pane_open, cxx_name = "paneOpen", READ, NOTIFY)]
        // Whether the list is open, whether or not a thread hides it.
        #[qproperty(bool, list_open, cxx_name = "listOpen", READ, NOTIFY)]
        // `all` or `following`.
        #[qproperty(QString, filter, READ, NOTIFY)]
        #[qproperty(bool, loading, READ, NOTIFY)]
        #[qproperty(bool, loading_more, cxx_name = "loadingMore", READ, NOTIFY)]
        #[qproperty(bool, reached_end, cxx_name = "reachedEnd", READ, NOTIFY)]
        #[qproperty(QString, error_text, cxx_name = "errorText", READ, NOTIFY)]
        // The name of the open room, for the pane's heading.
        #[qproperty(QString, room_name, cxx_name = "roomName", READ, NOTIFY)]
        // The list has loaded and holds nothing; the words say so.
        #[qproperty(bool, empty, READ, NOTIFY)]
        #[qproperty(QString, empty_title, cxx_name = "emptyTitle", READ, NOTIFY)]
        #[qproperty(QString, empty_hint, cxx_name = "emptyHint", READ, NOTIFY)]
        type ThreadListVm = super::ThreadListVmRust;

        /// Lists the open room's threads, `all` or `following`; with another filter than the open
        /// list's, it starts again from the first page. Nothing happens for a name that is
        /// neither.
        #[qinvokable]
        #[cxx_name = "openList"]
        fn open_list(self: Pin<&mut ThreadListVm>, filter: &QString);

        /// Closes the list. The open thread stays.
        #[qinvokable]
        #[cxx_name = "closeList"]
        fn close_list(self: Pin<&mut ThreadListVm>);

        /// Fetches the next page of the list. Ignored when there is nothing to load or a load is
        /// in progress, so QML can call it whenever the end is reached.
        #[qinvokable]
        #[cxx_name = "loadMore"]
        fn load_more(self: Pin<&mut ThreadListVm>);

        /// Leaves the open thread for the list, opening the list if it was not open (with the
        /// filter last used in this room).
        #[qinvokable]
        fn back(self: Pin<&mut ThreadListVm>);

        /// Closes the thread and the list: the whole pane.
        #[qinvokable]
        #[cxx_name = "closePane"]
        fn close_pane(self: Pin<&mut ThreadListVm>);

        /// The header button: closes the pane if it shows something, else opens the list.
        #[qinvokable]
        fn toggle(self: Pin<&mut ThreadListVm>);

        /// What Escape does in the pane: back from a thread, close the list from the list.
        #[qinvokable]
        fn escape(self: Pin<&mut ThreadListVm>);
    }

    impl cxx_qt::Threading for ThreadListVm {}
    impl cxx_qt::Initialize for ThreadListVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::hub;
use rc_view::thread_list_view::{ThreadListState, parse_filter};

/// Backing data of [`qobject::ThreadListVm`].
#[allow(clippy::struct_excessive_bools)] // A property each.
pub struct ThreadListVmRust {
    pane: QString,
    pane_open: bool,
    list_open: bool,
    filter: QString,
    loading: bool,
    loading_more: bool,
    reached_end: bool,
    error_text: QString,
    room_name: QString,
    empty: bool,
    empty_title: QString,
    empty_hint: QString,
}

impl Default for ThreadListVmRust {
    fn default() -> Self {
        Self::from(&ThreadListState::default())
    }
}

impl From<&ThreadListState> for ThreadListVmRust {
    fn from(state: &ThreadListState) -> Self {
        Self {
            pane: QString::from(&state.pane),
            pane_open: state.pane_open,
            list_open: state.list_open,
            filter: QString::from(&state.filter),
            loading: state.loading,
            loading_more: state.loading_more,
            reached_end: state.reached_end,
            error_text: QString::from(&state.error_text),
            room_name: QString::from(&state.room_name),
            empty: state.empty,
            empty_title: QString::from(&state.empty_title),
            empty_hint: QString::from(&state.empty_hint),
        }
    }
}

impl cxx_qt::Initialize for qobject::ThreadListVm {
    fn initialize(mut self: Pin<&mut Self>) {
        let state = hub::register_thread_list(self.qt_thread());
        self.as_mut().show(&state);
    }
}

impl qobject::ThreadListVm {
    /// Copies `state` into the properties, then emits the change signal of each one that changed,
    /// so a handler sees the new values of all of them.
    pub fn show(mut self: Pin<&mut Self>, state: &ThreadListState) {
        let new = ThreadListVmRust::from(state);
        let pane = self.pane != new.pane;
        let pane_open = self.pane_open != new.pane_open;
        let list_open = self.list_open != new.list_open;
        let filter = self.filter != new.filter;
        let loading = self.loading != new.loading;
        let loading_more = self.loading_more != new.loading_more;
        let reached_end = self.reached_end != new.reached_end;
        let error_text = self.error_text != new.error_text;
        let room_name = self.room_name != new.room_name;
        let empty = self.empty != new.empty;
        let empty_title = self.empty_title != new.empty_title;
        let empty_hint = self.empty_hint != new.empty_hint;
        *self.as_mut().rust_mut() = new;
        if pane {
            self.as_mut().pane_changed();
        }
        if pane_open {
            self.as_mut().pane_open_changed();
        }
        if list_open {
            self.as_mut().list_open_changed();
        }
        if filter {
            self.as_mut().filter_changed();
        }
        if loading {
            self.as_mut().loading_changed();
        }
        if loading_more {
            self.as_mut().loading_more_changed();
        }
        if reached_end {
            self.as_mut().reached_end_changed();
        }
        if error_text {
            self.as_mut().error_text_changed();
        }
        if room_name {
            self.as_mut().room_name_changed();
        }
        if empty {
            self.as_mut().empty_changed();
        }
        if empty_title {
            self.as_mut().empty_title_changed();
        }
        if empty_hint {
            self.as_mut().empty_hint_changed();
        }
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn open_list(self: Pin<&mut Self>, filter: &QString) {
        if let Some(filter) = parse_filter(&filter.to_string()) {
            hub::open_thread_list(filter);
        }
    }

    #[allow(clippy::unused_self)]
    fn close_list(self: Pin<&mut Self>) {
        hub::close_thread_list();
    }

    #[allow(clippy::unused_self)]
    fn load_more(self: Pin<&mut Self>) {
        hub::load_more_threads();
    }

    #[allow(clippy::unused_self)]
    fn back(self: Pin<&mut Self>) {
        hub::thread_back();
    }

    #[allow(clippy::unused_self)]
    fn close_pane(self: Pin<&mut Self>) {
        hub::close_pane();
    }

    #[allow(clippy::unused_self)]
    fn toggle(self: Pin<&mut Self>) {
        hub::toggle_pane();
    }

    #[allow(clippy::unused_self)]
    fn escape(self: Pin<&mut Self>) {
        hub::escape_pane();
    }
}
