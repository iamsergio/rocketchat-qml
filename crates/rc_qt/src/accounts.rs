// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The accounts the hub knows, in the order of their `Added`, each with its own [`Cache`], and the
//! one the view models show: the current account. Plain Rust, so it can be unit-tested without Qt
//! or a [`Client`](rc_core::Client).

use rc_core::Event;
use rc_core::domain::AccountId;
use rc_core::session::{LoginFailure, SessionState};

use crate::cache::Cache;

/// Where an account is, as the account list shows it. Mirrors the `AccountStatus` Qt enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Also before its first `Session` event, which follows its `Added` at once.
    #[default]
    LoggedOut,
    Connecting,
    /// A stored session is being picked up at startup.
    Resuming,
    LoggedIn,
    /// The server refused the token: the account is listed, logged out, until the user logs in
    /// again.
    Expired,
}

/// The status `state` makes an account show, where `None` is the state before the first event.
#[must_use]
pub fn account_status(state: Option<&SessionState>) -> Status {
    match state {
        Some(SessionState::LoggedOut {
            failure: Some(LoginFailure::SessionExpired),
        }) => Status::Expired,
        None | Some(SessionState::LoggedOut { .. }) => Status::LoggedOut,
        Some(SessionState::Connecting) => Status::Connecting,
        Some(SessionState::Resuming) => Status::Resuming,
        Some(SessionState::LoggedIn(_)) => Status::LoggedIn,
    }
}

/// One row of the `AccountList` model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountView {
    pub id: String,
    /// The URL of the account's server; empty until the account has an identity.
    pub server: String,
    /// The host and port of `server`.
    pub host: String,
    pub username: String,
    pub status: Status,
    /// The realtime connection is up. It can't be while the account is logged out.
    pub connected: bool,
    /// How many rooms show as unread.
    pub unread_rooms: u32,
    /// The sum of the mention badges of the rooms.
    pub mentions: u32,
    /// The view models show this account.
    pub current: bool,
}

/// A change to the rows of the account list, applied to the model's rows one to one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountChange {
    Inserted { index: usize, view: AccountView },
    Updated { index: usize, view: AccountView },
    Removed { index: usize },
}

impl AccountChange {
    /// Applies the change to `rows`.
    pub fn apply(&self, rows: &mut Vec<AccountView>) {
        match self {
            Self::Inserted { index, view } => rows.insert(*index, view.clone()),
            Self::Updated { index, view } => rows[*index].clone_from(view),
            Self::Removed { index } => {
                rows.remove(*index);
            }
        }
    }
}

#[derive(Debug)]
struct Entry {
    id: AccountId,
    cache: Cache,
}

/// What [`Accounts::apply`] says about the event it took in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The account's row in the list.
    pub row: usize,
    /// The account is the current one: its events are for the view models.
    pub current: bool,
}

/// The accounts in the order of their `Added`, and which one is current.
#[derive(Debug)]
pub struct Accounts {
    entries: Vec<Entry>,
    /// `None` only before the first account is added, and between the removal of the last one and
    /// the next addition.
    current: Option<AccountId>,
}

/// What the view models show when there is no current account.
static NO_CACHE: Cache = Cache::new();

impl Default for Accounts {
    fn default() -> Self {
        Self::new()
    }
}

impl Accounts {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            current: None,
        }
    }

    fn row(&self, id: &AccountId) -> Option<usize> {
        self.entries.iter().position(|entry| &entry.id == id)
    }

    /// Adds the account, with an empty cache, unless it is known. Returns the row it took, if it
    /// did. The first account added while there is no current one becomes current.
    pub fn added(&mut self, id: AccountId) -> Option<usize> {
        if self.row(&id).is_some() {
            return None;
        }
        if self.current.is_none() {
            self.current = Some(id.clone());
        }
        self.entries.push(Entry {
            id,
            cache: Cache::new(),
        });
        Some(self.entries.len() - 1)
    }

    /// Removes the account. Returns the row it had, if it was known. If it was current, the first
    /// account left becomes current.
    pub fn removed(&mut self, id: &AccountId) -> Option<usize> {
        let row = self.row(id)?;
        self.entries.remove(row);
        if self.current.as_ref() == Some(id) {
            self.current = self.entries.first().map(|entry| entry.id.clone());
        }
        Some(row)
    }

    /// Gives the event to the account's cache. `None` if the account isn't known.
    pub fn apply(&mut self, id: &AccountId, event: &Event) -> Option<Applied> {
        let row = self.row(id)?;
        self.entries[row].cache.apply(event);
        Some(Applied {
            row,
            current: self.current.as_ref() == Some(id),
        })
    }

    /// Makes the account current. Returns whether that changed anything: not if it is current
    /// already or isn't known.
    pub fn select(&mut self, id: &AccountId) -> bool {
        if self.current.as_ref() == Some(id) || self.row(id).is_none() {
            return false;
        }
        self.current = Some(id.clone());
        true
    }

    #[must_use]
    pub const fn current(&self) -> Option<&AccountId> {
        self.current.as_ref()
    }

    /// What the view models show: the current account's cache, or an empty one.
    #[must_use]
    pub fn current_cache(&self) -> &Cache {
        self.current
            .as_ref()
            .and_then(|id| self.row(id))
            .map_or(&NO_CACHE, |row| &self.entries[row].cache)
    }

    /// The account's row, as the account list shows it.
    #[must_use]
    pub fn view(&self, id: &AccountId) -> Option<(usize, AccountView)> {
        let row = self.row(id)?;
        let current = self.current.as_ref() == Some(id);
        Some((row, self.entries[row].cache.account_view(id, current)))
    }

    /// All the rows, in order.
    #[must_use]
    pub fn views(&self) -> Vec<AccountView> {
        self.entries
            .iter()
            .map(|entry| {
                let current = self.current.as_ref() == Some(&entry.id);
                entry.cache.account_view(&entry.id, current)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Account, Identity, User};
    use rc_core::rooms::Unread;
    use rc_core::{Connection, Event};

    use super::*;

    fn id(name: &str) -> AccountId {
        AccountId::from(name.to_owned())
    }

    fn identity(server: &str, username: &str) -> Event {
        Event::Identity(Identity {
            server: server.to_owned(),
            username: username.to_owned(),
        })
    }

    fn logged_in(server: &str, username: &str) -> Event {
        Event::Session(SessionState::LoggedIn(Account {
            server: server.to_owned(),
            user: User {
                id: "u".to_owned().into(),
                username: username.to_owned(),
                display_name: None,
            },
        }))
    }

    fn logged_out() -> Event {
        Event::Session(SessionState::LoggedOut { failure: None })
    }

    /// Accounts `a`, `b` and `c`, added in that order.
    fn three() -> Accounts {
        let mut accounts = Accounts::new();
        for name in ["a", "b", "c"] {
            accounts.added(id(name));
        }
        accounts
    }

    fn ids(accounts: &Accounts) -> Vec<String> {
        accounts.views().into_iter().map(|view| view.id).collect()
    }

    // ACCT-001
    #[test]
    fn accounts_are_listed_in_the_order_they_were_added() {
        let accounts = three();
        assert_eq!(ids(&accounts), ["a", "b", "c"]);
    }

    // ACCT-001
    #[test]
    fn adding_a_known_account_changes_nothing() {
        let mut accounts = Accounts::new();
        assert_eq!(accounts.added(id("a")), Some(0));
        assert_eq!(accounts.added(id("b")), Some(1));
        accounts.apply(&id("b"), &logged_out());
        let before = accounts.views();
        assert_eq!(accounts.added(id("b")), None);
        assert_eq!(accounts.views(), before);
    }

    // ACCT-001
    #[test]
    fn the_first_account_becomes_current_and_the_next_ones_do_not() {
        let mut accounts = Accounts::new();
        assert_eq!(accounts.current(), None);
        accounts.added(id("a"));
        accounts.added(id("b"));
        assert_eq!(accounts.current(), Some(&id("a")));
        let current: Vec<bool> = accounts.views().iter().map(|view| view.current).collect();
        assert_eq!(current, [true, false]);
    }

    // ACCT-005
    #[test]
    fn removing_an_account_leaves_the_others_as_they_were() {
        let mut accounts = three();
        accounts.apply(&id("c"), &identity("http://c", "carol"));
        assert_eq!(accounts.removed(&id("b")), Some(1));
        assert_eq!(ids(&accounts), ["a", "c"]);
        assert_eq!(accounts.current(), Some(&id("a")));
        assert_eq!(accounts.view(&id("c")).unwrap().1.username, "carol");
        assert_eq!(accounts.removed(&id("b")), None);
    }

    // ACCT-005
    #[test]
    fn removing_the_current_account_makes_the_first_one_left_current() {
        let mut accounts = three();
        assert!(accounts.select(&id("b")));
        accounts.removed(&id("b"));
        assert_eq!(accounts.current(), Some(&id("a")));
        accounts.removed(&id("a"));
        assert_eq!(accounts.current(), Some(&id("c")));
    }

    // ACCT-005
    #[test]
    fn removing_the_last_account_leaves_none_current_until_the_next_is_added() {
        let mut accounts = Accounts::new();
        accounts.added(id("a"));
        accounts.apply(&id("a"), &logged_in("http://h", "alice"));
        accounts.removed(&id("a"));
        assert_eq!(accounts.current(), None);
        assert!(accounts.views().is_empty());
        assert_eq!(accounts.current_cache().session_view().username, "");
        accounts.added(id("b"));
        assert_eq!(accounts.current(), Some(&id("b")));
    }

    // ACCT-003
    #[test]
    fn selecting_an_account_makes_it_current() {
        let mut accounts = three();
        assert!(accounts.select(&id("c")));
        assert_eq!(accounts.current(), Some(&id("c")));
        let current: Vec<bool> = accounts.views().iter().map(|view| view.current).collect();
        assert_eq!(current, [false, false, true]);
    }

    // ACCT-003
    #[test]
    fn selecting_the_current_or_an_unknown_account_changes_nothing() {
        let mut accounts = three();
        assert!(!accounts.select(&id("a")));
        assert!(!accounts.select(&id("nobody")));
        assert_eq!(accounts.current(), Some(&id("a")));
    }

    // ACCT-003
    #[test]
    fn the_current_cache_is_the_current_accounts() {
        let mut accounts = three();
        accounts.apply(&id("a"), &logged_in("http://a", "alice"));
        accounts.apply(&id("b"), &logged_in("http://b", "bob"));
        assert_eq!(accounts.current_cache().session_view().username, "alice");
        accounts.select(&id("b"));
        assert_eq!(accounts.current_cache().session_view().username, "bob");
        assert_eq!(accounts.current_cache().session_view().host, "b");
    }

    // ACCT-003
    #[test]
    fn events_go_to_their_accounts_cache_and_say_whether_it_is_current() {
        let mut accounts = three();
        assert_eq!(
            accounts.apply(&id("a"), &logged_in("http://a", "alice")),
            Some(Applied {
                row: 0,
                current: true
            })
        );
        assert_eq!(
            accounts.apply(&id("c"), &logged_in("http://c", "carol")),
            Some(Applied {
                row: 2,
                current: false
            })
        );
        assert_eq!(accounts.apply(&id("nobody"), &logged_out()), None);
        // The background account kept the event; the current cache didn't see it.
        assert_eq!(accounts.current_cache().session_view().username, "alice");
        accounts.select(&id("c"));
        assert_eq!(accounts.current_cache().session_view().username, "carol");
    }

    // ACCT-003
    #[test]
    fn an_account_without_a_current_one_shows_an_empty_cache() {
        let accounts = Accounts::new();
        let view = accounts.current_cache().session_view();
        assert_eq!(view.username, "");
        assert!(accounts.current_cache().viewer().username.is_none());
    }

    // ACCT-001
    #[test]
    fn the_row_says_who_the_account_is_even_when_logged_out() {
        let mut accounts = three();
        let view = |accounts: &Accounts| accounts.view(&id("b")).unwrap().1;
        assert_eq!(view(&accounts).server, "");
        accounts.apply(&id("b"), &identity("https://chat.example.com:8443/", "bob"));
        accounts.apply(&id("b"), &logged_out());
        let row = view(&accounts);
        assert_eq!(row.id, "b");
        assert_eq!(row.server, "https://chat.example.com:8443/");
        assert_eq!(row.host, "chat.example.com:8443");
        assert_eq!(row.username, "bob");
        assert_eq!(row.status, Status::LoggedOut);
        assert!(!row.connected);
    }

    // ACCT-001
    #[test]
    fn the_identity_stays_across_session_changes() {
        let mut accounts = three();
        accounts.apply(&id("a"), &identity("http://h", "alice"));
        for event in [
            Event::Session(SessionState::Resuming),
            logged_in("http://h", "alice"),
            logged_out(),
            Event::Session(SessionState::Connecting),
        ] {
            accounts.apply(&id("a"), &event);
            let row = accounts.view(&id("a")).unwrap().1;
            assert_eq!(row.username, "alice", "{event:?}");
            assert_eq!(row.server, "http://h", "{event:?}");
        }
    }

    // ACCT-006
    #[test]
    fn the_status_follows_the_session() {
        let failure = |failure| {
            Some(SessionState::LoggedOut {
                failure: Some(failure),
            })
        };
        for (state, status) in [
            (None, Status::LoggedOut),
            (
                Some(SessionState::LoggedOut { failure: None }),
                Status::LoggedOut,
            ),
            (failure(LoginFailure::InvalidCredentials), Status::LoggedOut),
            (failure(LoginFailure::AlreadyAdded), Status::LoggedOut),
            (failure(LoginFailure::SessionExpired), Status::Expired),
            (Some(SessionState::Connecting), Status::Connecting),
            (Some(SessionState::Resuming), Status::Resuming),
        ] {
            assert_eq!(account_status(state.as_ref()), status, "{state:?}");
        }
        let account = Account {
            server: "http://h".to_owned(),
            user: User {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
        };
        assert_eq!(
            account_status(Some(&SessionState::LoggedIn(account))),
            Status::LoggedIn
        );
    }

    // ACCT-006
    #[test]
    fn an_expired_account_stays_listed_with_who_it_is() {
        let mut accounts = three();
        accounts.apply(&id("b"), &identity("http://h", "bob"));
        accounts.apply(&id("b"), &logged_in("http://h", "bob"));
        accounts.apply(
            &id("b"),
            &Event::Session(SessionState::LoggedOut {
                failure: Some(LoginFailure::SessionExpired),
            }),
        );
        let row = accounts.view(&id("b")).unwrap().1;
        assert_eq!(row.status, Status::Expired);
        assert_eq!(row.username, "bob");
        assert_eq!(accounts.views().len(), 3);
    }

    // ACCT-004
    #[test]
    fn the_row_shows_the_unread_totals_of_a_background_account() {
        let mut accounts = three();
        accounts.apply(&id("b"), &logged_in("http://h", "bob"));
        accounts.apply(
            &id("b"),
            &Event::Unread(Unread {
                rooms: 3,
                mentions: 5,
            }),
        );
        let row = accounts.view(&id("b")).unwrap().1;
        assert_eq!((row.unread_rooms, row.mentions), (3, 5));
        assert!(!row.current);
        accounts.apply(&id("b"), &Event::Unread(Unread::default()));
        let row = accounts.view(&id("b")).unwrap().1;
        assert_eq!((row.unread_rooms, row.mentions), (0, 0));
    }

    // ACCT-004
    #[test]
    fn connected_is_the_realtime_connection_of_a_logged_in_account() {
        use std::time::Duration;

        let mut accounts = three();
        let connected = |accounts: &Accounts| accounts.view(&id("a")).unwrap().1.connected;
        assert!(!connected(&accounts));
        accounts.apply(&id("a"), &logged_in("http://h", "alice"));
        assert!(connected(&accounts));
        accounts.apply(
            &id("a"),
            &Event::Connection(Connection::Waiting {
                retry_in: Duration::from_secs(2),
            }),
        );
        assert!(!connected(&accounts));
        accounts.apply(&id("a"), &Event::Connection(Connection::Reconnecting));
        assert!(!connected(&accounts));
        accounts.apply(&id("a"), &Event::Connection(Connection::Connected));
        assert!(connected(&accounts));
        accounts.apply(&id("a"), &logged_out());
        assert!(!connected(&accounts));
    }

    // ACCT-001
    #[test]
    fn changes_to_the_rows_apply_to_a_list_of_rows() {
        let view = |name: &str, current| AccountView {
            id: name.to_owned(),
            current,
            ..AccountView::default()
        };
        let mut rows = Vec::new();
        for change in [
            AccountChange::Inserted {
                index: 0,
                view: view("a", true),
            },
            AccountChange::Inserted {
                index: 1,
                view: view("b", false),
            },
            AccountChange::Updated {
                index: 1,
                view: view("b", true),
            },
            AccountChange::Updated {
                index: 0,
                view: view("a", false),
            },
            AccountChange::Removed { index: 0 },
        ] {
            change.apply(&mut rows);
        }
        assert_eq!(rows, [view("b", true)]);
    }

    // ACCT-003
    #[test]
    fn the_changes_the_accounts_describe_keep_a_list_of_rows_equal_to_their_views() {
        let mut accounts = Accounts::new();
        let mut rows: Vec<AccountView> = Vec::new();
        for name in ["a", "b", "c"] {
            let id = id(name);
            let row = accounts.added(id.clone()).unwrap();
            let (index, view) = accounts.view(&id).unwrap();
            assert_eq!(index, row);
            AccountChange::Inserted { index, view }.apply(&mut rows);
        }
        accounts.apply(&id("b"), &logged_in("http://h", "bob"));
        let (index, view) = accounts.view(&id("b")).unwrap();
        AccountChange::Updated { index, view }.apply(&mut rows);
        let removed = accounts.removed(&id("a")).unwrap();
        AccountChange::Removed { index: removed }.apply(&mut rows);
        for name in ["b", "c"] {
            let (index, view) = accounts.view(&id(name)).unwrap();
            AccountChange::Updated { index, view }.apply(&mut rows);
        }
        assert_eq!(rows, accounts.views());
    }
}
