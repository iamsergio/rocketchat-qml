// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What Ctrl+V does in a composer (ACT-012): which of the clipboard's contents is attached, the
//! name and place of a pasted picture, and which staged files are ours to delete. The toolkit
//! reads the clipboard and saves the picture; every decision is here, so each frontend decides
//! the same.

use std::path::{Component, Path, PathBuf};

use jiff::civil::DateTime;

use crate::file_urls::file_path;

/// The directory of the cache directory that holds the pasted pictures.
const PASTED_DIR: &str = "pasted";

/// What the clipboard holds, as far as a paste cares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clipboard {
    /// The URLs it lists (a copy in a file manager).
    pub urls: Vec<String>,
    /// Its plain text, empty if it has none.
    pub text: String,
    pub has_image: bool,
}

/// What a paste does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Paste {
    /// Attach these files.
    Files(Vec<PathBuf>),
    /// Attach the picture, saved to a file first.
    Image,
    /// Not ours: the text field pastes whatever it pastes.
    Pass,
}

/// What pasting `clipboard` does: copied local files first, then text (which wins over a picture,
/// since a spreadsheet puts both on the clipboard), then a picture.
#[must_use]
pub fn decide(clipboard: &Clipboard) -> Paste {
    let files: Option<Vec<PathBuf>> = clipboard.urls.iter().map(|url| file_path(url)).collect();
    match files {
        Some(files) if !files.is_empty() => Paste::Files(files),
        _ if !clipboard.text.is_empty() => Paste::Pass,
        _ if clipboard.has_image => Paste::Image,
        _ => Paste::Pass,
    }
}

/// The directory for pasted pictures inside `cache_dir`.
#[must_use]
pub fn pasted_dir(cache_dir: &Path) -> PathBuf {
    cache_dir.join(PASTED_DIR)
}

/// Where a picture pasted at `now` is saved in `dir`: `Pasted image 2026-10-28 14-03-05.png`, with
/// ` 2`, ` 3`... added while `exists` says the name is taken. The name is the one the server shows.
#[must_use]
pub fn pasted_path(dir: &Path, now: DateTime, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let stem = format!(
        "Pasted image {:04}-{:02}-{:02} {:02}-{:02}-{:02}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let first = dir.join(format!("{stem}.png"));
    if !exists(&first) {
        return first;
    }
    (2..=u32::MAX)
        .map(|n| dir.join(format!("{stem} {n}.png")))
        .find(|path| !exists(path))
        .unwrap_or(first)
}

/// Whether `path` is a file of the pasted directory `dir`, as opposed to one of the user's own.
/// Looks at the path only: a path that climbs out with `..` is not.
#[must_use]
pub fn is_pasted(dir: &Path, path: &Path) -> bool {
    path.parent() == Some(dir) && !path.components().any(|part| part == Component::ParentDir)
}

/// Empties `dir` of what an earlier run left, creating it if need be.
///
/// # Errors
///
/// If the directory can't be emptied or created.
pub fn reset_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::create_dir_all(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clipboard(urls: &[&str], text: &str, has_image: bool) -> Clipboard {
        Clipboard {
            urls: urls.iter().map(|url| (*url).to_owned()).collect(),
            text: text.to_owned(),
            has_image,
        }
    }

    fn moment() -> DateTime {
        jiff::civil::date(2026, 3, 9).at(4, 5, 6, 0)
    }

    // ACT-012
    #[test]
    fn copied_local_files_are_attached_whatever_else_the_clipboard_holds() {
        let held = clipboard(&["file:///tmp/a%20b.txt", "file:///tmp/c.png"], "a b", true);
        assert_eq!(
            decide(&held),
            Paste::Files(vec!["/tmp/a b.txt".into(), "/tmp/c.png".into()])
        );
    }

    // ACT-012
    #[test]
    fn one_url_that_is_not_a_local_file_leaves_the_paste_to_the_field() {
        let held = clipboard(
            &["file:///tmp/a.txt", "https://example.com/x.png"],
            "",
            false,
        );
        assert_eq!(decide(&held), Paste::Pass);
        // With a picture next to them, the picture is attached instead.
        let held = clipboard(&["https://example.com/x.png"], "", true);
        assert_eq!(decide(&held), Paste::Image);
    }

    // ACT-012
    #[test]
    fn text_wins_over_a_picture() {
        assert_eq!(decide(&clipboard(&[], "cells", true)), Paste::Pass);
    }

    // ACT-012
    #[test]
    fn a_picture_alone_is_attached_and_nothing_at_all_is_left_to_the_field() {
        assert_eq!(decide(&clipboard(&[], "", true)), Paste::Image);
        assert_eq!(decide(&clipboard(&[], "", false)), Paste::Pass);
    }

    // ACT-012
    #[test]
    fn a_pasted_picture_is_named_after_the_second_and_numbered_if_it_exists() {
        let dir = Path::new("/cache/pasted");
        assert_eq!(
            pasted_path(dir, moment(), |_| false),
            Path::new("/cache/pasted/Pasted image 2026-03-09 04-05-06.png")
        );
        let taken = |path: &Path| {
            path.ends_with("Pasted image 2026-03-09 04-05-06.png")
                || path.ends_with("Pasted image 2026-03-09 04-05-06 2.png")
        };
        assert_eq!(
            pasted_path(dir, moment(), taken),
            Path::new("/cache/pasted/Pasted image 2026-03-09 04-05-06 3.png")
        );
    }

    // ACT-012
    #[test]
    fn only_files_directly_in_the_pasted_directory_are_ours() {
        let dir = Path::new("/cache/pasted");
        assert!(is_pasted(dir, Path::new("/cache/pasted/Pasted image.png")));
        assert!(!is_pasted(dir, Path::new("/home/me/Pasted image.png")));
        assert!(!is_pasted(dir, Path::new("/cache/pasted/../other.png")));
        assert!(!is_pasted(dir, Path::new("/cache/pasted/sub/a.png")));
        assert!(!is_pasted(dir, Path::new("/cache/pasted-not/a.png")));
    }

    // ACT-012
    #[test]
    fn the_pasted_directory_is_emptied_and_made() {
        let base = std::env::temp_dir().join(format!("rc_view_paste_{}", std::process::id()));
        let dir = pasted_dir(&base);
        reset_dir(&dir).unwrap();
        std::fs::write(dir.join("old.png"), b"x").unwrap();
        reset_dir(&dir).unwrap();
        assert!(dir.is_dir());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
