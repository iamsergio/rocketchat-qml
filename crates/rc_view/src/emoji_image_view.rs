// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The image files of custom emoji, for a frontend that can't load their URLs.

use std::path::Path;

use rc_core::EmojiImageUpdate;

use crate::file_urls::{FileUpdate, FileUrls};

/// The image files by emoji name (without colons), as the `file://` URLs the GUI loads them from.
/// An emoji without a file has no entry.
pub type EmojiImageUrls = FileUrls<String>;

impl FileUpdate for EmojiImageUpdate {
    type Key = String;

    fn key(&self) -> &String {
        &self.name
    }

    fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}
