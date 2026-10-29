// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The search of a room's members, which the composer's mention popup lists (see
//! `docs/design/mentions.md`).

use crate::directory::Found;
use crate::domain::{RoomId, User};
use crate::ports::ApiError;

/// How many members a search shows. The rest is only counted. The request asks for one more, as
/// the user may be among them and is left out.
pub const MEMBERS_PER_SEARCH: usize = 6;

/// How long the search waits after the last change of the query before it asks the server, so that
/// typing a name doesn't ask once per letter.
pub(crate) const DEBOUNCE: jiff::SignedDuration = jiff::SignedDuration::from_millis(300);

/// A member of a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub id: String,
    pub username: String,
    /// The display name, if the user has one.
    pub name: Option<String>,
}

/// Why the members couldn't be searched. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MemberError {
    #[error("Can't reach the server")]
    Unreachable,
    /// Any other error, with its message.
    #[error("The server reported an error: {0}")]
    Server(String),
}

impl From<&ApiError> for MemberError {
    fn from(error: &ApiError) -> Self {
        match error {
            ApiError::Unreachable(_) => Self::Unreachable,
            other => Self::Server(other.to_string()),
        }
    }
}

/// How far a member search is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberSearchState {
    /// The query was typed and the answer is not here yet: the search waits for the typing to stop
    /// (`Tuning::members_debounce`), then asks the server. A frontend keeps showing the previous
    /// results meanwhile.
    Searching,
    /// The members whose username or name contain the query, in the server's order: owners,
    /// moderators, then by name. At most [`MEMBERS_PER_SEARCH`].
    Found {
        members: Vec<Member>,
        /// How many members match in all.
        total: u32,
    },
    /// The server couldn't be asked, or refused. Searching for the same text again retries.
    Failed(MemberError),
}

/// What the search of a room's members holds, as the frontend is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberSearch {
    pub room: RoomId,
    /// The text after the `@`, as typed.
    pub query: String,
    pub state: MemberSearchState,
}

/// The page without the user, who has no use for mentioning themselves: `page` came from a request
/// for `MEMBERS_PER_SEARCH + 1` members, so it still fills the rows if it held the user. The total
/// loses one if the user matches `query` (as the server matches: a substring of the username or
/// of the name, ignoring case), whether or not they were on the page.
#[must_use]
pub(crate) fn without_user(page: Found<Member>, user: &User, query: &str) -> Found<Member> {
    let query = query.to_lowercase();
    let matches = |text: &str| text.to_lowercase().contains(&query);
    let own = matches(&user.username) || user.display_name.as_deref().is_some_and(matches);
    let mut items = page.items;
    items.retain(|member| member.username != user.username);
    items.truncate(MEMBERS_PER_SEARCH);
    Found {
        items,
        total: if own {
            page.total.saturating_sub(1)
        } else {
            page.total
        },
    }
}

/// The members of a direct message that match `query`: the other person, if their username or
/// name contains it, ignoring case. The server doesn't list a direct message's members.
#[must_use]
pub(crate) fn direct_members(peer: Option<Member>, query: &str) -> Vec<Member> {
    let query = query.to_lowercase();
    peer.into_iter()
        .filter(|member| {
            member.username.to_lowercase().contains(&query)
                || member
                    .name
                    .as_ref()
                    .is_some_and(|name| name.to_lowercase().contains(&query))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Member {
        Member {
            id: String::new(),
            username: "alice".to_owned(),
            name: Some("Alice Doe".to_owned()),
        }
    }

    // ACT-013
    #[test]
    fn the_other_person_of_a_direct_message_matches_by_username_or_name_in_any_case() {
        for query in ["", "LIC", "doe", "alice"] {
            assert_eq!(direct_members(Some(alice()), query), [alice()], "{query}");
        }
    }

    // ACT-013
    #[test]
    fn nobody_matches_what_is_in_neither_name() {
        assert_eq!(direct_members(Some(alice()), "bob"), []);
        assert_eq!(direct_members(None, ""), []);
    }
}
