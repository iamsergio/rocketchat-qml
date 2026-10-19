// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The session state machine and the login use case.

use rc_api::ServerVersion;
use secrecy::SecretString;

use crate::domain::{Account, Credentials};
use crate::ports::{ApiError, Connector, ServerApi};

/// The oldest Rocket.Chat the app supports. Follows the company server (see PLAN.md, Decisions);
/// 8.0 is also what the local test server runs.
pub const MIN_SERVER_VERSION: ServerVersion = ServerVersion { major: 8, minor: 0 };

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// `failure` says why the last login attempt failed; `None` after a logout or at startup.
    LoggedOut {
        failure: Option<LoginFailure>,
    },
    Connecting,
    LoggedIn(Account),
}

/// Why a login failed. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoginFailure {
    #[error("That isn't a valid server address")]
    InvalidUrl,
    #[error("Can't reach the server")]
    Unreachable,
    #[error("That server doesn't look like Rocket.Chat")]
    NotRocketChat,
    #[error("Rocket.Chat {found} is too old, {minimum} or newer is needed")]
    UnsupportedVersion { found: String, minimum: String },
    #[error("Wrong username or password")]
    InvalidCredentials,
    /// Any other server error, with its message.
    #[error("The server reported an error: {0}")]
    Server(String),
}

impl From<ApiError> for LoginFailure {
    fn from(error: ApiError) -> Self {
        match error {
            ApiError::InvalidUrl(_) => Self::InvalidUrl,
            ApiError::Unreachable(_) => Self::Unreachable,
            ApiError::NotRocketChat => Self::NotRocketChat,
            ApiError::InvalidCredentials => Self::InvalidCredentials,
            other => Self::Server(other.to_string()),
        }
    }
}

/// What a successful login leaves behind.
pub(crate) struct Established<A> {
    pub api: A,
    pub credentials: Credentials,
    pub account: Account,
}

/// What the user types may lack the scheme: assume https.
pub(crate) fn normalize_server(input: &str) -> String {
    let input = input.trim();
    if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    }
}

/// Connects, checks the server version and logs in. Doesn't try to log in on a server that's too
/// old.
pub(crate) async fn log_in<C: Connector>(
    connector: &C,
    server: &str,
    username: &str,
    password: &SecretString,
) -> Result<Established<C::Api>, LoginFailure> {
    let server = normalize_server(server);
    let api = connector.connect(&server)?;
    let version = api.version().await?;
    if version < MIN_SERVER_VERSION {
        return Err(LoginFailure::UnsupportedVersion {
            found: version.to_string(),
            minimum: MIN_SERVER_VERSION.to_string(),
        });
    }
    let (credentials, user) = api.login(username, password).await?;
    Ok(Established {
        api,
        credentials,
        account: Account { server, user },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // AUTH-001
    #[test]
    fn normalizes_server_input() {
        assert_eq!(
            normalize_server("chat.example.com"),
            "https://chat.example.com"
        );
        assert_eq!(
            normalize_server("http://127.0.0.1:3000"),
            "http://127.0.0.1:3000"
        );
        assert_eq!(
            normalize_server("  chat.example.com/chat \n"),
            "https://chat.example.com/chat"
        );
    }

    // AUTH-003
    #[test]
    fn failure_texts_are_user_facing() {
        let failure = LoginFailure::UnsupportedVersion {
            found: "7.9".into(),
            minimum: "8.0".into(),
        };
        assert_eq!(
            failure.to_string(),
            "Rocket.Chat 7.9 is too old, 8.0 or newer is needed"
        );
    }
}
