// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Decides what a frontend changes on screen when the core sends an event, or when the user adds,
//! selects or removes an account. Plain Rust that knows no GUI toolkit: a frontend takes in
//! what happened and carries out, in order, what it is told to show, in its own way (queueing to
//! Qt view models, or calling into the Slint event loop).
//!
//! The switchboard owns the [`Accounts`]: the accounts in the order of their `Added`, each with
//! its cache, and the current one. The first account added is current. Selecting another one, or
//! removing the current one, switches: every view gets a full snapshot of the new current
//! account's cache. Events of the current account are shown by the views; those of the others
//! only update their cache, and their row in the account list. A logout removes the account, and
//! the core adds a new logged-out one if it was the last, so the app goes on with that one.
//!
//! What fills the main area is a [`Screen`], apart from the current account: the GitHub page can
//! be shown over it. The current account stays current then, and its events still reach the
//! views, so going back needs no snapshot; only its tile (the row's `current`) stops looking
//! current. Selecting or adding an account, or the GitHub tile, change the screen.

use rc_core::Event;
use rc_core::domain::AccountId;

use crate::accounts::{AccountChange, Accounts};
use crate::session_view::SessionView;

/// What fills the main area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    /// The current account.
    #[default]
    Account,
    /// The GitHub pull requests.
    GitHub,
}

/// What the user saw before a change, for [`Switchboard::switched_from`] to compare with.
struct Seen {
    current: Option<AccountId>,
    /// The account whose tile looks current.
    lit: Option<AccountId>,
    screen: Screen,
}

/// What a frontend changes on screen, in this order, after the switchboard took something in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    /// A change to the account list's rows.
    AccountRow(AccountChange),
    /// The session view of the current account, as it was when the switchboard said so: what it
    /// shows depends on how many accounts there are. Taken then, because a switch that follows
    /// must not show the next account's session before its first `Session` event.
    SessionView(SessionView),
    /// Everything the current account's cache says, replacing what every view showed (the
    /// snapshot a view gets when it registers).
    All,
    /// An event of the current account, which its cache has taken in: the views show what it
    /// changed.
    Event(Event),
    /// The main area shows another screen, after the rows and the snapshot that came before it.
    Screen(Screen),
}

/// The accounts, and the rules that turn what happens to them into [`Shown`].
#[derive(Debug, Default)]
pub struct Switchboard {
    /// The accounts, each with its cache, and the current one: the one the views show and the
    /// commands go to.
    accounts: Accounts,
    /// What fills the main area.
    screen: Screen,
}

impl Switchboard {
    /// A switchboard with no accounts.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            accounts: Accounts::new(),
            screen: Screen::Account,
        }
    }

    /// What fills the main area.
    #[must_use]
    pub const fn screen(&self) -> Screen {
        self.screen
    }

    fn seen(&self) -> Seen {
        Seen {
            current: self.accounts.current().cloned(),
            lit: self.accounts.on_screen().cloned(),
            screen: self.screen,
        }
    }

    /// Shows the GitHub page over the current account. Nothing if it is shown already.
    pub fn show_github(&mut self) -> Vec<Shown> {
        let mut shown = Vec::new();
        if self.screen == Screen::GitHub {
            return shown;
        }
        let before = self.seen();
        self.set_screen(Screen::GitHub);
        self.switched_from(&before, &mut shown);
        shown
    }

    fn set_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.accounts.cover(screen == Screen::GitHub);
    }

    /// The accounts, for the views to read what they show.
    #[must_use]
    pub const fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    /// F5 (ROOM-012): calls `release` for every account whose cache says it holds channel news
    /// back, and returns whether it called it at all. The frontend shows its "No new notifications"
    /// toast when it did not. The cache decides at once, with no round trip to the core. The
    /// race is benign: news held in the core whose `HoldingNews` is not in the cache yet gives a
    /// toast, and the next batch shows the news.
    pub fn release_news(&self, mut release: impl FnMut(&AccountId)) -> bool {
        let mut sent = false;
        for id in self.accounts.holding_news() {
            tracing::info!(account = %id, "F5: the cache says the account holds news");
            release(id);
            sent = true;
        }
        if !sent {
            tracing::info!("F5: no account's cache holds news");
        }
        sent
    }

    /// Takes in an event of the core, and says what to show. Nothing for the event of an account
    /// that isn't known, other than `Added`.
    pub fn dispatch(&mut self, account: &AccountId, event: Event) -> Vec<Shown> {
        let before = self.seen();
        let mut shown = Vec::new();
        match event {
            Event::Added => self.add_row(account, &mut shown),
            Event::Removed => {
                // The logout's events came before, so the cache showed logged out already.
                if let Some(index) = self.accounts.removed(account) {
                    shown.push(Shown::AccountRow(AccountChange::Removed { index }));
                    self.show_session_count(&mut shown);
                }
            }
            _ => {
                // The first state of an account that became current before it had one: show it
                // whole.
                let first_state = matches!(event, Event::Session(_))
                    && before.current.as_ref() == Some(account)
                    && !self.accounts.current_cache().session_known();
                let Some(applied) = self.accounts.apply(account, &event) else {
                    tracing::debug!(%account, "event of an unknown account ignored");
                    return shown;
                };
                if matches!(
                    event,
                    Event::Identity(_)
                        | Event::Unread(_)
                        | Event::Session(_)
                        | Event::Connection(_)
                ) {
                    self.update_row(account, &mut shown);
                }
                if applied.current && first_state {
                    shown.push(Shown::All);
                } else if applied.current {
                    shown.push(Shown::Event(event));
                } else if matches!(
                    event,
                    Event::MessageAction { .. }
                        | Event::RoomAction { .. }
                        | Event::DirectoryAction { .. }
                ) {
                    tracing::debug!(%account, "answer of another account dropped");
                }
                return shown;
            }
        }
        self.switched_from(&before, &mut shown);
        shown
    }

    /// Makes the account current and shows it, also when the GitHub page covers it. Nothing to
    /// show if it is on screen already or isn't known.
    pub fn select(&mut self, id: &AccountId) -> Vec<Shown> {
        let before = self.seen();
        let mut shown = Vec::new();
        let known = self.accounts.select(id) || self.accounts.current() == Some(id);
        if known {
            self.set_screen(Screen::Account);
            self.switched_from(&before, &mut shown);
        }
        shown
    }

    /// Lists the account, which the core just created, and makes it current, so that the login
    /// page shows for it. The core's `Added` for it is a no-op afterwards.
    pub fn add(&mut self, id: &AccountId) -> Vec<Shown> {
        let before = self.seen();
        let mut shown = Vec::new();
        self.add_row(id, &mut shown);
        self.accounts.select(id);
        self.set_screen(Screen::Account);
        self.switched_from(&before, &mut shown);
        shown
    }

    /// Forgets every account, when the core stops.
    pub fn clear(&mut self) {
        self.accounts = Accounts::new();
        self.accounts.cover(self.screen == Screen::GitHub);
    }

    /// Adds the account, unless it is known, and lists it.
    fn add_row(&mut self, id: &AccountId, shown: &mut Vec<Shown>) {
        if self.accounts.added(id.clone()).is_some()
            && let Some((index, view)) = self.accounts.view(id)
        {
            shown.push(Shown::AccountRow(AccountChange::Inserted { index, view }));
            self.show_session_count(shown);
        }
    }

    /// Shows the session view that the number of accounts changed, if it shows an account that
    /// has said what state it is in. Whether other accounts exist is part of what it shows.
    fn show_session_count(&self, shown: &mut Vec<Shown>) {
        if self.accounts.current_cache().session_known() {
            shown.push(Shown::SessionView(self.accounts.session_view()));
        }
    }

    /// Updates the account's row in the account list.
    fn update_row(&self, id: &AccountId, shown: &mut Vec<Shown>) {
        if let Some((index, view)) = self.accounts.view(id) {
            shown.push(Shown::AccountRow(AccountChange::Updated { index, view }));
        }
    }

    /// Brings the views and the account list up to date if the current account, the one that
    /// looks current or the screen is not what `before` saw. Call it after every change.
    fn switched_from(&self, before: &Seen, shown: &mut Vec<Shown>) {
        let current = self.accounts.current().cloned();
        let lit = self.accounts.on_screen().cloned();
        // The two rows whose `current` changed. The one that was current may be gone.
        if lit != before.lit {
            for id in before.lit.iter().chain(&lit) {
                self.update_row(id, shown);
            }
        }
        // With no account left, the views keep showing the last one, logged out, until the next
        // account is added. An account added a moment ago is shown with its first `Session`
        // event (see `dispatch`): showing it before would flash the startup page.
        if current != before.current
            && current.is_some()
            && self.accounts.current_cache().session_known()
        {
            shown.push(Shown::All);
        }
        if self.screen != before.screen {
            shown.push(Shown::Screen(self.screen));
        }
    }
}

#[cfg(test)]
mod tests {
    use rc_core::Connection;
    use rc_core::directory::{ActionState, Directory, DirectoryAction};
    use rc_core::domain::{Account, Identity, User};
    use rc_core::policy::{ActionError, MessageAction};
    use rc_core::rooms::Unread;
    use rc_core::session::SessionState;

    use super::*;

    fn id(name: &str) -> AccountId {
        AccountId::from(name.to_owned())
    }

    fn logged_out() -> Event {
        Event::Session(SessionState::LoggedOut { failure: None })
    }

    fn logged_in(username: &str) -> Event {
        Event::Session(SessionState::LoggedIn(Account {
            server: "http://h".to_owned(),
            user: User {
                id: "u".to_owned().into(),
                username: username.to_owned(),
                display_name: None,
            },
        }))
    }

    fn identity() -> Event {
        Event::Identity(Identity {
            server: "http://h".to_owned(),
            username: "u".to_owned(),
        })
    }

    /// Accounts `a`, `b` and `c` added, each with a session, `a` current.
    fn three() -> Switchboard {
        let mut board = Switchboard::new();
        for name in ["a", "b", "c"] {
            board.dispatch(&id(name), Event::Added);
            board.dispatch(&id(name), logged_out());
        }
        board
    }

    fn is_updated(shown: &Shown, row: usize) -> bool {
        matches!(shown, Shown::AccountRow(AccountChange::Updated { index, .. }) if *index == row)
    }

    // ROOM-012
    #[test]
    fn f5_releases_only_the_accounts_that_hold_news() {
        let mut board = three();
        let mut sent = Vec::new();
        assert!(!board.release_news(|id| sent.push(id.clone())));
        assert!(sent.is_empty());

        // `b` holds, in the background; `a` is current and does not.
        board.dispatch(&id("b"), Event::HoldingNews(true));
        assert!(board.release_news(|id| sent.push(id.clone())));
        assert_eq!(sent, [id("b")]);

        // The core says it released: nothing is held again.
        board.dispatch(&id("b"), Event::HoldingNews(false));
        sent.clear();
        assert!(!board.release_news(|id| sent.push(id.clone())));
        assert!(sent.is_empty());
    }

    // ACCT-001
    #[test]
    fn added_lists_the_row() {
        let mut board = Switchboard::new();
        let shown = board.dispatch(&id("a"), Event::Added);
        // The first account becomes current: its row is listed, then updated as current. With no
        // session yet there is nothing else to show.
        assert!(matches!(
            &shown[0],
            Shown::AccountRow(AccountChange::Inserted { index: 0, view }) if view.id == "a"
        ));
        assert_eq!(shown.len(), 2);
        assert!(is_updated(&shown[1], 0));
        // A known account is not listed twice.
        assert!(board.dispatch(&id("a"), Event::Added).is_empty());
        assert_eq!(board.accounts().views().len(), 1);
    }

    // ACCT-004
    #[test]
    fn an_event_of_another_account_only_updates_its_row() {
        let mut board = three();
        for event in [
            identity(),
            Event::Unread(Unread {
                rooms: 1,
                mentions: 0,
            }),
            logged_in("bob"),
            Event::Connection(Connection::Reconnecting),
        ] {
            let shown = board.dispatch(&id("b"), event);
            assert_eq!(shown.len(), 1);
            assert!(is_updated(&shown[0], 1));
        }
        // Other events change no row.
        let shown = board.dispatch(&id("b"), Event::Rooms(Vec::new()));
        assert!(shown.is_empty());
    }

    // ACCT-004
    #[test]
    fn an_event_of_an_unknown_account_is_ignored() {
        let mut board = three();
        assert!(board.dispatch(&id("nobody"), logged_out()).is_empty());
        assert!(board.dispatch(&id("nobody"), Event::Removed).is_empty());
    }

    // ACCT-004
    #[test]
    fn an_answer_of_another_account_is_dropped() {
        let mut board = three();
        let shown = board.dispatch(
            &id("b"),
            Event::MessageAction {
                id: "m".to_owned().into(),
                action: MessageAction::Edit,
                outcome: Err(ActionError::NotAllowed(MessageAction::Edit)),
            },
        );
        assert!(shown.is_empty());
    }

    // ACT-008
    #[test]
    fn the_directory_events_of_the_current_account_are_shown_and_kept_for_the_others() {
        let mut board = three();
        let events = [
            Event::Directory(Directory::searching("bob")),
            Event::DirectoryAction {
                action: DirectoryAction::Join("c".to_owned().into()),
                state: ActionState::Working,
            },
            Event::Preview(None),
        ];

        for event in events.clone() {
            assert_eq!(
                board.dispatch(&id("a"), event.clone()),
                [Shown::Event(event)]
            );
        }
        for event in events {
            assert!(board.dispatch(&id("b"), event).is_empty());
        }

        let query_of = |board: &mut Switchboard, name: &str| {
            board.select(&id(name));
            board.accounts().current_cache().directory().query.clone()
        };
        assert_eq!(query_of(&mut board, "b"), "bob");
        assert_eq!(query_of(&mut board, "c"), "");
    }

    // ACCT-003
    #[test]
    fn the_first_session_of_the_current_account_shows_everything() {
        let mut board = Switchboard::new();
        board.dispatch(&id("a"), Event::Added);
        let shown = board.dispatch(&id("a"), logged_out());
        assert_eq!(shown.len(), 2);
        assert!(is_updated(&shown[0], 0));
        assert_eq!(shown[1], Shown::All);
    }

    // ACCT-004
    #[test]
    fn an_event_of_the_current_account_after_that_is_shown() {
        let mut board = three();
        let event = Event::Typing {
            room: "r".to_owned().into(),
            names: Vec::new(),
        };
        assert_eq!(
            board.dispatch(&id("a"), event.clone()),
            [Shown::Event(event)]
        );
        // Its session event updates the row too, before the event.
        let shown = board.dispatch(&id("a"), logged_in("alice"));
        assert_eq!(shown.len(), 2);
        assert!(is_updated(&shown[0], 0));
        assert!(matches!(&shown[1], Shown::Event(Event::Session(_))));
    }

    // ACCT-005
    #[test]
    fn removing_the_current_account_switches_to_the_next() {
        let mut board = three();
        let shown = board.dispatch(&id("a"), Event::Removed);
        // The row goes, the session view shows the number of accounts, `b` is current now: its
        // row is updated, and everything is shown.
        assert_eq!(shown.len(), 4);
        assert_eq!(
            shown[0],
            Shown::AccountRow(AccountChange::Removed { index: 0 })
        );
        assert!(matches!(&shown[1], Shown::SessionView(view) if view.account_id == "b"));
        assert!(is_updated(&shown[2], 0));
        assert_eq!(shown[3], Shown::All);
        assert_eq!(board.accounts().current(), Some(&id("b")));
    }

    // ACCT-005
    #[test]
    fn removing_another_account_keeps_the_current_one() {
        let mut board = three();
        let shown = board.dispatch(&id("b"), Event::Removed);
        assert_eq!(shown.len(), 2);
        assert_eq!(
            shown[0],
            Shown::AccountRow(AccountChange::Removed { index: 1 })
        );
        assert!(matches!(&shown[1], Shown::SessionView(view) if view.account_id == "a"));
        assert_eq!(board.accounts().current(), Some(&id("a")));
    }

    // ACCT-005
    #[test]
    fn removing_the_last_account_shows_nothing_more() {
        let mut board = Switchboard::new();
        board.dispatch(&id("a"), Event::Added);
        board.dispatch(&id("a"), logged_out());
        let shown = board.dispatch(&id("a"), Event::Removed);
        // The session view is not told: the current cache is the empty one.
        assert_eq!(
            shown,
            [Shown::AccountRow(AccountChange::Removed { index: 0 })]
        );
        assert_eq!(board.accounts().current(), None);
    }

    // ACCT-003
    #[test]
    fn selecting_an_account_shows_it() {
        let mut board = three();
        let shown = board.select(&id("c"));
        assert_eq!(shown.len(), 3);
        assert!(is_updated(&shown[0], 0));
        assert!(is_updated(&shown[1], 2));
        assert_eq!(shown[2], Shown::All);
    }

    // ACCT-003
    #[test]
    fn selecting_the_current_or_an_unknown_account_shows_nothing() {
        let mut board = three();
        assert!(board.select(&id("a")).is_empty());
        assert!(board.select(&id("nobody")).is_empty());
        assert_eq!(board.accounts().current(), Some(&id("a")));
    }

    // ACCT-003
    #[test]
    fn selecting_an_account_without_a_session_does_not_show_everything() {
        let mut board = Switchboard::new();
        board.dispatch(&id("a"), Event::Added);
        board.dispatch(&id("a"), logged_out());
        board.dispatch(&id("b"), Event::Added);
        let shown = board.select(&id("b"));
        assert_eq!(shown.len(), 2);
        assert!(shown.iter().all(|one| *one != Shown::All));
    }

    // ACCT-001
    #[test]
    fn adding_lists_the_account_and_switches_to_it() {
        let mut board = three();
        let shown = board.add(&id("d"));
        assert!(matches!(
            &shown[0],
            Shown::AccountRow(AccountChange::Inserted { index: 3, view }) if view.id == "d"
        ));
        // The session view is the one of `a`, still current then: `d` has no session to show yet.
        assert!(matches!(&shown[1], Shown::SessionView(view) if view.account_id == "a"));
        assert!(is_updated(&shown[2], 0));
        assert!(is_updated(&shown[3], 3));
        // `d` has no session yet, so its first one shows everything, not this.
        assert_eq!(shown.len(), 4);
        assert_eq!(board.accounts().current(), Some(&id("d")));
        // The core's `Added` for it changes nothing.
        assert!(board.dispatch(&id("d"), Event::Added).is_empty());
        assert_eq!(
            board.dispatch(&id("d"), logged_out()).last(),
            Some(&Shown::All)
        );
    }

    fn lit_rows(board: &Switchboard) -> Vec<bool> {
        board.accounts().views().iter().map(|v| v.current).collect()
    }

    // ACCT-003
    #[test]
    fn showing_github_covers_the_current_account() {
        let mut board = three();
        let shown = board.show_github();
        // Only `a`'s row changes, then the screen.
        assert_eq!(shown.len(), 2);
        assert!(is_updated(&shown[0], 0));
        assert_eq!(shown[1], Shown::Screen(Screen::GitHub));
        assert_eq!(board.screen(), Screen::GitHub);
        assert_eq!(board.accounts().current(), Some(&id("a")));
        assert_eq!(lit_rows(&board), [false, false, false]);
        // Showing it again changes nothing.
        assert!(board.show_github().is_empty());
    }

    // ACCT-004
    #[test]
    fn the_events_of_the_current_account_still_reach_the_views_under_github() {
        let mut board = three();
        board.show_github();
        let event = Event::Typing {
            room: "r".to_owned().into(),
            names: Vec::new(),
        };
        assert_eq!(
            board.dispatch(&id("a"), event.clone()),
            [Shown::Event(event)]
        );
        // The unread dot shows on a tile that is not on screen.
        let shown = board.dispatch(
            &id("a"),
            Event::Unread(Unread {
                rooms: 1,
                mentions: 0,
            }),
        );
        assert!(is_updated(&shown[0], 0));
        assert!(board.accounts().views()[0].unread);
    }

    // ACCT-003
    #[test]
    fn selecting_the_current_account_from_github_goes_back_without_a_snapshot() {
        let mut board = three();
        board.show_github();
        let shown = board.select(&id("a"));
        assert_eq!(shown.len(), 2);
        assert!(is_updated(&shown[0], 0));
        assert_eq!(shown[1], Shown::Screen(Screen::Account));
        assert_eq!(lit_rows(&board), [true, false, false]);
    }

    // ACCT-003
    #[test]
    fn selecting_another_account_from_github_shows_it() {
        let mut board = three();
        board.show_github();
        let shown = board.select(&id("c"));
        // Only `c`'s row lights: `a` was dark already.
        assert_eq!(shown.len(), 3);
        assert!(is_updated(&shown[0], 2));
        assert_eq!(shown[1], Shown::All);
        assert_eq!(shown[2], Shown::Screen(Screen::Account));
        assert_eq!(lit_rows(&board), [false, false, true]);
        // An unknown account leaves GitHub where it is.
        board.show_github();
        assert!(board.select(&id("nobody")).is_empty());
        assert_eq!(board.screen(), Screen::GitHub);
    }

    // ACCT-001
    #[test]
    fn adding_an_account_from_github_shows_its_login_page() {
        let mut board = three();
        board.show_github();
        let shown = board.add(&id("d"));
        assert!(matches!(
            &shown[0],
            Shown::AccountRow(AccountChange::Inserted { index: 3, view }) if view.id == "d"
        ));
        assert!(matches!(&shown[1], Shown::SessionView(view) if view.account_id == "a"));
        assert!(is_updated(&shown[2], 3));
        assert_eq!(shown[3], Shown::Screen(Screen::Account));
        assert_eq!(shown.len(), 4);
        assert_eq!(board.accounts().current(), Some(&id("d")));
        assert_eq!(lit_rows(&board), [false, false, false, true]);
    }

    // ACCT-005
    #[test]
    fn removing_accounts_from_under_github_keeps_it_on_screen() {
        let mut board = three();
        board.show_github();
        // Another account: the row goes, the session view is told.
        let shown = board.dispatch(&id("b"), Event::Removed);
        assert_eq!(shown.len(), 2);
        assert_eq!(
            shown[0],
            Shown::AccountRow(AccountChange::Removed { index: 1 })
        );
        // The current one: `b` is next; the views get its snapshot, no row lights.
        let shown = board.dispatch(&id("a"), Event::Removed);
        assert_eq!(shown.len(), 3);
        assert_eq!(
            shown[0],
            Shown::AccountRow(AccountChange::Removed { index: 0 })
        );
        assert!(matches!(&shown[1], Shown::SessionView(view) if view.account_id == "c"));
        assert_eq!(shown[2], Shown::All);
        assert_eq!(board.screen(), Screen::GitHub);
        assert_eq!(lit_rows(&board), [false]);
        // The last one: nothing more to show.
        let shown = board.dispatch(&id("c"), Event::Removed);
        assert_eq!(
            shown,
            [Shown::AccountRow(AccountChange::Removed { index: 0 })]
        );
        assert_eq!(board.screen(), Screen::GitHub);
    }
}
