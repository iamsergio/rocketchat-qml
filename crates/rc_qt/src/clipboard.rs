// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Reading the system clipboard for a paste into a composer (ACT-012). cxx-qt-lib binds neither
//! `QClipboard` nor `QMimeData` (CXX-QT.md), so `clipboard_probe.cpp` reports what the clipboard
//! holds and saves its picture; what to do about it is `rc_view::paste`.

#[cxx_qt::bridge(namespace = "rc_qt")]
mod ffi {
    unsafe extern "C++" {
        include!("rc_qt/src/clipboard_probe.h");

        fn clipboard_urls() -> Vec<String>;
        fn clipboard_text() -> String;
        fn clipboard_has_image() -> bool;
        fn clipboard_save_image(path: &str) -> bool;
        fn clipboard_seed(paths: &Vec<String>, text: &str, image_path: &str);
    }
}

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rc_view::paste::{self, Clipboard, Paste};

/// Where pasted pictures are saved: set once by [`init`].
static PASTED_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Empties the directory of pasted pictures inside `cache_dir`, which an earlier run left, and
/// remembers it. Call on the GUI thread once the application exists, as the core is started.
pub fn init(cache_dir: &Path) {
    let dir = paste::pasted_dir(cache_dir);
    if let Err(error) = paste::reset_dir(&dir) {
        tracing::warn!(%error, dir = %dir.display(), "can't empty the pasted files directory");
    }
    // A second start keeps the first directory: the cache directory doesn't change.
    let _ = PASTED_DIR.set(dir);
    if let Some((files, text, image)) = test_clipboard() {
        ffi::clipboard_seed(&files, &text, &image);
    }
}

/// The files a paste into a composer attaches, or none if the text field should paste instead.
/// Saves a picture of the clipboard to a file of its own.
pub fn files_to_paste() -> Vec<PathBuf> {
    let held = Clipboard {
        urls: ffi::clipboard_urls(),
        text: ffi::clipboard_text(),
        has_image: ffi::clipboard_has_image(),
    };
    match paste::decide(&held) {
        Paste::Files(files) => files,
        Paste::Image => save_image().into_iter().collect(),
        Paste::Pass => Vec::new(),
    }
}

fn save_image() -> Option<PathBuf> {
    let dir = PASTED_DIR.get()?;
    if let Err(error) = std::fs::create_dir_all(dir) {
        tracing::warn!(%error, "can't make the pasted files directory");
        return None;
    }
    let now = jiff::Zoned::now().datetime();
    let path = paste::pasted_path(dir, now, Path::exists);
    if ffi::clipboard_save_image(&path.to_string_lossy()) {
        Some(path)
    } else {
        tracing::warn!(path = %path.display(), "can't save the pasted picture");
        None
    }
}

/// Deletes the file of a chip that was taken out, if we made it by pasting. The user's own files
/// are never touched.
pub fn forget(path: &Path) {
    let Some(dir) = PASTED_DIR.get() else {
        return;
    };
    if paste::is_pasted(dir, path)
        && let Err(error) = std::fs::remove_file(path)
    {
        tracing::warn!(%error, path = %path.display(), "can't delete the pasted file");
    }
}

/// What the GUI tests ask to find on the clipboard, if anything: the files of
/// `RC_TEST_CLIPBOARD_FILES` (separated as `PATH` is), the text of `RC_TEST_CLIPBOARD_TEXT` and the
/// picture in the file `RC_TEST_CLIPBOARD_IMAGE`. A test can't copy anything. Only builds with
/// `test-attach-files` read them.
#[cfg(feature = "test-attach-files")]
fn test_clipboard() -> Option<(Vec<String>, String, String)> {
    let files = std::env::var_os("RC_TEST_CLIPBOARD_FILES");
    let text = std::env::var("RC_TEST_CLIPBOARD_TEXT").ok();
    let image = std::env::var("RC_TEST_CLIPBOARD_IMAGE").ok();
    if files.is_none() && text.is_none() && image.is_none() {
        return None;
    }
    let files = files
        .iter()
        .flat_map(std::env::split_paths)
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    Some((files, text.unwrap_or_default(), image.unwrap_or_default()))
}

/// Without the feature a user's environment means nothing.
#[cfg(not(feature = "test-attach-files"))]
fn test_clipboard() -> Option<(Vec<String>, String, String)> {
    None
}
