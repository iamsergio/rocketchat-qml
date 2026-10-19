// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the core needs from the outside world. Use cases depend on these traits only; adapters
//! implement them for real I/O and tests implement them with fakes.

use rc_api::ServerVersion;
use secrecy::SecretString;

use crate::domain::{Credentials, User};

/// Why a server call failed. Owned strings only, so that fakes can build and compare them.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error("not a valid server URL: {0}")]
    InvalidUrl(String),
    #[error("can't reach the server: {0}")]
    Unreachable(String),
    #[error("the server doesn't look like Rocket.Chat")]
    NotRocketChat,
    #[error("wrong username or password")]
    InvalidCredentials,
    #[error("the session is no longer valid")]
    Unauthorized,
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
    #[error("unexpected answer from the server: {0}")]
    Unexpected(String),
}

/// Opens a [`ServerApi`] for a URL. Sync: it only validates and builds, no I/O.
pub trait Connector: Send + Sync + 'static {
    type Api: ServerApi;

    /// # Errors
    ///
    /// [`ApiError::InvalidUrl`] if `server` can't be used.
    fn connect(&self, server: &str) -> Result<Self::Api, ApiError>;
}

/// One Rocket.Chat server. Cheap to clone.
pub trait ServerApi: Clone + Send + Sync + 'static {
    fn version(&self) -> impl Future<Output = Result<ServerVersion, ApiError>> + Send;

    fn login(
        &self,
        username: &str,
        password: &SecretString,
    ) -> impl Future<Output = Result<(Credentials, User), ApiError>> + Send;

    /// Revokes the token on the server.
    fn logout(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;
}
