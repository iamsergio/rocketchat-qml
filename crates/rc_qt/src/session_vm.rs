// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `Session` view model: login state, error text and the login/logout commands.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    /// Where the session is. `Unknown` until the core has said anything.
    #[qenum(SessionVm)]
    enum SessionStatus {
        Unknown,
        LoggedOut,
        Connecting,
        Resuming,
        LoggedIn,
    }

    /// What the login page is for.
    #[qenum(SessionVm)]
    enum LoginMode {
        /// Log in: the only account, or one that is logged out but not expired.
        Login,
        /// A new account, with others to go back to.
        Adding,
        /// The session ended.
        Again,
    }

    extern "RustQt" {
        /// Available in QML as the `Session` singleton.
        #[qobject]
        #[qml_element = "Session"]
        #[qml_singleton]
        #[qproperty(SessionStatus, state, READ, NOTIFY)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(QString, error_text, cxx_name = "errorText", READ, NOTIFY)]
        #[qproperty(QString, username, READ, NOTIFY)]
        #[qproperty(QString, server, READ, NOTIFY)]
        // The server without its scheme or path.
        #[qproperty(QString, host, READ, NOTIFY)]
        // Empty while the realtime connection is up.
        #[qproperty(QString, connection_text, cxx_name = "connectionText", READ, NOTIFY)]
        // The local id of the account shown: the login page starts again when it changes.
        #[qproperty(QString, account_id, cxx_name = "accountId", READ, NOTIFY)]
        #[qproperty(LoginMode, login_mode, cxx_name = "loginMode", READ, NOTIFY)]
        // The heading, the sentence under it and the notice of the login page. Empty if there is
        // none.
        #[qproperty(QString, login_title, cxx_name = "loginTitle", READ, NOTIFY)]
        #[qproperty(QString, login_lede, cxx_name = "loginLede", READ, NOTIFY)]
        #[qproperty(QString, login_notice, cxx_name = "loginNotice", READ, NOTIFY)]
        // What the login fields start with: who the account is, if it says so; else empty, unless
        // a developer-mode build got `--dev`.
        #[qproperty(QString, login_server, cxx_name = "loginServer", READ, NOTIFY)]
        #[qproperty(QString, login_username, cxx_name = "loginUsername", READ, NOTIFY)]
        // The password the fields start with: empty, unless a developer-mode build got `--dev`.
        #[qproperty(
            QString,
            prefill_password,
            cxx_name = "prefillPassword",
            READ,
            CONSTANT
        )]
        type SessionVm = super::SessionVmRust;

        /// Starts logging in; the outcome shows in the properties. Ignored unless logged out.
        #[qinvokable]
        fn login(
            self: Pin<&mut SessionVm>,
            server: &QString,
            username: &QString,
            password: &QString,
        );

        /// Cancels a login in progress, or logs out, and forgets the account: the next one takes
        /// its place on screen, or a new one if it was the last. Works for a logged-out account
        /// too (the login page's Cancel and Remove this server).
        #[qinvokable]
        fn logout(self: Pin<&mut SessionVm>);
    }

    impl cxx_qt::Threading for SessionVm {}
    impl cxx_qt::Initialize for SessionVm {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use secrecy::SecretString;

use crate::hub;
use rc_view::dev_login;
use rc_view::session_view::{self, SessionView, Status};

use qobject::{LoginMode, SessionStatus};

/// `value`, or `prefill` if it is empty.
fn or_prefill(value: &str, prefill: &str) -> String {
    if value.is_empty() { prefill } else { value }.to_owned()
}

/// Backing data of [`qobject::SessionVm`].
pub struct SessionVmRust {
    state: SessionStatus,
    busy: bool,
    error_text: QString,
    username: QString,
    server: QString,
    host: QString,
    connection_text: QString,
    account_id: QString,
    login_mode: LoginMode,
    login_title: QString,
    login_lede: QString,
    login_notice: QString,
    login_server: QString,
    login_username: QString,
    prefill_password: QString,
}

impl Default for SessionVmRust {
    fn default() -> Self {
        Self::from(&SessionView::default())
    }
}

impl From<&SessionView> for SessionVmRust {
    fn from(view: &SessionView) -> Self {
        // Set before the QML is loaded and never changed, so `show` keeps the same values.
        let prefill = dev_login::prefill();
        Self {
            state: match view.status {
                Status::Unknown => SessionStatus::Unknown,
                Status::LoggedOut => SessionStatus::LoggedOut,
                Status::Connecting => SessionStatus::Connecting,
                Status::Resuming => SessionStatus::Resuming,
                Status::LoggedIn => SessionStatus::LoggedIn,
            },
            busy: view.busy,
            error_text: QString::from(&view.error_text),
            username: QString::from(&view.username),
            server: QString::from(&view.server),
            host: QString::from(&view.host),
            connection_text: QString::from(&view.connection_text),
            account_id: QString::from(&view.account_id),
            login_mode: match view.login_mode {
                session_view::LoginMode::Login => LoginMode::Login,
                session_view::LoginMode::Adding => LoginMode::Adding,
                session_view::LoginMode::Again => LoginMode::Again,
            },
            login_title: QString::from(&view.login_title),
            login_lede: QString::from(&view.login_lede),
            login_notice: QString::from(&view.login_notice),
            login_server: QString::from(&or_prefill(&view.login_server, &prefill.server)),
            login_username: QString::from(&or_prefill(&view.login_username, &prefill.username)),
            prefill_password: QString::from(&prefill.password),
        }
    }
}

impl cxx_qt::Initialize for qobject::SessionVm {
    fn initialize(mut self: Pin<&mut Self>) {
        let view = hub::register_session(self.qt_thread());
        self.as_mut().show(&view);
    }
}

impl qobject::SessionVm {
    /// Copies `view` into the properties, then emits the change signal of each one that changed,
    /// so a handler sees the new values of all of them.
    pub fn show(mut self: Pin<&mut Self>, view: &SessionView) {
        let new = SessionVmRust::from(view);
        let state = self.state != new.state;
        let busy = self.busy != new.busy;
        let error_text = self.error_text != new.error_text;
        let username = self.username != new.username;
        let server = self.server != new.server;
        let host = self.host != new.host;
        let connection_text = self.connection_text != new.connection_text;
        let account_id = self.account_id != new.account_id;
        let login_mode = self.login_mode != new.login_mode;
        let login_title = self.login_title != new.login_title;
        let login_lede = self.login_lede != new.login_lede;
        let login_notice = self.login_notice != new.login_notice;
        let login_server = self.login_server != new.login_server;
        let login_username = self.login_username != new.login_username;
        *self.as_mut().rust_mut() = new;
        if state {
            self.as_mut().state_changed();
        }
        if busy {
            self.as_mut().busy_changed();
        }
        if error_text {
            self.as_mut().error_text_changed();
        }
        if username {
            self.as_mut().username_changed();
        }
        if server {
            self.as_mut().server_changed();
        }
        if host {
            self.as_mut().host_changed();
        }
        if connection_text {
            self.as_mut().connection_text_changed();
        }
        if account_id {
            self.as_mut().account_id_changed();
        }
        if login_mode {
            self.as_mut().login_mode_changed();
        }
        if login_title {
            self.as_mut().login_title_changed();
        }
        if login_lede {
            self.as_mut().login_lede_changed();
        }
        if login_notice {
            self.as_mut().login_notice_changed();
        }
        if login_server {
            self.as_mut().login_server_changed();
        }
        if login_username {
            self.as_mut().login_username_changed();
        }
    }

    #[allow(clippy::unused_self)] // The invokable needs an object; the core is process-wide.
    fn login(self: Pin<&mut Self>, server: &QString, username: &QString, password: &QString) {
        // The password is copied out of the QString, which can't be wiped: the copy in Qt's
        // memory stays until it is freed. The Rust side wraps its copy right away.
        let password = SecretString::from(password.to_string());
        hub::login(&server.to_string(), &username.to_string(), password);
    }

    #[allow(clippy::unused_self)]
    fn logout(self: Pin<&mut Self>) {
        hub::logout();
    }
}
