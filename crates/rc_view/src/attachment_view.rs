// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `MessageList` model shows for the files attached to a message: the local file of each
//! image, the link to the original and the size in words. Plain Rust, so it can be unit-tested
//! without Qt.

use std::collections::HashSet;
use std::hash::BuildHasher;
use std::path::Path;

use rc_core::ImageUpdate;
use rc_core::domain::{Attachment, Message};

use crate::file_urls::{FileUpdate, FileUrls};

/// The image files by their path on the server, as the `file://` URLs the GUI loads them from. An
/// image without a file has no entry.
pub type ImageUrls = FileUrls<String>;

impl FileUpdate for ImageUpdate {
    type Key = String;

    fn key(&self) -> &String {
        &self.path
    }

    fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}

/// One attachment as the GUI gets it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttachmentView {
    /// The file's name, or its link's if the server gave none.
    pub title: String,
    pub description: String,
    /// For example `1.5 KB`; empty when the server didn't say.
    pub size: String,
    /// Where the original is: the address on the server.
    pub link: String,
    pub is_image: bool,
    /// The local file of the image, empty while it isn't downloaded.
    pub image: String,
    /// The size in pixels of the image, 0 when unknown.
    pub width: i32,
    pub height: i32,
}

/// The attachments of `message`. `server` is the address the links start with; without one (no
/// session) the links are empty.
#[must_use]
pub fn attachments(
    message: &Message,
    server: Option<&str>,
    images: &ImageUrls,
) -> Vec<AttachmentView> {
    message
        .attachments
        .iter()
        .map(|attachment| view(attachment, server, images))
        .collect()
}

fn view(attachment: &Attachment, server: Option<&str>, images: &ImageUrls) -> AttachmentView {
    let image = attachment.image.as_ref();
    AttachmentView {
        title: if attachment.title.is_empty() {
            file_name(&attachment.link).to_owned()
        } else {
            attachment.title.clone()
        },
        description: attachment.description.clone().unwrap_or_default(),
        size: attachment.size.map(human_size).unwrap_or_default(),
        link: server.map_or_else(String::new, |server| absolute(server, &attachment.link)),
        is_image: image.is_some(),
        image: image.map_or("", |image| images.url(&image.path)).to_owned(),
        width: image.and_then(|image| image.width).map_or(0, clamp),
        height: image.and_then(|image| image.height).map_or(0, clamp),
    }
}

fn clamp(pixels: u32) -> i32 {
    i32::try_from(pixels).unwrap_or(i32::MAX)
}

/// `path`, which starts with `/`, on `server`, the URL the session logged in with (a path prefix
/// is kept).
fn absolute(server: &str, path: &str) -> String {
    format!("{}{path}", server.trim_end_matches('/'))
}

/// The last segment of a server path, as the server escaped it.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `n` bytes in words: `512 B`, `1.5 KB`, `2.0 MB`. Units of 1024, one decimal above bytes.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Lossless enough for a label.
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// The paths in `changes`.
#[must_use]
pub fn paths(changes: &[(String, String)]) -> HashSet<String> {
    changes.iter().map(|(path, _)| path.clone()).collect()
}

/// Whether the message shows one of the images at `changed`.
#[must_use]
pub fn shows_image<S: BuildHasher>(message: &Message, changed: &HashSet<String, S>) -> bool {
    message
        .attachments
        .iter()
        .filter_map(|attachment| attachment.image.as_ref())
        .any(|image| changed.contains(&image.path))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rc_core::domain::{Author, Delivery, ImageAttachment, MessageKind};

    use super::*;

    fn image(path: &str, width: Option<u32>, height: Option<u32>) -> Attachment {
        Attachment {
            title: "pic.png".into(),
            link: "/file-upload/a/pic.png".into(),
            description: None,
            size: None,
            image: Some(ImageAttachment {
                path: path.into(),
                width,
                height,
            }),
        }
    }

    fn file() -> Attachment {
        Attachment {
            title: "notes.txt".into(),
            link: "/file-upload/b/notes.txt".into(),
            description: Some("my notes".into()),
            size: Some(1536),
            image: None,
        }
    }

    fn message(attachments: Vec<Attachment>) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".into(),
                display_name: None,
            },
            ts: jiff::Timestamp::UNIX_EPOCH,
            kind: MessageKind::Text("".into()),
            delivery: Delivery::Sent,
            attachments,
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

    fn update(path: &str, file: Option<&str>) -> ImageUpdate {
        ImageUpdate {
            path: path.into(),
            file: file.map(PathBuf::from),
        }
    }

    // RICH-005
    #[test]
    fn sizes_are_in_words() {
        for (bytes, text) in [
            (0, "0 B"),
            (1023, "1023 B"),
            (1024, "1.0 KB"),
            (1536, "1.5 KB"),
            (5 * 1024 * 1024, "5.0 MB"),
            (3 << 30, "3.0 GB"),
            (u64::MAX, "16777216.0 TB"),
        ] {
            assert_eq!(human_size(bytes), text, "{bytes}");
        }
    }

    // RICH-005
    #[test]
    fn an_image_shows_its_file_once_downloaded() {
        let mut urls = ImageUrls::default();
        let message = message(vec![image("/file-upload/t/pic.png", Some(480), Some(360))]);
        let before = attachments(&message, Some("http://h:3000/"), &urls);
        assert_eq!(
            before,
            [AttachmentView {
                title: "pic.png".into(),
                link: "http://h:3000/file-upload/a/pic.png".into(),
                is_image: true,
                width: 480,
                height: 360,
                ..AttachmentView::default()
            }]
        );

        urls.apply(&[update("/file-upload/t/pic.png", Some("/cache/pic.png"))]);
        let after = attachments(&message, Some("http://h:3000/"), &urls);
        assert_eq!(after[0].image, "file:///cache/pic.png?v=1");
    }

    // RICH-005
    #[test]
    fn a_file_has_a_link_a_size_and_a_description() {
        let message = message(vec![file()]);
        let shown = attachments(&message, Some("https://h/team"), &ImageUrls::default());
        assert_eq!(
            shown,
            [AttachmentView {
                title: "notes.txt".into(),
                description: "my notes".into(),
                size: "1.5 KB".into(),
                link: "https://h/team/file-upload/b/notes.txt".into(),
                ..AttachmentView::default()
            }]
        );
    }

    // RICH-005
    #[test]
    fn the_links_are_empty_without_a_server_and_the_title_falls_back_to_the_file_name() {
        let mut nameless = file();
        nameless.title = String::new();
        let shown = attachments(&message(vec![nameless]), None, &ImageUrls::default());
        assert_eq!(shown[0].title, "notes.txt");
        assert_eq!(shown[0].link, "");
    }

    // RICH-005
    #[test]
    fn an_image_of_unknown_size_has_zero_dimensions() {
        let message = message(vec![image("/file-upload/t/pic.png", None, None)]);
        let shown = attachments(&message, Some("http://h"), &ImageUrls::default());
        assert_eq!((shown[0].width, shown[0].height), (0, 0));
    }

    // RICH-005
    #[test]
    fn a_message_without_attachments_shows_none() {
        let shown = attachments(
            &message(Vec::new()),
            Some("http://h"),
            &ImageUrls::default(),
        );
        assert_eq!(shown, []);
    }

    // RICH-005
    #[test]
    fn only_messages_with_a_changed_image_are_announced() {
        let changed: HashSet<String> = ["/file-upload/t/pic.png".to_owned()].into();
        assert!(shows_image(
            &message(vec![file(), image("/file-upload/t/pic.png", None, None)]),
            &changed
        ));
        assert!(!shows_image(
            &message(vec![image("/file-upload/other.png", None, None)]),
            &changed
        ));
        assert!(!shows_image(&message(vec![file()]), &changed));
        assert_eq!(
            paths(&[("/a".into(), "u".into())]),
            ["/a".to_owned()].into()
        );
    }
}
