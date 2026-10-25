// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The commands and the views of the thread pane beside the room. Split from `app.rs` to keep it by
//! concern.

use jiff::Timestamp;
use rc_core::AccountHandle;
use rc_core::domain::MessageId;
use rc_core::threads::ThreadFilter;
use rc_view::thread_list_view::{PaneStep, ThreadListState, close_pane_steps, escape_steps};
use rc_view::thread_view::outgoing_reply;

use super::App;

impl App {
    /// Shows the open thread: where it is, and how loading its replies goes, as the current
    /// account's cache says. Nothing is open for an empty parent id, and then the pane isn't there.
    pub(super) fn show_thread(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let state = self.switchboard.accounts().current_cache().thread_state();
        window.set_thread_id(state.parent_id.into());
        window.set_thread_room_name(state.room_name.into());
        window.set_thread_loading(state.loading);
        window.set_thread_loading_older(state.loading_older);
        window.set_thread_error(state.error_text.into());
        window.set_thread_typing(state.typing.into());
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
    /// and also to the room if `also_to_channel`. False if there is nothing to send (no thread is
    /// open or the text is blank), so that the box keeps its text.
    pub(super) fn send_reply(&self, text: &str, quoted: &str, also_to_channel: bool) -> bool {
        let state = self.switchboard.accounts().current_cache().thread_state();
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
