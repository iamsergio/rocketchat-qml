// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

/// What can go wrong talking to a Rocket.Chat server, over REST or the realtime connection.
///
/// Any `reqwest` failure that isn't an HTTP status (connect, DNS, TLS, timeout, a broken body,
/// client setup) is reported as [`Error::Unreachable`]; any failure of the realtime connection's
/// WebSocket as [`Error::WebSocket`].
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
    /// The realtime connection couldn't be opened, or broke. Not answering in time is an
    /// [`Io`](tokio_tungstenite::tungstenite::Error::Io) error of kind `TimedOut`.
    #[error("the realtime connection failed")]
    WebSocket(#[source] Box<tokio_tungstenite::tungstenite::Error>),
    /// The realtime connection ended before the answer came, or had ended already.
    #[error("the realtime connection is closed")]
    ConnectionClosed,
    /// The server refused a realtime call or subscription. `code` is its error code, such as `404`
    /// or `error-not-allowed`.
    #[error("the server refused ({code}): {message}")]
    Refused { code: String, message: String },
}
