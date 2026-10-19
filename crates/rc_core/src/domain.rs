// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the application knows about, independent of how the server spells it.

use std::fmt;

use secrecy::SecretString;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UserId(String);

impl UserId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for UserId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub display_name: Option<String>,
}

/// Enough to make authenticated calls and to resume a session later. Debug output doesn't reveal
/// the token.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub user_id: UserId,
    pub token: SecretString,
}

/// Who is logged in, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The normalized URL that was used to log in.
    pub server: String,
    pub user: User,
}

/// What is kept between runs to resume a session.
#[derive(Clone, Debug)]
pub struct StoredSession {
    /// The normalized URL that was used to log in.
    pub server: String,
    pub credentials: Credentials,
}
