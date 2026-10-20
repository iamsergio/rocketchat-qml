// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `Session` view model shows for a session state. Plain Rust, so it can be unit-tested
//! without Qt; the `QObject` only copies the values into its properties.

use rc_core::session::SessionState;

/// The session as QML sees it. Mirrors the `SessionStatus` Qt enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Before the first event from the core.
    #[default]
    Unknown,
    LoggedOut,
    Connecting,
    LoggedIn,
}

/// The values of the `Session` properties.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionView {
    pub status: Status,
    /// Waiting for something: the first event, or a login in progress. Derived here so that QML
    /// has no logic.
    pub busy: bool,
    /// Why the last login failed, as it is to be shown. Empty if it didn't.
    pub error_text: String,
    /// The logged-in account; empty otherwise.
    pub username: String,
    pub server: String,
}

/// The view of `state`, where `None` is the state before the first event.
#[must_use]
pub fn session_view(state: Option<&SessionState>) -> SessionView {
    match state {
        None => SessionView {
            status: Status::Unknown,
            busy: true,
            ..SessionView::default()
        },
        Some(SessionState::LoggedOut { failure }) => SessionView {
            status: Status::LoggedOut,
            error_text: failure
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            ..SessionView::default()
        },
        Some(SessionState::Connecting) => SessionView {
            status: Status::Connecting,
            busy: true,
            ..SessionView::default()
        },
        Some(SessionState::LoggedIn(account)) => SessionView {
            status: Status::LoggedIn,
            username: account.user.username.clone(),
            server: account.server.clone(),
            ..SessionView::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Account, User};
    use rc_core::session::LoginFailure;

    use super::*;

    fn logged_in() -> SessionState {
        SessionState::LoggedIn(Account {
            server: "http://127.0.0.1:3000".to_owned(),
            user: User {
                id: "u1".to_owned().into(),
                username: "admin".to_owned(),
                display_name: None,
            },
        })
    }

    // AUTH-001
    #[test]
    fn is_busy_until_the_first_event() {
        let view = session_view(None);
        assert_eq!(view.status, Status::Unknown);
        assert!(view.busy);
        assert_eq!(view.error_text, "");
    }

    // AUTH-001
    #[test]
    fn is_busy_while_connecting() {
        let view = session_view(Some(&SessionState::Connecting));
        assert_eq!(view.status, Status::Connecting);
        assert!(view.busy);
    }

    // AUTH-001
    #[test]
    fn logged_in_shows_the_account() {
        let view = session_view(Some(&logged_in()));
        assert_eq!(view.status, Status::LoggedIn);
        assert!(!view.busy);
        assert_eq!(view.username, "admin");
        assert_eq!(view.server, "http://127.0.0.1:3000");
        assert_eq!(view.error_text, "");
    }

    // AUTH-002
    #[test]
    fn invalid_credentials_show_their_text() {
        let state = SessionState::LoggedOut {
            failure: Some(LoginFailure::InvalidCredentials),
        };
        let view = session_view(Some(&state));
        assert_eq!(view.status, Status::LoggedOut);
        assert!(!view.busy);
        assert_eq!(view.error_text, "Wrong username or password");
        assert_eq!(view.username, "");
    }

    // AUTH-003
    #[test]
    fn other_failures_show_their_text() {
        for (failure, text) in [
            (LoginFailure::Unreachable, "Can't reach the server"),
            (
                LoginFailure::NotRocketChat,
                "That server doesn't look like Rocket.Chat",
            ),
            (
                LoginFailure::UnsupportedVersion {
                    found: "7.9".to_owned(),
                    minimum: "8.0".to_owned(),
                },
                "Rocket.Chat 7.9 is too old, 8.0 or newer is needed",
            ),
        ] {
            let state = SessionState::LoggedOut {
                failure: Some(failure),
            };
            assert_eq!(session_view(Some(&state)).error_text, text);
        }
    }

    // AUTH-005
    #[test]
    fn logged_out_without_a_failure_has_no_error() {
        let view = session_view(Some(&SessionState::LoggedOut { failure: None }));
        assert_eq!(view.status, Status::LoggedOut);
        assert!(!view.busy);
        assert_eq!(view.error_text, "");
        assert_eq!(view.username, "");
        assert_eq!(view.server, "");
    }
}
