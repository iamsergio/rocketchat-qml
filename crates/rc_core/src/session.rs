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
    /// A login is in flight.
    Connecting,
    /// A stored session is being picked up at startup. Apart from the name it's the same as
    /// `Connecting`, so that a frontend can tell it from a login the user started.
    Resuming,
    LoggedIn(Account),
}

impl SessionState {
    /// Whether a login or a resume is in flight.
    #[must_use]
    pub fn attempting(&self) -> bool {
        matches!(self, Self::Connecting | Self::Resuming)
    }
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
    #[error("Your session has expired, log in again")]
    SessionExpired,
    /// The user logged in is another account's already.
    #[error("You have added that account already")]
    AlreadyAdded,
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
    /// What the links of quotes must begin with: the server's `Site_Url`, or the address that was
    /// used to log in if the server has none or didn't say.
    pub site_url: String,
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

/// Connects and checks the server version, and asks for its `Site_Url` at the same time (the
/// answer is the login's address if the server has none, or the request fails, which only
/// warrants a warning).
async fn connect_checked<C: Connector>(
    connector: &C,
    server: &str,
) -> Result<(C::Api, String), LoginFailure> {
    let api = connector.connect(server)?;
    let (version, site_url) = tokio::join!(api.version(), api.site_url());
    let version = version?;
    if version < MIN_SERVER_VERSION {
        return Err(LoginFailure::UnsupportedVersion {
            found: version.to_string(),
            minimum: MIN_SERVER_VERSION.to_string(),
        });
    }
    let site_url = site_url.unwrap_or_else(|error| {
        tracing::warn!(%error, "can't read the server's own address");
        None
    });
    Ok((api, site_url.unwrap_or_else(|| server.to_owned())))
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
    let (api, site_url) = connect_checked(connector, &server).await?;
    let (credentials, user) = api.login(username, password).await?;
    Ok(Established {
        api,
        credentials,
        account: Account { server, user },
        site_url,
    })
}

/// Picks up a stored session: same checks as a login, then asks the server who the credentials
/// belong to. [`LoginFailure::SessionExpired`] means the server doesn't accept them any more.
pub(crate) async fn resume<C: Connector>(
    connector: &C,
    server: &str,
    credentials: &Credentials,
) -> Result<Established<C::Api>, LoginFailure> {
    let (api, site_url) = connect_checked(connector, server).await?;
    let user = api.me(credentials).await.map_err(|error| match error {
        ApiError::Unauthorized => LoginFailure::SessionExpired,
        other => other.into(),
    })?;
    Ok(Established {
        api,
        credentials: credentials.clone(),
        account: Account {
            server: server.to_owned(),
            user,
        },
        site_url,
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
