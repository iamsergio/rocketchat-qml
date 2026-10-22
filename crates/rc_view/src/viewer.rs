// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Who looks at the messages and when it is, which the views and the renderers take as parameters.
//! Plain Rust, so it can be unit-tested without Qt.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::UserId;
use rc_core::emoji::CustomEmojis;
use rc_core::policy::MessagePolicy;

/// Whoever looks at the messages: it decides which mentions stand out, which emoji exist and where
/// the links of attachments lead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Viewer {
    /// The logged-in user, if any.
    pub username: Option<String>,
    /// The URL the user logged in with: attachment links are relative to it.
    pub server: Option<String>,
    pub emoji: CustomEmojis,
    /// The logged-in user's id: their messages are the ones they may change.
    pub user_id: Option<UserId>,
    /// What the server lets users do with their own messages.
    pub policy: MessagePolicy,
}

/// Where times are read and when it is now, so that tests don't depend on either.
#[derive(Clone, Copy)]
pub struct Clock<'a> {
    pub zone: &'a TimeZone,
    pub now: Timestamp,
}

/// Whether `url` is an `http` or `https` one, which is what an image may load from.
#[must_use]
pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}
