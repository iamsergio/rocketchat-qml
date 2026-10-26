// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The commands and the views of the pane beside the room: the open thread and the list of
//! threads. Split from `app.rs` to keep it by concern.

use jiff::Timestamp;
use rc_core::AccountHandle;
use rc_core::domain::MessageId;
use rc_core::threads::ThreadFilter;
use rc_view::thread_list_view::{
    PaneStep, ThreadListState, back_steps, close_pane_steps, escape_steps, parse_filter,
    toggle_steps,
};
use rc_view::thread_view::outgoing_reply;

use super::App;
use crate::ui::ThreadPaneState;

impl App {
    /// Tells the list of threads which threads of the open room have unread replies, as the
    /// current account's cache says. Only the rows that look different are announced.
    pub(super) fn show_thread_unread(&self) {
        let unread = self.switchboard.accounts().current_cache().thread_unread();
        self.thread_list.set_unread(unread);
    }

    /// Shows the open thread: where it is, and how loading its replies goes, as the current
    /// account's cache says. Nothing is open for an empty parent id, and then the pane isn't there.
    pub(super) fn show_thread(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let state = self.switchboard.accounts().current_cache().thread_state();
        // Another thread is another conversation: the files that wait were for the last one.
        if window.get_thread_id().as_str() != state.parent_id {
            self.thread_staged.clear();
        }
        window.set_thread_id(state.parent_id.into());
        window.set_thread_room_name(state.room_name.into());
        window.set_thread_loading(state.loading);
        window.set_thread_loading_older(state.loading_older);
        window.set_thread_error(state.error_text.into());
        window.set_thread_typing(state.typing.into());
        // Whether a thread is open decides what the pane shows.
        self.show_thread_list();
    }

    /// Shows what the pane beside the room is about and how the list of threads is doing, as the
    /// current account's cache says.
    pub(super) fn show_thread_list(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let state = self
            .switchboard
            .accounts()
            .current_cache()
            .thread_list_state();
        window.set_thread_pane(ThreadPaneState {
            open: state.pane_open,
            shows_thread: state.thread_open,
            shows_list: state.list_open && !state.thread_open,
            filter: state.filter.into(),
            loading: state.loading,
            loading_more: state.loading_more,
            error_text: state.error_text.into(),
            empty: state.empty,
            empty_title: state.empty_title.into(),
            empty_hint: state.empty_hint.into(),
            unread_count: i32::try_from(state.unread_count).unwrap_or(i32::MAX),
            unread_mention: state.unread_mention,
        });
    }

    /// Shows who is typing in the open thread. Typists emit every few seconds, so this leaves the
    /// rest of the pane alone.
    pub(super) fn show_thread_typing(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let state = self.switchboard.accounts().current_cache().thread_state();
        window.set_thread_typing(state.typing.into());
    }

    /// Opens the thread of a message of the open room beside it. The core fetches the parent if
    /// the room doesn't hold it, and ignores what can't start a thread.
    pub(super) fn open_thread(&self, id: &str) {
        self.current_account("open_thread", |account| {
            account.open_thread(MessageId::from(id.to_owned()));
        });
    }

    /// The threads button of the room's header: shows the list, or closes the pane.
    pub(super) fn toggle_pane(&self) {
        self.pane_command("toggle_pane", toggle_steps);
    }

    /// The back button of a thread: to the list, with the filter last used in the room.
    pub(super) fn back_pane(&self) {
        self.pane_command("back_pane", back_steps);
    }

    /// Lists the open room's threads with the filter the GUI calls `filter`; the same filter again
    /// tries again if the last request failed.
    pub(super) fn open_thread_list(&self, filter: &str) {
        let Some(filter) = parse_filter(filter) else {
            tracing::warn!(filter, "open_thread_list ignored: no such filter");
            return;
        };
        self.current_account("open_thread_list", |account| {
            account.open_thread_list(filter);
        });
    }

    /// Asks for the next page of the list. The core ignores it when there is none or a load is
    /// going on already.
    pub(super) fn load_more_threads(&self) {
        self.current_account("load_more_threads", |account| account.load_more_threads());
    }

    /// The close button of the pane.
    pub(super) fn close_pane(&self) {
        self.pane_command("close_pane", |state, _| close_pane_steps(state));
    }

    /// Escape in the box of the pane, with nothing to cancel there.
    pub(super) fn escape_pane(&self) {
        self.pane_command("escape_pane", escape_steps);
    }

    /// Carries out the steps `decide` takes from the pane's state and the filter last used in the
    /// open room.
    fn pane_command(
        &self,
        what: &str,
        decide: impl FnOnce(&ThreadListState, ThreadFilter) -> Vec<PaneStep>,
    ) {
        let cache = self.switchboard.accounts().current_cache();
        let steps = decide(&cache.thread_list_state(), cache.thread_list_filter());
        self.current_account(what, |account| run_steps(&account, &steps));
    }

    /// Asks for the replies before the first one loaded. The core ignores it when there are none
    /// or a load is going on already.
    pub(super) fn load_older_thread(&self) {
        self.current_account("load_older_thread", |account| account.load_older_thread());
    }

    /// Sends `text` as a reply in the open thread, quoting the message `quoted` (empty for none),
    /// and also to the room if `also_to_channel`, along with the files that wait in the box (then
    /// the text may be blank, and the room does not get the files). False if there is nothing to
    /// send (no thread is open or the text is blank), so that the box keeps its text.
    pub(super) fn send_reply(&self, text: &str, quoted: &str, also_to_channel: bool) -> bool {
        let state = self.switchboard.accounts().current_cache().thread_state();
        let files = self.thread_staged.files();
        if !files.is_empty() {
            return self.send_with_files(true, text, quoted, also_to_channel, &files);
        }
        let Some((room, text, quote, parent)) = outgoing_reply(&state, text, quoted) else {
            return false;
        };
        self.current_account("send_reply", |account| {
            account.send_message(room, &text, quote, Some(parent), also_to_channel);
        });
        true
    }

    /// The id of the newest message of the open thread that the user can edit now, or empty.
    pub(super) fn thread_last_editable(&self) -> String {
        self.switchboard
            .accounts()
            .current_cache()
            .thread_last_editable(Timestamp::now())
            .map_or_else(String::new, |message| message.id.to_string())
    }
}

/// Does what a command on the pane decided, in order.
fn run_steps(account: &AccountHandle<'_>, steps: &[PaneStep]) {
    for step in steps {
        match *step {
            PaneStep::CloseThread => account.close_thread(),
            PaneStep::CloseList => account.close_thread_list(),
            PaneStep::OpenList(filter) => account.open_thread_list(filter),
        }
    }
}
