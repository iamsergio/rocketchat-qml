// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

/// What can go wrong talking to a Rocket.Chat server over REST.
///
/// Any `reqwest` failure that isn't an HTTP status (connect, DNS, TLS, timeout, a broken body,
/// client setup) is reported as [`Error::Unreachable`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a valid server URL: {0}")]
    InvalidUrl(String),
    #[error("can't reach the server")]
    Unreachable(#[source] reqwest::Error),
    #[error("the server doesn't look like Rocket.Chat")]
    NotRocketChat,
    #[error("wrong username or password")]
    InvalidCredentials,
    #[error("the session is no longer valid, log in again")]
    Unauthorized,
    /// Any other non-2xx answer. `message` comes from the Rocket.Chat error body when there is one.
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
    /// A kind of room that has no history call, e.g. livechat.
    #[error("rooms of type {0:?} have no history")]
    UnsupportedRoomType(String),
    #[error("unexpected answer from the server")]
    Decode(#[source] serde_json::Error),
}
