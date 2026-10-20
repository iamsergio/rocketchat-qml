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
        type SessionVm = super::SessionVmRust;

        /// Starts logging in; the outcome shows in the properties. Ignored unless logged out.
        #[qinvokable]
        fn login(
            self: Pin<&mut SessionVm>,
            server: &QString,
            username: &QString,
            password: &QString,
        );

        /// Cancels a login in progress, or logs out. Ignored when logged out.
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
use crate::session_view::{SessionView, Status};

use qobject::SessionStatus;

/// Backing data of [`qobject::SessionVm`].
pub struct SessionVmRust {
    state: SessionStatus,
    busy: bool,
    error_text: QString,
    username: QString,
    server: QString,
}

impl Default for SessionVmRust {
    fn default() -> Self {
        Self::from(&SessionView::default())
    }
}

impl From<&SessionView> for SessionVmRust {
    fn from(view: &SessionView) -> Self {
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
