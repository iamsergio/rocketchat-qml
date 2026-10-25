// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The files the core downloaded for a kind of image, as the `file://` URLs the GUI loads them
//! from. Plain Rust, so it can be unit-tested without a GUI.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::Hash;
use std::path::{Path, PathBuf};

/// A core event's update of the file shown for a key.
pub trait FileUpdate {
    type Key;

    fn key(&self) -> &Self::Key;

    /// The file to show, or `None` if it is gone.
    fn file(&self) -> Option<&Path>;
}

/// The files by key, as URLs. A key without a file has no entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileUrls<K: Eq + Hash> {
    urls: HashMap<K, String>,
    /// Counts how often the key's file was announced: the GUI keeps images by URL, so a replaced
    /// file needs a new one.
    versions: HashMap<K, u64>,
}

impl<K: Eq + Hash> Default for FileUrls<K> {
    fn default() -> Self {
        Self {
            urls: HashMap::new(),
            versions: HashMap::new(),
        }
    }
}

impl<K: Clone + Eq + Hash> FileUrls<K> {
    /// Takes the updates in. Returns the keys whose URL is not what it was, each with its URL
    /// (empty when the file is gone).
    pub fn apply<U: FileUpdate<Key = K>>(&mut self, updates: &[U]) -> Vec<(K, String)> {
        let mut changed = Vec::new();
        for update in updates {
            let key = update.key();
            let version = self.versions.entry(key.clone()).or_default();
            *version += 1;
            let url = if let Some(file) = update.file() {
                let url = file_url(file, *version);
                self.urls.insert(key.clone(), url.clone());
                url
            } else {
                self.urls.remove(key);
                String::new()
            };
            changed.push((key.clone(), url));
        }
        changed
    }

    /// The URL, or nothing if the key has no file.
    #[must_use]
    pub fn url<Q>(&self, key: &Q) -> &str
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.urls.get(key).map_or("", String::as_str)
    }

    /// Takes the URLs of `changes` in, as [`apply`](Self::apply) returned them.
    pub fn set(&mut self, changes: &[(K, String)]) {
        for (key, url) in changes {
            if url.is_empty() {
                self.urls.remove(key);
            } else {
                self.urls.insert(key.clone(), url.clone());
            }
        }
    }
}

/// The `file://` URL of `path`, with `?v=<version>` so that a replaced file is another image to the
/// GUI. The query is not part of the file: Qt takes the path of a local URL.
#[must_use]
pub fn file_url(path: &Path, version: u64) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !text.starts_with('/') {
        // A Windows drive: file:///C:/...
        url.push('/');
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
    let _ = write!(url, "?v={version}");
    url
}

/// The file a URL of [`file_url`] names: without its `?v=` query, the percent escapes decoded, and
/// without the slash before a Windows drive. For a GUI that loads images from paths, where Qt takes
/// the URL as it is. `None` for anything else than a `file://` URL.
#[must_use]
pub fn file_path(url: &str) -> Option<PathBuf> {
    let path = url.strip_prefix("file://")?;
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let escaped = (byte == b'%')
            .then(|| path.get(at + 1..at + 3))
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        if let Some(value) = escaped {
            decoded.push(value);
            at += 3;
        } else {
            decoded.push(byte);
            at += 1;
        }
    }
    let text = String::from_utf8(decoded).ok()?;
    let mut chars = text.chars();
    let drive = chars.next() == Some('/')
        && chars
            .next()
            .is_some_and(|letter| letter.is_ascii_alphabetic())
        && chars.next() == Some(':');
    Some(PathBuf::from(if drive { &text[1..] } else { &text[..] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Update(String, Option<PathBuf>);

    fn update(key: &str, file: Option<&str>) -> Update {
        Update(key.to_owned(), file.map(PathBuf::from))
    }

    impl FileUpdate for Update {
        type Key = String;

        fn key(&self) -> &String {
            &self.0
        }

        fn file(&self) -> Option<&Path> {
            self.1.as_deref()
        }
    }

    // RICH-004, RICH-005
    #[test]
    fn a_file_url_gives_its_file_back() {
        for path in [
            "/home/me/.cache/app/h%3A3000/avatars/user/a b.svg",
            "/tmp/\u{e9}.png",
        ] {
            let url = file_url(Path::new(path), 4);
            assert_eq!(file_path(&url), Some(PathBuf::from(path)), "{url}");
        }
        // A Windows drive loses the slash before it; other URLs aren't files.
        assert_eq!(
            file_path("file:///C:/Users/me/a.png?v=1"),
            Some(PathBuf::from("C:/Users/me/a.png"))
        );
        assert_eq!(file_path("https://h/a.png"), None);
    }

    // RICH-004, RICH-005
    #[test]
    fn a_file_url_escapes_the_path_and_names_the_version() {
        assert_eq!(
            file_url(
                Path::new("/home/me/.cache/app/h%3A3000/avatars/user/a b.svg"),
                3
            ),
            "file:///home/me/.cache/app/h%253A3000/avatars/user/a%20b.svg?v=3"
        );
        assert_eq!(
            file_url(Path::new("C:\\Users\\me\\a.png"), 1),
            "file:///C:/Users/me/a.png?v=1"
        );
    }

    // RICH-004, RICH-005
    #[test]
    fn a_replaced_file_gets_a_new_url_so_that_the_gui_loads_it_again() {
        let mut urls = FileUrls::default();
        let first = urls.apply(&[update("a", Some("/c/a.png"))]);
        let second = urls.apply(&[update("a", Some("/c/a.png"))]);
        assert_eq!(first[0].1, "file:///c/a.png?v=1");
        assert_eq!(second[0].1, "file:///c/a.png?v=2");
        assert_eq!(urls.url("a"), "file:///c/a.png?v=2");
    }

    // RICH-004, RICH-005
    #[test]
    fn a_forgotten_file_has_no_url() {
        let mut urls = FileUrls::default();
        urls.apply(&[update("a", Some("/c/a.png"))]);
        let gone = urls.apply(&[update("a", None)]);
        assert_eq!(gone, [("a".to_owned(), String::new())]);
        assert_eq!(urls.url("a"), "");
        assert_eq!(urls.url("b"), "");
    }

    // RICH-004, RICH-005
    #[test]
    fn a_model_follows_the_cache_through_the_changes() {
        let mut cache = FileUrls::default();
        let mut model = FileUrls::default();
        model.set(&cache.apply(&[update("a", Some("/c/a.png")), update("b", Some("/c/b.png"))]));
        model.set(&cache.apply(&[update("a", None)]));
        assert_eq!(model.url("a"), "");
        assert_eq!(model.url("b"), "file:///c/b.png?v=1");
        assert_eq!(model.url("b"), cache.url("b"));
    }
}
