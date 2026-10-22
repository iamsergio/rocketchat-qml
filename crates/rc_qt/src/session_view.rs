// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `Session` view model shows for a session state. Plain Rust, so it can be unit-tested
//! without Qt; the `QObject` only copies the values into its properties.

use rc_core::Connection;
use rc_core::session::SessionState;

/// The session as QML sees it. Mirrors the `SessionStatus` Qt enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Before the first event from the core.
    #[default]
    Unknown,
    LoggedOut,
    Connecting,
    /// A stored session is being picked up at startup.
    Resuming,
    LoggedIn,
}

/// The values of the `Session` properties.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionView {
    pub status: Status,
    /// Waiting for something: the first event, or a login or a resume in progress. Derived here so that QML
    /// has no logic.
    pub busy: bool,
    /// Why the last login failed, as it is to be shown. Empty if it didn't.
    pub error_text: String,
    /// The logged-in account; empty otherwise.
    pub username: String,
    pub server: String,
    /// The server without its scheme or path, as the sidebar names it; see [`host`].
    pub host: String,
    /// What to tell the user while the realtime connection is down. Empty while it is up, and
    /// outside a session.
    pub connection_text: String,
}

/// The view of `state`, where `None` is the state before the first event, with the connection as
/// it was then.
#[must_use]
pub fn session_view(state: Option<&SessionState>, connection: Connection) -> SessionView {
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
        Some(SessionState::Resuming) => SessionView {
            status: Status::Resuming,
            busy: true,
            ..SessionView::default()
        },
        Some(SessionState::LoggedIn(account)) => SessionView {
            status: Status::LoggedIn,
            username: account.user.username.clone(),
            server: account.server.clone(),
            host: host(&account.server),
            connection_text: connection_text(connection),
            ..SessionView::default()
        },
    }
}

/// The host and port of `server`, a URL: no scheme, no path, query or fragment.
#[must_use]
pub fn host(server: &str) -> String {
    let rest = server.split_once("://").map_or(server, |(_, rest)| rest);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn connection_text(connection: Connection) -> String {
    match connection {
        Connection::Connected => String::new(),
        Connection::Waiting { retry_in } => {
            let seconds = retry_in.as_secs() + u64::from(retry_in.subsec_nanos() > 0);
            format!("Connection lost, reconnecting in {}s…", seconds.max(1))
        }
        Connection::Reconnecting => "Reconnecting…".to_owned(),
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
        let view = session_view(None, Connection::Connected);
        assert_eq!(view.status, Status::Unknown);
        assert!(view.busy);
        assert_eq!(view.error_text, "");
    }

    // AUTH-001
    #[test]
    fn is_busy_while_connecting() {
        let view = session_view(Some(&SessionState::Connecting), Connection::Connected);
        assert_eq!(view.status, Status::Connecting);
        assert!(view.busy);
    }

    // AUTH-004
    #[test]
    fn is_busy_while_resuming() {
        let view = session_view(Some(&SessionState::Resuming), Connection::Connected);
        assert_eq!(view.status, Status::Resuming);
        assert!(view.busy);
    }

    // AUTH-001
    #[test]
    fn logged_in_shows_the_account() {
        let view = session_view(Some(&logged_in()), Connection::Connected);
        assert_eq!(view.status, Status::LoggedIn);
        assert!(!view.busy);
        assert_eq!(view.username, "admin");
        assert_eq!(view.server, "http://127.0.0.1:3000");
        assert_eq!(view.host, "127.0.0.1:3000");
        assert_eq!(view.error_text, "");
    }

    // ACCT-001
    #[test]
    fn the_host_has_no_scheme_or_path() {
        for (server, expected) in [
            ("http://127.0.0.1:3000", "127.0.0.1:3000"),
            ("https://chat.example.com/", "chat.example.com"),
            (
                "https://chat.example.com/sub/path?x=1#y",
                "chat.example.com",
            ),
            ("HTTPS://Chat.Example.com:8443/a", "Chat.Example.com:8443"),
            ("chat.example.com/a", "chat.example.com"),
            ("", ""),
        ] {
            assert_eq!(host(server), expected, "{server}");
        }
    }

    // AUTH-002
    #[test]
    fn invalid_credentials_show_their_text() {
        let state = SessionState::LoggedOut {
            failure: Some(LoginFailure::InvalidCredentials),
        };
        let view = session_view(Some(&state), Connection::Connected);
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
            assert_eq!(
                session_view(Some(&state), Connection::Connected).error_text,
                text
            );
        }
    }

    // AUTH-005
    #[test]
    fn logged_out_without_a_failure_has_no_error() {
        let view = session_view(
            Some(&SessionState::LoggedOut { failure: None }),
            Connection::Connected,
        );
        assert_eq!(view.status, Status::LoggedOut);
        assert!(!view.busy);
        assert_eq!(view.error_text, "");
        assert_eq!(view.username, "");
        assert_eq!(view.server, "");
        assert_eq!(view.host, "");
    }

    // RT-004
    #[test]
    fn the_connection_text_follows_the_connection() {
        use std::time::Duration;

        let text = |connection| session_view(Some(&logged_in()), connection).connection_text;
        assert_eq!(text(Connection::Connected), "");
        assert_eq!(text(Connection::Reconnecting), "Reconnecting…");
        assert_eq!(
            text(Connection::Waiting {
                retry_in: Duration::from_millis(1200)
            }),
            "Connection lost, reconnecting in 2s…"
        );
        assert_eq!(
            text(Connection::Waiting {
                retry_in: Duration::from_millis(10)
            }),
            "Connection lost, reconnecting in 1s…"
        );
    }

    // RT-004
    #[test]
    fn there_is_no_connection_text_outside_a_session() {
        let state = SessionState::LoggedOut { failure: None };
        let view = session_view(Some(&state), Connection::Reconnecting);
        assert_eq!(view.connection_text, "");
    }
}
