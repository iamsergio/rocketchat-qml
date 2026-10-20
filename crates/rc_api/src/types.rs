// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Public types of the REST API. Plain data: the serde types they come from are in `wire`.

use std::fmt;

use secrecy::SecretString;

/// What a login gives: enough to make authenticated calls and to resume the session later.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub user_id: String,
    /// Debug output doesn't reveal it.
    pub token: SecretString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub username: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LoginResult {
    pub credentials: Credentials,
    pub user: User,
}

/// Server version as `/api/info` reports it. Patch and suffix are ignored: servers only send
/// major.minor to unauthenticated clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServerVersion {
    pub major: u32,
    pub minor: u32,
}

impl ServerVersion {
    /// Reads `8.0`, `8.0.1`, `8.10.0-develop` or `8.9.0-rc.5`. `None` if there's no numeric
    /// major.minor.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let major = parts.next()?.parse().ok()?;
        // The minor can carry a suffix when there's no patch ("8.9-rc.5").
        let minor = parts.next()?.split(['-', '+']).next()?.parse().ok()?;
        Some(Self { major, minor })
    }
}

impl fmt::Display for ServerVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: ServerVersion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomType {
    Channel,
    Private,
    Direct,
    /// Anything else the server sends, e.g. `l` for livechat.
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscription {
    pub room_id: String,
    pub room_type: RoomType,
    /// Empty if the server didn't send one. For direct messages it's the other user's username.
    pub name: String,
    /// The human-readable name, when the server has one.
    pub display_name: Option<String>,
    pub unread: u32,
    pub user_mentions: u32,
    pub group_mentions: u32,
    pub alert: bool,
    pub open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: u32, minor: u32) -> ServerVersion {
        ServerVersion { major, minor }
    }

    // AUTH-003
    #[test]
    fn parses_versions() {
        assert_eq!(ServerVersion::parse("8.0"), Some(version(8, 0)));
        assert_eq!(ServerVersion::parse("8.0.1"), Some(version(8, 0)));
        assert_eq!(ServerVersion::parse("8.10.0-develop"), Some(version(8, 10)));
        assert_eq!(ServerVersion::parse("8.9.0-rc.5"), Some(version(8, 9)));
        assert_eq!(ServerVersion::parse("8.9-rc.5"), Some(version(8, 9)));
    }

    // AUTH-003
    #[test]
    fn rejects_garbage_versions() {
        for text in ["", "8", "abc", "8.x", ".1", "8.", "-1.0"] {
            assert_eq!(ServerVersion::parse(text), None, "{text:?}");
        }
    }

    // AUTH-003
    #[test]
    fn orders_versions_numerically() {
        let develop = ServerVersion::parse("8.10.0-develop").unwrap();
        assert!(develop > ServerVersion::parse("8.9").unwrap());
        assert!(version(9, 0) > develop);
        assert_eq!(version(8, 0).to_string(), "8.0");
    }
}
