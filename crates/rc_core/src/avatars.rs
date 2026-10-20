// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Which avatars to fetch, and what the frontend has been told about them. Pure bookkeeping: the
//! session actor does the I/O.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use crate::domain::{AvatarOwner, MessageKind};
use crate::timeline::TimelineChange;

/// Downloads running at once.
pub(crate) const MAX_DOWNLOADS: usize = 4;

/// A change of the avatar file the frontend shows for an owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarUpdate {
    pub owner: AvatarOwner,
    /// The file to show, or `None` if the avatar is gone (logging out forgets them all).
    pub path: Option<PathBuf>,
}

/// How a download ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fetched {
    /// The file is stored at `path`; `changed` tells whether its content is not what was there.
    Stored { path: PathBuf, changed: bool },
    /// The server says there is no avatar of its own, which a cached file may predate.
    Missing,
    /// There is no avatar to show: the server refused, or the file can't be kept.
    Nothing,
    /// The server couldn't be reached: worth another try when the avatar is wanted again.
    Unreachable,
}

#[derive(Default)]
pub(crate) struct Avatars {
    /// The owners looked at in this session: their cached file was looked up, and a refresh was
    /// queued, once. It is what makes a refresh happen at most once per session.
    seen: HashSet<AvatarOwner>,
    queue: VecDeque<AvatarOwner>,
    running: usize,
    /// What the frontend has been told.
    shown: HashMap<AvatarOwner, PathBuf>,
}

impl Avatars {
    /// The owner's avatar is wanted. The first time in a session this looks the cached file up
    /// (`cached`), which is returned if there is one, and queues the refresh.
    pub(crate) fn want(
        &mut self,
        owner: AvatarOwner,
        cached: impl FnOnce(&AvatarOwner) -> Option<PathBuf>,
    ) -> Option<AvatarUpdate> {
        if !self.seen.insert(owner.clone()) {
            return None;
        }
        self.queue.push_back(owner.clone());
        let path = cached(&owner)?;
        if self.shown.get(&owner) == Some(&path) {
            return None;
        }
        Some(self.show(owner, path))
    }

    /// The next download to start, if fewer than [`MAX_DOWNLOADS`] are running.
    pub(crate) fn next_download(&mut self) -> Option<AvatarOwner> {
        if self.running >= MAX_DOWNLOADS {
            return None;
        }
        let owner = self.queue.pop_front()?;
        self.running += 1;
        Some(owner)
    }

    /// A download ended. Returns what to tell the frontend, if it changes what it shows.
    pub(crate) fn finished(
        &mut self,
        owner: AvatarOwner,
        fetched: Fetched,
    ) -> Option<AvatarUpdate> {
        self.running = self.running.saturating_sub(1);
        match fetched {
            Fetched::Stored { path, changed } => {
                if !changed && self.shown.get(&owner) == Some(&path) {
                    return None;
                }
                Some(self.show(owner, path))
            }
            Fetched::Missing => self
                .shown
                .remove(&owner)
                .map(|_| AvatarUpdate { owner, path: None }),
            Fetched::Nothing => None,
            Fetched::Unreachable => {
                self.seen.remove(&owner);
                None
            }
        }
    }

    /// Looks at the owner's avatar again, if it was looked at: the server says it changed.
    pub(crate) fn invalidate(&mut self, owner: &AvatarOwner) -> bool {
        self.seen.remove(owner)
    }

    /// Forgets everything, for a new session. Returns what the frontend should forget.
    pub(crate) fn reset(&mut self) -> Vec<AvatarUpdate> {
        self.seen.clear();
        self.queue.clear();
        self.running = 0;
        self.shown
            .drain()
            .map(|(owner, _)| AvatarUpdate { owner, path: None })
            .collect()
    }

    /// Records what the frontend is told.
    fn show(&mut self, owner: AvatarOwner, path: PathBuf) -> AvatarUpdate {
        self.shown.insert(owner.clone(), path.clone());
        AvatarUpdate {
            owner,
            path: Some(path),
        }
    }
}

/// The owners of the avatars of the people who wrote the messages in `changes`. System messages
/// show no avatar.
pub(crate) fn authors_of(changes: &[TimelineChange]) -> Vec<AvatarOwner> {
    let mut owners = Vec::new();
    let mut add = |messages: &[crate::domain::Message]| {
        for message in messages {
            if matches!(message.kind, MessageKind::Text(_)) {
                owners.push(AvatarOwner::User(message.author.username.clone()));
            }
        }
    };
    for change in changes {
        match change {
            TimelineChange::Reset { messages, .. }
            | TimelineChange::Inserted { messages, .. }
            | TimelineChange::Prepended(messages)
            | TimelineChange::Appended(messages) => add(messages),
            TimelineChange::Updated { message, .. } | TimelineChange::Moved { message, .. } => {
                add(std::slice::from_ref(message));
            }
        }
    }
    owners
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: &str) -> AvatarOwner {
        AvatarOwner::User(name.to_owned())
    }

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/cache/{name}.png"))
    }

    // RICH-004
    #[test]
    fn a_cached_file_is_shown_at_once_and_the_refresh_is_queued() {
        let mut avatars = Avatars::default();
        let update = avatars.want(user("alice"), |_| Some(path("alice")));
        assert_eq!(
            update,
            Some(AvatarUpdate {
                owner: user("alice"),
                path: Some(path("alice"))
            })
        );
        assert_eq!(avatars.next_download(), Some(user("alice")));
    }

    // RICH-004
    #[test]
    fn without_a_cached_file_there_is_nothing_to_show_yet() {
        let mut avatars = Avatars::default();
        assert_eq!(avatars.want(user("alice"), |_| None), None);
        assert_eq!(avatars.next_download(), Some(user("alice")));
    }

    // RICH-004
    #[test]
    fn an_avatar_is_refreshed_once_per_session() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| None);
        avatars.want(user("alice"), |_| panic!("looked up again"));
        assert_eq!(avatars.next_download(), Some(user("alice")));
        assert_eq!(avatars.next_download(), None);
        avatars.finished(user("alice"), Fetched::Nothing);
        avatars.want(user("alice"), |_| panic!("looked up again"));
        assert_eq!(avatars.next_download(), None);
    }

    // RICH-004
    #[test]
    fn at_most_four_downloads_run_at_once() {
        let mut avatars = Avatars::default();
        for name in ["a", "b", "c", "d", "e", "f"] {
            avatars.want(user(name), |_| None);
        }
        let started: Vec<_> = std::iter::from_fn(|| avatars.next_download()).collect();
        assert_eq!(started, [user("a"), user("b"), user("c"), user("d")]);

        avatars.finished(user("b"), Fetched::Nothing);
        assert_eq!(avatars.next_download(), Some(user("e")));
        assert_eq!(avatars.next_download(), None);
        avatars.finished(user("a"), Fetched::Nothing);
        avatars.finished(user("c"), Fetched::Nothing);
        assert_eq!(avatars.next_download(), Some(user("f")));
    }

    // RICH-004
    #[test]
    fn an_avatar_the_server_doesnt_have_any_more_is_forgotten() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| Some(path("alice")));
        avatars.want(user("bob"), |_| None);
        avatars.next_download();
        avatars.next_download();

        assert_eq!(
            avatars.finished(user("alice"), Fetched::Missing),
            Some(AvatarUpdate {
                owner: user("alice"),
                path: None
            })
        );
        // Nothing was shown for bob.
        assert_eq!(avatars.finished(user("bob"), Fetched::Missing), None);
    }

    // RICH-004
    #[test]
    fn a_download_that_changes_nothing_says_nothing() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| Some(path("alice")));
        avatars.next_download();
        let same = Fetched::Stored {
            path: path("alice"),
            changed: false,
        };
        assert_eq!(avatars.finished(user("alice"), same), None);
    }

    // RICH-004
    #[test]
    fn a_download_that_changes_the_file_says_so() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| Some(path("alice")));
        avatars.next_download();
        let changed = Fetched::Stored {
            path: path("alice"),
            changed: true,
        };
        assert_eq!(
            avatars.finished(user("alice"), changed),
            Some(AvatarUpdate {
                owner: user("alice"),
                path: Some(path("alice"))
            })
        );
    }

    // RICH-004
    #[test]
    fn a_first_download_is_shown_even_if_it_was_cached_unseen() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| None);
        avatars.next_download();
        let stored = Fetched::Stored {
            path: path("alice"),
            changed: false,
        };
        assert!(avatars.finished(user("alice"), stored).is_some());
    }

    // RICH-004
    #[test]
    fn an_unreachable_server_lets_the_avatar_be_wanted_again() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| None);
        avatars.next_download();
        avatars.finished(user("alice"), Fetched::Unreachable);
        avatars.want(user("alice"), |_| None);
        assert_eq!(avatars.next_download(), Some(user("alice")));
    }

    // RICH-004
    #[test]
    fn an_avatar_the_server_refused_is_not_asked_for_again() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| None);
        avatars.next_download();
        avatars.finished(user("alice"), Fetched::Nothing);
        avatars.want(user("alice"), |_| None);
        assert_eq!(avatars.next_download(), None);
    }

    // RICH-004
    #[test]
    fn an_avatar_the_server_says_changed_is_looked_at_again_only_if_it_was_before() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| Some(path("alice")));
        avatars.next_download();
        avatars.finished(user("alice"), Fetched::Nothing);

        assert!(!avatars.invalidate(&user("bob")));
        assert!(avatars.invalidate(&user("alice")));
        // The file shown is the one cached: nothing to tell, a refresh is queued.
        assert_eq!(avatars.want(user("alice"), |_| Some(path("alice"))), None);
        assert_eq!(avatars.next_download(), Some(user("alice")));
    }

    // RICH-004
    #[test]
    fn resetting_forgets_what_was_shown_and_starts_the_session_over() {
        let mut avatars = Avatars::default();
        avatars.want(user("alice"), |_| Some(path("alice")));
        avatars.want(user("bob"), |_| None);
        avatars.next_download();

        let forgotten = avatars.reset();

        assert_eq!(
            forgotten,
            [AvatarUpdate {
                owner: user("alice"),
                path: None
            }]
        );
        assert_eq!(avatars.next_download(), None);
        // The slot of the aborted download is free.
        for name in ["v", "w", "x", "y", "z"] {
            avatars.want(user(name), |_| None);
        }
        assert_eq!(std::iter::from_fn(|| avatars.next_download()).count(), 4);
    }
}
