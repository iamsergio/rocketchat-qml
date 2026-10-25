// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The avatars the frontend is told about, and which ones the messages show.

use std::path::PathBuf;

use crate::domain::{AvatarOwner, MessageKind};
use crate::downloads::Change;
use crate::timeline::TimelineChange;

/// A change of the avatar file the frontend shows for an owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarUpdate {
    pub owner: AvatarOwner,
    /// The file to show, or `None` if the avatar is gone (logging out forgets them all).
    pub path: Option<PathBuf>,
}

impl From<Change<AvatarOwner>> for AvatarUpdate {
    fn from(change: Change<AvatarOwner>) -> Self {
        Self {
            owner: change.key,
            path: change.file,
        }
    }
}

/// The owners of the avatars of the people who wrote the messages in `changes`. System messages
/// show no avatar.
pub(crate) fn authors_of(changes: &[TimelineChange]) -> Vec<AvatarOwner> {
    changes
        .iter()
        .flat_map(TimelineChange::messages)
        .filter(|message| matches!(message.kind, MessageKind::Text(_)))
        .map(|message| AvatarOwner::User(message.author.username.clone()))
        .collect()
}
