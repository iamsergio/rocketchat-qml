// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `Github` view model: the tile of the rail, the connect page, the header of the list and the
//! header of the pull request that is open. The hub builds the state (see `github_view`) and this
//! only shows it, one property at a time.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    /// Where the connection to GitHub is.
    #[qenum(GithubVm)]
    enum ConnectState {
        NotConnected,
        Connecting,
        Failed,
        Connected,
    }

    extern "RustQt" {
        /// Available in QML as the `Github` singleton.
        #[qobject]
        #[qml_element = "Github"]
        #[qml_singleton]
        // The GitHub page is on screen, in place of an account's.
        #[qproperty(bool, shown, READ, NOTIFY)]
        // The tile: connected, how many pull requests fail, a pull request finished unseen, and
        // the tool tip.
        #[qproperty(bool, tile_connected, cxx_name = "tileConnected", READ, NOTIFY)]
        #[qproperty(i32, tile_failing, cxx_name = "tileFailing", READ, NOTIFY)]
        #[qproperty(bool, tile_pip, cxx_name = "tilePip", READ, NOTIFY)]
        #[qproperty(QString, tip_title, cxx_name = "tipTitle", READ, NOTIFY)]
        #[qproperty(QString, tip_line, cxx_name = "tipLine", READ, NOTIFY)]
        // The connect page.
        #[qproperty(ConnectState, connect_state, cxx_name = "connectState", READ, NOTIFY)]
        #[qproperty(QString, connect_reason, cxx_name = "connectReason", READ, NOTIFY)]
        // The header of the list: its title, "<login>, updated 40 s ago", whether it refreshes,
        // and the counts of the tabs.
        #[qproperty(QString, header_title, cxx_name = "headerTitle", READ, NOTIFY)]
        #[qproperty(QString, header_status, cxx_name = "headerStatus", READ, NOTIFY)]
        #[qproperty(bool, refreshing, READ, NOTIFY)]
        #[qproperty(i32, mine_count, cxx_name = "mineCount", READ, NOTIFY)]
        #[qproperty(i32, watched_count, cxx_name = "watchedCount", READ, NOTIFY)]
        // The pull request that is open: empty `pageId` when none is.
        #[qproperty(QString, page_id, cxx_name = "pageId", READ, NOTIFY)]
        #[qproperty(QString, page_repo, cxx_name = "pageRepo", READ, NOTIFY)]
        #[qproperty(QString, page_number, cxx_name = "pageNumber", READ, NOTIFY)]
        #[qproperty(QString, page_updated, cxx_name = "pageUpdated", READ, NOTIFY)]
        #[qproperty(QString, page_title, cxx_name = "pageTitle", READ, NOTIFY)]
        #[qproperty(QString, page_head, cxx_name = "pageHead", READ, NOTIFY)]
        #[qproperty(QString, page_base, cxx_name = "pageBase", READ, NOTIFY)]
        #[qproperty(QString, page_review, cxx_name = "pageReview", READ, NOTIFY)]
        #[qproperty(QString, page_base_note, cxx_name = "pageBaseNote", READ, NOTIFY)]
        #[qproperty(bool, page_draft, cxx_name = "pageDraft", READ, NOTIFY)]
        #[qproperty(QString, page_url, cxx_name = "pageUrl", READ, NOTIFY)]
        type GithubVm = super::GithubVmRust;

        /// Shows the GitHub page in place of the account's.
        #[qinvokable]
        fn show(self: Pin<&mut GithubVm>);

        /// Connects with the token of the GitHub CLI.
        #[qinvokable]
        #[cxx_name = "connectWithGh"]
        fn connect_with_gh(self: Pin<&mut GithubVm>);

        /// Connects with a personal access token.
        #[qinvokable]
        #[cxx_name = "connectWithToken"]
        fn connect_with_token(self: Pin<&mut GithubVm>, token: &QString);

        /// Forgets the login.
        #[qinvokable]
        fn disconnect(self: Pin<&mut GithubVm>);

        /// Fetches the lists now.
        #[qinvokable]
        fn refresh(self: Pin<&mut GithubVm>);

        /// The time passed: shows the ages and "updated N s ago" again. The page calls it every
        /// so often while it is on screen.
        #[qinvokable]
        fn tick(self: Pin<&mut GithubVm>);
    }

    impl cxx_qt::Threading for GithubVm {}
    impl cxx_qt::Initialize for GithubVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rc_view::github::ConnectState as ViewConnect;
use secrecy::SecretString;

use crate::github_view::VmState;
use crate::hub;

use qobject::{ConnectState, GithubVm};

/// Backing data of [`qobject::GithubVm`]. A field for each property, flags and all.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
pub struct GithubVmRust {
    shown: bool,
    tile_connected: bool,
    tile_failing: i32,
    tile_pip: bool,
    tip_title: QString,
    tip_line: QString,
    connect_state: ConnectState,
    connect_reason: QString,
    header_title: QString,
    header_status: QString,
    refreshing: bool,
    mine_count: i32,
    watched_count: i32,
    page_id: QString,
    page_repo: QString,
    page_number: QString,
    page_updated: QString,
    page_title: QString,
    page_head: QString,
    page_base: QString,
    page_review: QString,
    page_base_note: QString,
    page_draft: bool,
    page_url: QString,
}

impl Default for ConnectState {
    fn default() -> Self {
        Self::NotConnected
    }
}

impl From<&VmState> for GithubVmRust {
    fn from(state: &VmState) -> Self {
        let text = |value: &str| QString::from(value);
        Self {
            shown: state.shown,
            tile_connected: state.tile_connected,
            tile_failing: state.tile_failing,
            tile_pip: state.tile_pip,
            tip_title: text(&state.tip_title),
            tip_line: text(&state.tip_line),
            connect_state: match state.connect {
                ViewConnect::NotConnected => ConnectState::NotConnected,
                ViewConnect::Connecting => ConnectState::Connecting,
                ViewConnect::Failed => ConnectState::Failed,
                ViewConnect::Connected => ConnectState::Connected,
            },
            connect_reason: text(&state.connect_reason),
            header_title: text(&state.header_title),
            header_status: text(&state.header_status),
            refreshing: state.refreshing,
            mine_count: state.mine_count,
            watched_count: state.watched_count,
            page_id: text(&state.page.id),
            page_repo: text(&state.page.repo),
            page_number: text(&state.page.number),
            page_updated: text(&state.page.updated),
            page_title: text(&state.page.title),
            page_head: text(&state.page.head),
            page_base: text(&state.page.base),
            page_review: text(&state.page.review),
            page_base_note: text(&state.page.base_note),
            page_draft: state.page.draft,
            page_url: text(&state.page.url),
        }
    }
}

impl cxx_qt::Initialize for GithubVm {
    fn initialize(mut self: Pin<&mut Self>) {
        let state = hub::register_github(self.qt_thread());
        *self.as_mut().rust_mut() = GithubVmRust::from(&state);
    }
}

/// Takes `$new` in and emits the change signal of each property that differs, after all of them
/// are set, so a handler sees the new values of every one.
macro_rules! show_changes {
    ($this:ident, $new:ident, $($field:ident => $signal:ident),+ $(,)?) => {{
        let changes = ($($this.$field != $new.$field,)+);
        *$this.as_mut().rust_mut() = $new;
        let ($($field,)+) = changes;
        $(
            if $field {
                $this.as_mut().$signal();
            }
        )+
    }};
}

impl GithubVm {
    /// Copies the state into the properties.
    pub fn show_state(mut self: Pin<&mut Self>, state: &VmState) {
        let new = GithubVmRust::from(state);
        show_changes!(
            self, new,
            shown => shown_changed,
            tile_connected => tile_connected_changed,
            tile_failing => tile_failing_changed,
            tile_pip => tile_pip_changed,
            tip_title => tip_title_changed,
            tip_line => tip_line_changed,
            connect_state => connect_state_changed,
            connect_reason => connect_reason_changed,
            header_title => header_title_changed,
            header_status => header_status_changed,
            refreshing => refreshing_changed,
            mine_count => mine_count_changed,
            watched_count => watched_count_changed,
            page_id => page_id_changed,
            page_repo => page_repo_changed,
            page_number => page_number_changed,
            page_updated => page_updated_changed,
            page_title => page_title_changed,
            page_head => page_head_changed,
            page_base => page_base_changed,
            page_review => page_review_changed,
            page_base_note => page_base_note_changed,
            page_draft => page_draft_changed,
            page_url => page_url_changed,
        );
    }

    #[allow(clippy::unused_self)] // The invokables need an object; the service is process-wide.
    fn show(self: Pin<&mut Self>) {
        hub::github_show();
    }

    #[allow(clippy::unused_self)]
    fn connect_with_gh(self: Pin<&mut Self>) {
        hub::github_connect_with_gh();
    }

    #[allow(clippy::unused_self)]
    fn connect_with_token(self: Pin<&mut Self>, token: &QString) {
        // Copied out of the QString, which can't be wiped; the Rust side wraps its copy at once.
        hub::github_connect_with_token(SecretString::from(token.to_string()));
    }

    #[allow(clippy::unused_self)]
    fn disconnect(self: Pin<&mut Self>) {
        hub::github_disconnect();
    }

    #[allow(clippy::unused_self)]
    fn refresh(self: Pin<&mut Self>) {
        hub::github_refresh();
    }

    #[allow(clippy::unused_self)]
    fn tick(self: Pin<&mut Self>) {
        hub::github_tick();
    }
}
