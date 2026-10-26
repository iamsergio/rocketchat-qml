// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The files the user attached to the composer and has not sent yet (ACT-009), and the thumbnail
//! both they and the upload rows show. Plain Rust, so each frontend shows the same.

use std::path::{Path, PathBuf};

use crate::attachment_view::human_size;
use crate::file_urls::file_url;

/// The extensions the thumbnail shows the file itself for.
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// The longest extension the thumbnail names: more would not fit in it.
const LABEL_LENGTH: usize = 4;

/// What the small square before a file's name shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thumbnail {
    /// The thumbnail is the picture itself: `url` is set.
    pub is_image: bool,
    /// The `file://` URL of the picture. Empty unless `is_image`.
    pub url: String,
    /// The extension in capitals, for a file that is not a picture: `LOG`.
    pub label: String,
}

impl Thumbnail {
    /// The thumbnail of the file at `path` that is called `name`.
    #[must_use]
    pub fn of(name: &str, path: &Path) -> Self {
        let extension = Path::new(name)
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
            Self {
                is_image: true,
                url: file_url(path, 0),
                label: String::new(),
            }
        } else {
            Self {
                is_image: false,
                url: String::new(),
                label: extension
                    .to_uppercase()
                    .chars()
                    .take(LABEL_LENGTH)
                    .collect(),
            }
        }
    }
}

/// A file waiting in the composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedFile {
    pub path: PathBuf,
    pub name: String,
    /// In words: `1.5 MB`. Empty if the file can't be read: the upload then says why it fails.
    pub size: String,
    pub thumbnail: Thumbnail,
}

impl StagedFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let size = std::fs::metadata(&path)
            .map(|meta| human_size(meta.len()))
            .unwrap_or_default();
        Self {
            thumbnail: Thumbnail::of(&name, &path),
            path,
            name,
            size,
        }
    }
}

/// The files waiting in one composer, in the order they were attached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Staged {
    files: Vec<StagedFile>,
}

impl Staged {
    #[must_use]
    pub fn files(&self) -> &[StagedFile] {
        &self.files
    }

    /// Whether the file at `path` waits already.
    #[must_use]
    pub fn has(&self, path: &Path) -> bool {
        self.files.iter().any(|file| file.path == path)
    }

    /// Adds the files, in order, except the ones that wait already. Returns the files added.
    pub fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> Vec<StagedFile> {
        let mut added = Vec::new();
        for path in paths {
            if self.has(&path) {
                continue;
            }
            let file = StagedFile::new(path);
            self.files.push(file.clone());
            added.push(file);
        }
        added
    }

    /// Takes the file at `index` out, if there is one.
    pub fn remove(&mut self, index: usize) -> Option<StagedFile> {
        (index < self.files.len()).then(|| self.files.remove(index))
    }

    /// Takes all out.
    pub fn take(&mut self) -> Vec<StagedFile> {
        std::mem::take(&mut self.files)
    }
}

/// What sending the composer's text with staged files does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The text to send first as a message of its own, because it quotes a message and an upload
    /// can't carry a quote.
    pub message: Option<String>,
    /// The uploads in the order the files were attached, each with the text of its message.
    pub uploads: Vec<(PathBuf, String)>,
}

/// How `text` and the `files` are sent: the text goes with the first file as its message, unless
/// the composer quotes a message, in which case it is a message of its own, before the files,
/// which then have no text.
#[must_use]
pub fn plan(text: &str, quoting: bool, files: &[StagedFile]) -> Plan {
    let text = text.trim();
    let own = quoting && !text.is_empty();
    Plan {
        message: own.then(|| text.to_owned()),
        uploads: files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let with = if index == 0 && !quoting { text } else { "" };
                (file.path.clone(), with.to_owned())
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(names: &[&str]) -> Vec<StagedFile> {
        names
            .iter()
            .map(|name| StagedFile::new(PathBuf::from(format!("/nonexistent/{name}"))))
            .collect()
    }

    // ACT-009
    #[test]
    fn a_picture_is_its_own_thumbnail_and_any_other_file_names_its_extension() {
        let png = Thumbnail::of("Photo.PNG", Path::new("/tmp/a b/Photo.PNG"));
        assert!(png.is_image);
        assert_eq!(png.url, "file:///tmp/a%20b/Photo.PNG?v=0");
        assert!(png.label.is_empty());

        let log = Thumbnail::of("run.log", Path::new("/tmp/run.log"));
        assert!(!log.is_image && log.url.is_empty());
        assert_eq!(log.label, "LOG");
        assert_eq!(Thumbnail::of("x.markdown", Path::new("/x")).label, "MARK");
        assert_eq!(Thumbnail::of("README", Path::new("/README")).label, "");
    }

    // ACT-009
    #[test]
    fn a_staged_file_has_its_name_and_its_size_in_words() {
        let dir = std::env::temp_dir().join(format!("rc_view_staged_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.txt");
        std::fs::write(&path, vec![b'x'; 1536]).unwrap();

        let file = StagedFile::new(path);
        assert_eq!(
            (file.name.as_str(), file.size.as_str()),
            ("notes.txt", "1.5 KB")
        );
        assert_eq!(file.thumbnail.label, "TXT");
        assert_eq!(StagedFile::new(dir.join("gone.txt")).size, "");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ACT-009
    #[test]
    fn files_are_added_in_order_once_and_removed_by_index() {
        let mut files = Staged::default();
        let added = files.add(["/a/one.txt", "/a/two.png", "/a/one.txt"].map(PathBuf::from));
        assert_eq!(added.len(), 2);
        assert_eq!(files.add([PathBuf::from("/a/two.png")]).len(), 0);
        let names = |files: &Staged| {
            files
                .files()
                .iter()
                .map(|file| file.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&files), ["one.txt", "two.png"]);
        assert!(files.remove(5).is_none());
        assert_eq!(
            files.remove(0).map(|file| file.name),
            Some("one.txt".into())
        );
        assert_eq!(names(&files), ["two.png"]);
        assert_eq!(files.take().len(), 1);
        assert!(files.files().is_empty());
    }

    // ACT-009
    #[test]
    fn the_text_goes_with_the_first_file() {
        let files = staged(&["a.txt", "b.txt"]);
        let plan = plan("  hello ", false, &files);
        assert_eq!(plan.message, None);
        assert_eq!(
            plan.uploads,
            [
                (PathBuf::from("/nonexistent/a.txt"), "hello".to_owned()),
                (PathBuf::from("/nonexistent/b.txt"), String::new())
            ]
        );
    }

    // ACT-009
    #[test]
    fn a_quoting_text_is_a_message_before_the_files_which_have_no_text() {
        let files = staged(&["a.txt"]);
        let quoted = plan("hello", true, &files);
        assert_eq!(quoted.message.as_deref(), Some("hello"));
        assert_eq!(quoted.uploads[0].1, "");
        let blank = plan(" ", true, &files);
        assert_eq!(blank.message, None);
        assert_eq!(blank.uploads.len(), 1);
    }
}
