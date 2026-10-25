// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The custom emoji images the frontend is told about. Only a frontend that can't load a URL
//! asks for them. Like an avatar, and unlike an attachment, an image is downloaded again once per
//! session: an admin can replace an emoji under the same name.

use std::path::PathBuf;

use crate::downloads::Change;

/// A change of the file the frontend shows for a custom emoji.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiImageUpdate {
    /// The name the frontend asked for, without colons.
    pub name: String,
    /// The file to show, or `None` if it is gone (logging out forgets them all).
    pub file: Option<PathBuf>,
}

impl From<Change<String>> for EmojiImageUpdate {
    fn from(change: Change<String>) -> Self {
        Self {
            name: change.key,
            file: change.file,
        }
    }
}
