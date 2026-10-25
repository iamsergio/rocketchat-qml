// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Which files to fetch for a kind of image (avatars, attachment images), and what the frontend
//! has been told about them. Pure bookkeeping: the session actor does the I/O.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::path::PathBuf;

/// Downloads running at once.
pub(crate) const MAX_DOWNLOADS: usize = 4;

/// What a file the cache already holds means when its key is wanted for the first time in a
/// session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refresh {
    /// The file is final, as an uploaded one is: it is shown and nothing is downloaded.
    Never,
    /// The file may be out of date, as an avatar can be: it is shown at once and downloaded again,
    /// once per session.
    OncePerSession,
}

/// A change of the file the frontend shows for a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change<K> {
    pub(crate) key: K,
    /// The file to show, or `None` if it is gone.
    pub(crate) file: Option<PathBuf>,
}

/// How a download ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fetched {
    /// The file is stored at `path`; `changed` tells whether its content is not what was there.
    Stored { path: PathBuf, changed: bool },
    /// The server says there is nothing of its own, which a cached file may predate.
    Missing,
    /// There is nothing to show: the server refused, or the file can't be kept.
    Nothing,
    /// The server couldn't be reached: worth another try when the key is wanted again.
    Unreachable,
}

pub(crate) struct Downloads<K> {
    refresh: Refresh,
    /// The keys looked at in this session: their cached file was looked up and, if it needs one,
    /// a download queued, once.
    seen: HashSet<K>,
    queue: VecDeque<K>,
    running: usize,
    /// What the frontend has been told.
    shown: HashMap<K, PathBuf>,
}

impl<K: Clone + Eq + Hash> Downloads<K> {
    pub(crate) fn new(refresh: Refresh) -> Self {
        Self {
            refresh,
            seen: HashSet::new(),
            queue: VecDeque::new(),
            running: 0,
            shown: HashMap::new(),
        }
    }

    /// The file of `key` is wanted. The first time in a session this looks the cached file up
    /// (`cached`), which is returned if there is one, and queues a download as [`Refresh`] says.
    pub(crate) fn want(
        &mut self,
        key: K,
        cached: impl FnOnce(&K) -> Option<PathBuf>,
    ) -> Option<Change<K>> {
        if !self.seen.insert(key.clone()) {
            return None;
        }
        let file = cached(&key);
        if file.is_none() || self.refresh == Refresh::OncePerSession {
            self.queue.push_back(key.clone());
        }
        self.show_new(key, file?)
    }

    /// Shows the cached file (`cached`) of `key` if there is one and it isn't what the frontend
    /// has. For files that nobody looks at yet: the key isn't marked as looked at and no download
    /// is queued, so a later [`want`](Self::want) does both.
    pub(crate) fn show_cached(
        &mut self,
        key: K,
        cached: impl FnOnce(&K) -> Option<PathBuf>,
    ) -> Option<Change<K>> {
        let file = cached(&key)?;
        self.show_new(key, file)
    }

    /// The next download to start, if fewer than [`MAX_DOWNLOADS`] are running.
    pub(crate) fn next_download(&mut self) -> Option<K> {
        if self.running >= MAX_DOWNLOADS {
            return None;
        }
        let key = self.queue.pop_front()?;
        self.running += 1;
        Some(key)
    }

    /// A download ended. Returns what to tell the frontend, if it changes what it shows.
    pub(crate) fn finished(&mut self, key: K, fetched: Fetched) -> Option<Change<K>> {
        self.running = self.running.saturating_sub(1);
        match fetched {
            Fetched::Stored { path, changed } => {
                if !changed && self.shown.get(&key) == Some(&path) {
                    return None;
                }
                Some(self.show(key, path))
            }
            Fetched::Missing => self.shown.remove(&key).map(|_| Change { key, file: None }),
            Fetched::Nothing => None,
            Fetched::Unreachable => {
                self.seen.remove(&key);
                None
            }
        }
    }

    /// Looks at the key again, if it was looked at: the server says it changed.
    pub(crate) fn invalidate(&mut self, key: &K) -> bool {
        self.seen.remove(key)
    }

    /// Forgets everything, for a new session. Returns what the frontend should forget.
    pub(crate) fn reset(&mut self) -> Vec<Change<K>> {
        self.seen.clear();
        self.queue.clear();
        self.running = 0;
        self.shown
            .drain()
            .map(|(key, _)| Change { key, file: None })
            .collect()
    }

    fn show_new(&mut self, key: K, file: PathBuf) -> Option<Change<K>> {
        if self.shown.get(&key) == Some(&file) {
            return None;
        }
        Some(self.show(key, file))
    }

    /// Records what the frontend is told.
    fn show(&mut self, key: K, file: PathBuf) -> Change<K> {
        self.shown.insert(key.clone(), file.clone());
        Change {
            key,
            file: Some(file),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PathBuf {
        PathBuf::from(format!("/cache/{name}.png"))
    }

    fn shown(name: &str) -> Change<String> {
        Change {
            key: name.to_owned(),
            file: Some(file(name)),
        }
    }

    fn gone(name: &str) -> Change<String> {
        Change {
            key: name.to_owned(),
            file: None,
        }
    }

    fn stored(name: &str, changed: bool) -> Fetched {
        Fetched::Stored {
            path: file(name),
            changed,
        }
    }

    /// Wants `name` with nothing cached.
    fn want_uncached(downloads: &mut Downloads<String>, name: &str) -> Option<Change<String>> {
        downloads.want(name.to_owned(), |_| None)
    }

    fn avatars() -> Downloads<String> {
        Downloads::new(Refresh::OncePerSession)
    }

    fn images() -> Downloads<String> {
        Downloads::new(Refresh::Never)
    }

    // RICH-004
    #[test]
    fn a_cached_file_is_shown_at_once_and_the_refresh_is_queued() {
        let mut avatars = avatars();
        let update = avatars.want("alice".into(), |_| Some(file("alice")));
        assert_eq!(update, Some(shown("alice")));
        assert_eq!(avatars.next_download(), Some("alice".to_owned()));
    }

    // RICH-004
    #[test]
    fn without_a_cached_file_there_is_nothing_to_show_yet() {
        let mut avatars = avatars();
        assert_eq!(want_uncached(&mut avatars, "alice"), None);
        assert_eq!(avatars.next_download(), Some("alice".to_owned()));
    }

    // ROOM-010
    #[test]
    fn a_cached_file_can_be_shown_without_looking_at_the_avatar() {
        let mut avatars = avatars();
        // Nothing cached, nothing to show.
        assert_eq!(avatars.show_cached("alice".into(), |_| None), None);
        let update = avatars.show_cached("alice".into(), |_| Some(file("alice")));
        assert_eq!(update, Some(shown("alice")));
        // The same file again says nothing, and no download was queued.
        assert_eq!(
            avatars.show_cached("alice".into(), |_| Some(file("alice"))),
            None
        );
        assert_eq!(avatars.next_download(), None);
        // Wanted later, it is downloaded, and the cached file it already showed isn't repeated.
        assert_eq!(avatars.want("alice".into(), |_| Some(file("alice"))), None);
        assert_eq!(avatars.next_download(), Some("alice".to_owned()));
    }

    // RICH-004
    #[test]
    fn an_avatar_is_refreshed_once_per_session() {
        let mut avatars = avatars();
        want_uncached(&mut avatars, "alice");
        avatars.want("alice".into(), |_| panic!("looked up again"));
        assert_eq!(avatars.next_download(), Some("alice".to_owned()));
        assert_eq!(avatars.next_download(), None);
        avatars.finished("alice".into(), Fetched::Nothing);
        avatars.want("alice".into(), |_| panic!("looked up again"));
        assert_eq!(avatars.next_download(), None);
    }

    // RICH-004
    #[test]
    fn an_avatar_the_server_doesnt_have_any_more_is_forgotten() {
        let mut avatars = avatars();
        avatars.want("alice".into(), |_| Some(file("alice")));
        want_uncached(&mut avatars, "bob");
        avatars.next_download();
        avatars.next_download();

        assert_eq!(
            avatars.finished("alice".into(), Fetched::Missing),
            Some(gone("alice"))
        );
        // Nothing was shown for bob.
        assert_eq!(avatars.finished("bob".into(), Fetched::Missing), None);
    }

    // RICH-004
    #[test]
    fn a_download_that_changes_nothing_says_nothing() {
        let mut avatars = avatars();
        avatars.want("alice".into(), |_| Some(file("alice")));
        avatars.next_download();
        assert_eq!(
            avatars.finished("alice".into(), stored("alice", false)),
            None
        );
    }

    // RICH-004
    #[test]
    fn a_download_that_changes_the_file_says_so() {
        let mut avatars = avatars();
        avatars.want("alice".into(), |_| Some(file("alice")));
        avatars.next_download();
        assert_eq!(
            avatars.finished("alice".into(), stored("alice", true)),
            Some(shown("alice"))
        );
    }

    // RICH-004
    #[test]
    fn a_first_download_is_shown_even_if_it_was_cached_unseen() {
        let mut avatars = avatars();
        want_uncached(&mut avatars, "alice");
        avatars.next_download();
        assert!(
            avatars
                .finished("alice".into(), stored("alice", false))
                .is_some()
        );
    }

    // RICH-004
    #[test]
    fn an_avatar_the_server_says_changed_is_looked_at_again_only_if_it_was_before() {
        let mut avatars = avatars();
        avatars.want("alice".into(), |_| Some(file("alice")));
        avatars.next_download();
        avatars.finished("alice".into(), Fetched::Nothing);

        assert!(!avatars.invalidate(&"bob".to_owned()));
        assert!(avatars.invalidate(&"alice".to_owned()));
        // The file shown is the one cached: nothing to tell, a refresh is queued.
        assert_eq!(avatars.want("alice".into(), |_| Some(file("alice"))), None);
        assert_eq!(avatars.next_download(), Some("alice".to_owned()));
    }

    // RICH-005
    #[test]
    fn a_cached_image_is_shown_at_once_and_not_downloaded() {
        let mut images = images();
        let update = images.want("a".into(), |_| Some(file("a")));
        assert_eq!(update, Some(shown("a")));
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn an_image_that_is_not_cached_is_queued_once() {
        let mut images = images();
        assert_eq!(want_uncached(&mut images, "a"), None);
        images.want("a".into(), |_| panic!("looked up again"));
        assert_eq!(images.next_download(), Some("a".to_owned()));
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn a_downloaded_image_is_shown() {
        let mut images = images();
        want_uncached(&mut images, "a");
        images.next_download();
        assert_eq!(
            images.finished("a".into(), stored("a", true)),
            Some(shown("a"))
        );
        // Wanted again, it is neither shown again nor downloaded.
        images.want("a".into(), |_| panic!("looked up again"));
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn an_image_the_server_refused_is_not_asked_for_again() {
        let mut images = images();
        want_uncached(&mut images, "a");
        images.next_download();
        assert_eq!(images.finished("a".into(), Fetched::Nothing), None);
        want_uncached(&mut images, "a");
        assert_eq!(images.next_download(), None);
    }

    // RICH-004, RICH-005
    #[test]
    fn at_most_four_downloads_run_at_once() {
        for mut downloads in [avatars(), images()] {
            for name in ["a", "b", "c", "d", "e", "f"] {
                want_uncached(&mut downloads, name);
            }
            let started: Vec<_> = std::iter::from_fn(|| downloads.next_download()).collect();
            assert_eq!(started, ["a", "b", "c", "d"]);

            downloads.finished("b".into(), Fetched::Nothing);
            assert_eq!(downloads.next_download(), Some("e".to_owned()));
            assert_eq!(downloads.next_download(), None);
            downloads.finished("a".into(), Fetched::Nothing);
            downloads.finished("c".into(), Fetched::Nothing);
            assert_eq!(downloads.next_download(), Some("f".to_owned()));
        }
    }

    // RICH-004, RICH-005
    #[test]
    fn an_unreachable_server_lets_the_key_be_wanted_again() {
        for mut downloads in [avatars(), images()] {
            want_uncached(&mut downloads, "a");
            downloads.next_download();
            assert_eq!(downloads.finished("a".into(), Fetched::Unreachable), None);
            want_uncached(&mut downloads, "a");
            assert_eq!(downloads.next_download(), Some("a".to_owned()));
        }
    }

    // RICH-004, RICH-005
    #[test]
    fn resetting_forgets_what_was_shown_and_starts_the_session_over() {
        for mut downloads in [avatars(), images()] {
            downloads.want("a".into(), |_| Some(file("a")));
            want_uncached(&mut downloads, "b");
            downloads.next_download();

            assert_eq!(downloads.reset(), [gone("a")]);
            assert_eq!(downloads.next_download(), None);
            // The slot of the aborted download is free, and the cache is asked again.
            for name in ["v", "w", "x", "y", "z"] {
                want_uncached(&mut downloads, name);
            }
            assert_eq!(std::iter::from_fn(|| downloads.next_download()).count(), 4);
            assert!(downloads.want("a".into(), |_| Some(file("a"))).is_some());
        }
    }
}
