// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Which attachment images to fetch, and what the frontend has been told about them. Pure
//! bookkeeping: the session actor does the I/O.
//!
//! An uploaded file never changes, so unlike an avatar an image is downloaded only if the cache
//! doesn't hold it, and never refreshed.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use crate::timeline::TimelineChange;

/// Downloads running at once.
pub(crate) const MAX_DOWNLOADS: usize = 4;

/// A change of the file the frontend shows for an attachment image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageUpdate {
    /// The image's path on the server, as the message carries it.
    pub path: String,
    /// The file to show, or `None` if it is gone (logging out forgets them all).
    pub file: Option<PathBuf>,
}

/// How a download ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fetched {
    /// The file is stored at this path.
    Stored(PathBuf),
    /// There is nothing to show: the server refused, or the file can't be kept.
    Nothing,
    /// The server couldn't be reached: worth another try when the image is wanted again.
    Unreachable,
}

#[derive(Default)]
pub(crate) struct Images {
    /// The images looked at in this session: the cache was asked and, if it had no file, a
    /// download was queued, once.
    seen: HashSet<String>,
    queue: VecDeque<String>,
    running: usize,
    /// What the frontend has been told.
    shown: HashMap<String, PathBuf>,
}

impl Images {
    /// The image at `path` is wanted. The first time in a session this looks the cached file up
    /// (`cached`): if there is one it is returned and nothing is downloaded, else a download is
    /// queued.
    pub(crate) fn want(
        &mut self,
        path: String,
        cached: impl FnOnce(&str) -> Option<PathBuf>,
    ) -> Option<ImageUpdate> {
        if !self.seen.insert(path.clone()) {
            return None;
        }
        let Some(file) = cached(&path) else {
            self.queue.push_back(path);
            return None;
        };
        Some(self.show(path, file))
    }

    /// The next download to start, if fewer than [`MAX_DOWNLOADS`] are running.
    pub(crate) fn next_download(&mut self) -> Option<String> {
        if self.running >= MAX_DOWNLOADS {
            return None;
        }
        let path = self.queue.pop_front()?;
        self.running += 1;
        Some(path)
    }

    /// A download ended. Returns what to tell the frontend, if anything.
    pub(crate) fn finished(&mut self, path: String, fetched: Fetched) -> Option<ImageUpdate> {
        self.running = self.running.saturating_sub(1);
        match fetched {
            Fetched::Stored(file) => Some(self.show(path, file)),
            Fetched::Nothing => None,
            Fetched::Unreachable => {
                self.seen.remove(&path);
                None
            }
        }
    }

    /// Forgets everything, for a new session. Returns what the frontend should forget.
    pub(crate) fn reset(&mut self) -> Vec<ImageUpdate> {
        self.seen.clear();
        self.queue.clear();
        self.running = 0;
        self.shown
            .drain()
            .map(|(path, _)| ImageUpdate { path, file: None })
            .collect()
    }

    /// Records what the frontend is told.
    fn show(&mut self, path: String, file: PathBuf) -> ImageUpdate {
        self.shown.insert(path.clone(), file.clone());
        ImageUpdate {
            path,
            file: Some(file),
        }
    }
}

/// The paths of the images of the attachments of the messages in `changes`.
pub(crate) fn images_of(changes: &[TimelineChange]) -> Vec<String> {
    changes
        .iter()
        .flat_map(TimelineChange::messages)
        .flat_map(|message| &message.attachments)
        .filter_map(|attachment| attachment.image.as_ref())
        .map(|image| image.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Attachment, Author, Delivery, ImageAttachment, Message, MessageKind};

    fn path(name: &str) -> String {
        format!("/file-upload/id/{name}.png")
    }

    fn file(name: &str) -> PathBuf {
        PathBuf::from(format!("/cache/{name}.png"))
    }

    fn message(images: &[&str]) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
            ts: jiff::Timestamp::UNIX_EPOCH,
            kind: MessageKind::Text("".into()),
            delivery: Delivery::Sent,
            attachments: images
                .iter()
                .map(|name| Attachment {
                    title: (*name).to_owned(),
                    link: path(name),
                    description: None,
                    size: None,
                    image: Some(ImageAttachment {
                        path: path(name),
                        width: None,
                        height: None,
                    }),
                })
                .collect(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
        }
    }

    // RICH-005
    #[test]
    fn a_cached_image_is_shown_at_once_and_not_downloaded() {
        let mut images = Images::default();
        let update = images.want(path("a"), |_| Some(file("a")));
        assert_eq!(
            update,
            Some(ImageUpdate {
                path: path("a"),
                file: Some(file("a"))
            })
        );
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn an_image_that_is_not_cached_is_queued_once() {
        let mut images = Images::default();
        assert_eq!(images.want(path("a"), |_| None), None);
        images.want(path("a"), |_| panic!("looked up again"));
        assert_eq!(images.next_download(), Some(path("a")));
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn a_downloaded_image_is_shown() {
        let mut images = Images::default();
        images.want(path("a"), |_| None);
        images.next_download();
        assert_eq!(
            images.finished(path("a"), Fetched::Stored(file("a"))),
            Some(ImageUpdate {
                path: path("a"),
                file: Some(file("a"))
            })
        );
        // Wanted again, it is neither shown again nor downloaded.
        images.want(path("a"), |_| panic!("looked up again"));
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn at_most_four_downloads_run_at_once() {
        let mut images = Images::default();
        for name in ["a", "b", "c", "d", "e", "f"] {
            images.want(path(name), |_| None);
        }
        let started: Vec<_> = std::iter::from_fn(|| images.next_download()).collect();
        assert_eq!(started, [path("a"), path("b"), path("c"), path("d")]);

        images.finished(path("b"), Fetched::Nothing);
        assert_eq!(images.next_download(), Some(path("e")));
        assert_eq!(images.next_download(), None);
        images.finished(path("a"), Fetched::Nothing);
        images.finished(path("c"), Fetched::Nothing);
        assert_eq!(images.next_download(), Some(path("f")));
    }

    // RICH-005
    #[test]
    fn an_unreachable_server_lets_the_image_be_wanted_again() {
        let mut images = Images::default();
        images.want(path("a"), |_| None);
        images.next_download();
        assert_eq!(images.finished(path("a"), Fetched::Unreachable), None);
        images.want(path("a"), |_| None);
        assert_eq!(images.next_download(), Some(path("a")));
    }

    // RICH-005
    #[test]
    fn an_image_the_server_refused_is_not_asked_for_again() {
        let mut images = Images::default();
        images.want(path("a"), |_| None);
        images.next_download();
        assert_eq!(images.finished(path("a"), Fetched::Nothing), None);
        images.want(path("a"), |_| None);
        assert_eq!(images.next_download(), None);
    }

    // RICH-005
    #[test]
    fn resetting_forgets_what_was_shown_and_starts_the_session_over() {
        let mut images = Images::default();
        images.want(path("a"), |_| Some(file("a")));
        images.want(path("b"), |_| None);
        images.next_download();

        let forgotten = images.reset();

        assert_eq!(
            forgotten,
            [ImageUpdate {
                path: path("a"),
                file: None
            }]
        );
        assert_eq!(images.next_download(), None);
        // The slot of the aborted download is free, and the cache is asked again.
        for name in ["v", "w", "x", "y", "z"] {
            images.want(path(name), |_| None);
        }
        assert_eq!(std::iter::from_fn(|| images.next_download()).count(), 4);
        assert!(images.want(path("a"), |_| Some(file("a"))).is_some());
    }

    // RICH-005
    #[test]
    fn the_images_of_the_changed_messages_are_wanted() {
        let changes = [
            TimelineChange::Reset {
                room: None,
                messages: vec![message(&["a"]), message(&[])],
            },
            TimelineChange::Appended(vec![message(&["b", "c"])]),
            TimelineChange::Updated {
                index: 0,
                message: message(&["d"]),
            },
        ];
        assert_eq!(
            images_of(&changes),
            [path("a"), path("b"), path("c"), path("d")]
        );
    }

    // RICH-005
    #[test]
    fn attachments_without_an_image_want_nothing() {
        let mut plain = message(&["a"]);
        plain.attachments[0].image = None;
        assert_eq!(
            images_of(&[TimelineChange::Appended(vec![plain])]),
            [] as [String; 0]
        );
    }
}
