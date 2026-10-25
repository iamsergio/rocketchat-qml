// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The attachment images the frontend is told about, and which ones the messages show.
//!
//! An uploaded file never changes, so unlike an avatar an image is downloaded only if the cache
//! doesn't hold it, and never refreshed.

use std::path::PathBuf;

use crate::downloads::Change;
use crate::timeline::TimelineChange;

/// A change of the file the frontend shows for an attachment image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageUpdate {
    /// The image's path on the server, as the message carries it.
    pub path: String,
    /// The file to show, or `None` if it is gone (logging out forgets them all).
    pub file: Option<PathBuf>,
}

impl From<Change<String>> for ImageUpdate {
    fn from(change: Change<String>) -> Self {
        Self {
            path: change.key,
            file: change.file,
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
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
            thread_followers: Vec::new(),
        }
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
