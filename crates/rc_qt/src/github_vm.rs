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

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
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
        // The actions of the pull request: the banner for runs that wait for approval, the
        // buttons (each as visible, enabled and busy as far as it varies), why Merge is disabled,
        // the question Merge asks first ("merge" or "merge-anyway", else none), the notify
        // toggle, and how the last command went: why it failed, or the toast for one that is done.
        #[qproperty(QString, approval_text, cxx_name = "approvalText", READ, NOTIFY)]
        #[qproperty(bool, approve_enabled, cxx_name = "approveEnabled", READ, NOTIFY)]
        #[qproperty(bool, approve_busy, cxx_name = "approveBusy", READ, NOTIFY)]
        #[qproperty(bool, merge_enabled, cxx_name = "mergeEnabled", READ, NOTIFY)]
        #[qproperty(bool, merge_busy, cxx_name = "mergeBusy", READ, NOTIFY)]
        #[qproperty(QString, merge_reason, cxx_name = "mergeReason", READ, NOTIFY)]
        #[qproperty(
            bool,
            merge_anyway_visible,
            cxx_name = "mergeAnywayVisible",
            READ,
            NOTIFY
        )]
        #[qproperty(
            bool,
            merge_anyway_enabled,
            cxx_name = "mergeAnywayEnabled",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, merge_anyway_busy, cxx_name = "mergeAnywayBusy", READ, NOTIFY)]
        #[qproperty(
            bool,
            update_branch_visible,
            cxx_name = "updateBranchVisible",
            READ,
            NOTIFY
        )]
        #[qproperty(
            bool,
            update_branch_enabled,
            cxx_name = "updateBranchEnabled",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, update_branch_busy, cxx_name = "updateBranchBusy", READ, NOTIFY)]
        #[qproperty(
            bool,
            rerun_failed_visible,
            cxx_name = "rerunFailedVisible",
            READ,
            NOTIFY
        )]
        #[qproperty(
            bool,
            rerun_failed_enabled,
            cxx_name = "rerunFailedEnabled",
            READ,
            NOTIFY
        )]
        #[qproperty(bool, rerun_failed_busy, cxx_name = "rerunFailedBusy", READ, NOTIFY)]
        #[qproperty(bool, notify_visible, cxx_name = "notifyVisible", READ, NOTIFY)]
        #[qproperty(bool, notify_enabled, cxx_name = "notifyEnabled", READ, NOTIFY)]
        #[qproperty(QString, notify_label, cxx_name = "notifyLabel", READ, NOTIFY)]
        #[qproperty(bool, notifying, READ, NOTIFY)]
        #[qproperty(QString, confirm_kind, cxx_name = "confirmKind", READ, NOTIFY)]
        #[qproperty(QString, confirm_text, cxx_name = "confirmText", READ, NOTIFY)]
        #[qproperty(QString, command_error, cxx_name = "commandError", READ, NOTIFY)]
        #[qproperty(QString, toast_text, cxx_name = "toastText", READ, NOTIFY)]
        // The checks: the heading, the tally ("1 failed, 2 running"; and by state, for the
        // swatches) and the segments of the bar.
        #[qproperty(QString, checks_heading, cxx_name = "checksHeading", READ, NOTIFY)]
        #[qproperty(QString, checks_tally, cxx_name = "checksTally", READ, NOTIFY)]
        #[qproperty(QStringList, tally_states, cxx_name = "tallyStates", READ, NOTIFY)]
        #[qproperty(QStringList, tally_texts, cxx_name = "tallyTexts", READ, NOTIFY)]
        #[qproperty(QStringList, checks_bar, cxx_name = "checksBar", READ, NOTIFY)]
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

        /// Asks to confirm Merge, or Merge anyway when `force`.
        #[qinvokable]
        #[cxx_name = "askMerge"]
        fn ask_merge(self: Pin<&mut GithubVm>, force: bool);

        /// Drops the question of Merge.
        #[qinvokable]
        #[cxx_name = "cancelConfirm"]
        fn cancel_confirm(self: Pin<&mut GithubVm>);

        /// Merges the pull request that is open; `force` though its checks failed.
        #[qinvokable]
        fn merge(self: Pin<&mut GithubVm>, force: bool);

        /// Updates the branch of the pull request that is open.
        #[qinvokable]
        #[cxx_name = "updateBranch"]
        fn update_branch(self: Pin<&mut GithubVm>);

        /// Reruns the failed jobs of the pull request that is open.
        #[qinvokable]
        #[cxx_name = "rerunFailed"]
        fn rerun_failed(self: Pin<&mut GithubVm>);

        /// Reruns the job at this position of the check bar.
        #[qinvokable]
        #[cxx_name = "rerunJob"]
        fn rerun_job(self: Pin<&mut GithubVm>, index: i32);

        /// Approves the runs of the pull request that is open that wait for approval.
        #[qinvokable]
        #[cxx_name = "approveRuns"]
        fn approve_runs(self: Pin<&mut GithubVm>);

        /// Asks to be told (or not) when the CI of the pull request that is open ends.
        #[qinvokable]
        #[cxx_name = "setNotify"]
        fn set_notify(self: Pin<&mut GithubVm>, on: bool);

        /// The toast or the failure of the last command was shown or closed.
        #[qinvokable]
        #[cxx_name = "dismissCommand"]
        fn dismiss_command(self: Pin<&mut GithubVm>);
    }

    impl cxx_qt::Threading for GithubVm {}
    impl cxx_qt::Initialize for GithubVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
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
    approval_text: QString,
    approve_enabled: bool,
    approve_busy: bool,
    merge_enabled: bool,
    merge_busy: bool,
    merge_reason: QString,
    merge_anyway_visible: bool,
    merge_anyway_enabled: bool,
    merge_anyway_busy: bool,
    update_branch_visible: bool,
    update_branch_enabled: bool,
    update_branch_busy: bool,
    rerun_failed_visible: bool,
    rerun_failed_enabled: bool,
    rerun_failed_busy: bool,
    notify_visible: bool,
    notify_enabled: bool,
    notify_label: QString,
    notifying: bool,
    confirm_kind: QString,
    confirm_text: QString,
    command_error: QString,
    toast_text: QString,
    checks_heading: QString,
    checks_tally: QString,
    tally_states: QStringList,
    tally_texts: QStringList,
    checks_bar: QStringList,
}

impl Default for ConnectState {
    fn default() -> Self {
        Self::NotConnected
    }
}

impl From<&VmState> for GithubVmRust {
    fn from(state: &VmState) -> Self {
        let text = |value: &str| QString::from(value);
        let list = |values: &[String]| -> QStringList {
            values
                .iter()
                .map(|value| QString::from(value.as_str()))
                .collect()
        };
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
            approval_text: text(&state.page.approval),
            approve_enabled: state.page.approve.enabled,
            approve_busy: state.page.approve.busy,
            merge_enabled: state.page.merge.enabled,
            merge_busy: state.page.merge.busy,
            merge_reason: text(&state.page.merge_reason),
            merge_anyway_visible: state.page.merge_anyway.visible,
            merge_anyway_enabled: state.page.merge_anyway.enabled,
            merge_anyway_busy: state.page.merge_anyway.busy,
            update_branch_visible: state.page.update_branch.visible,
            update_branch_enabled: state.page.update_branch.enabled,
            update_branch_busy: state.page.update_branch.busy,
            rerun_failed_visible: state.page.rerun_failed.visible,
            rerun_failed_enabled: state.page.rerun_failed.enabled,
            rerun_failed_busy: state.page.rerun_failed.busy,
            notify_visible: state.page.notify.visible,
            notify_enabled: state.page.notify.enabled,
            notify_label: text(&state.page.notify_label),
            notifying: state.page.notifying,
            confirm_kind: text(&state.page.confirm_kind),
            confirm_text: text(&state.page.confirm_text),
            command_error: text(&state.command_error),
            toast_text: text(&state.toast),
            checks_heading: text(&state.page.heading),
            checks_tally: text(&state.page.tally),
            tally_states: list(&state.page.tally_states),
            tally_texts: list(&state.page.tally_texts),
            checks_bar: list(&state.page.bar),
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
            approval_text => approval_text_changed,
            approve_enabled => approve_enabled_changed,
            approve_busy => approve_busy_changed,
            merge_enabled => merge_enabled_changed,
            merge_busy => merge_busy_changed,
            merge_reason => merge_reason_changed,
            merge_anyway_visible => merge_anyway_visible_changed,
            merge_anyway_enabled => merge_anyway_enabled_changed,
            merge_anyway_busy => merge_anyway_busy_changed,
            update_branch_visible => update_branch_visible_changed,
            update_branch_enabled => update_branch_enabled_changed,
            update_branch_busy => update_branch_busy_changed,
            rerun_failed_visible => rerun_failed_visible_changed,
            rerun_failed_enabled => rerun_failed_enabled_changed,
            rerun_failed_busy => rerun_failed_busy_changed,
            notify_visible => notify_visible_changed,
            notify_enabled => notify_enabled_changed,
            notify_label => notify_label_changed,
            notifying => notifying_changed,
            confirm_kind => confirm_kind_changed,
            confirm_text => confirm_text_changed,
            command_error => command_error_changed,
            toast_text => toast_text_changed,
            checks_heading => checks_heading_changed,
            checks_tally => checks_tally_changed,
            tally_states => tally_states_changed,
            tally_texts => tally_texts_changed,
            checks_bar => checks_bar_changed,
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

    #[allow(clippy::unused_self)]
    fn ask_merge(self: Pin<&mut Self>, force: bool) {
        hub::github_ask_merge(force);
    }

    #[allow(clippy::unused_self)]
    fn cancel_confirm(self: Pin<&mut Self>) {
        hub::github_cancel_confirm();
    }

    #[allow(clippy::unused_self)]
    fn merge(self: Pin<&mut Self>, force: bool) {
        hub::github_merge(force);
    }

    #[allow(clippy::unused_self)]
    fn update_branch(self: Pin<&mut Self>) {
        hub::github_update_branch();
    }

    #[allow(clippy::unused_self)]
    fn rerun_failed(self: Pin<&mut Self>) {
        hub::github_rerun_failed();
    }

    #[allow(clippy::unused_self)]
    fn rerun_job(self: Pin<&mut Self>, index: i32) {
        if let Ok(index) = usize::try_from(index) {
            hub::github_rerun_job(index);
        }
    }

    #[allow(clippy::unused_self)]
    fn approve_runs(self: Pin<&mut Self>) {
        hub::github_approve_runs();
    }

    #[allow(clippy::unused_self)]
    fn set_notify(self: Pin<&mut Self>, on: bool) {
        hub::github_set_notify(on);
    }

    #[allow(clippy::unused_self)]
    fn dismiss_command(self: Pin<&mut Self>) {
        hub::github_dismiss_command();
    }
}
