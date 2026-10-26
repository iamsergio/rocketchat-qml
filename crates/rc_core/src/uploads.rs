// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The files the user is uploading (ACT-009), as the frontend sees them. The queue itself, and the
//! two requests each upload takes, are the account actor's: see `account/uploads.rs`.

use std::fmt;

use crate::domain::{MessageId, RoomId};

/// Names an upload for as long as the account is logged in. Made by the core, in the order the
/// uploads were asked for; it is not a server id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UploadId(pub(crate) u64);

impl UploadId {
    /// An id for the tests of the crates that show uploads: the core makes the real ones.
    #[doc(hidden)]
    #[must_use]
    pub const fn for_test(number: u64) -> Self {
        Self(number)
    }
}

impl fmt::Display for UploadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "upload {}", self.0)
    }
}

/// Why an upload failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UploadError {
    /// The file is bigger than the server takes. Refused without asking the server.
    #[error("the file is larger than the server's limit of {limit} bytes")]
    TooLarge {
        /// The server's limit, in bytes.
        limit: u64,
    },
    /// The server doesn't take uploads. Refused without asking the server.
    #[error("the server doesn't take file uploads")]
    Disabled,
    /// The file is missing, can't be read or isn't a file.
    #[error("can't read the file: {0}")]
    Unreadable(String),
    /// The server said no, in these words (a type it doesn't accept, a room the user isn't in...).
    #[error("the server refused the file: {0}")]
    Refused(String),
    /// The server couldn't be reached, or the connection broke.
    #[error("can't reach the server")]
    Connection,
}

/// Where an upload is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadState {
    /// Waits for the uploads before it to end: they run one at a time.
    Queued,
    /// The file is on its way. `sent` is how many bytes had gone when the state began (0); the
    /// progress comes as [`Event::UploadProgress`](crate::Event::UploadProgress).
    Uploading { sent: u64 },
    /// The server has the file, and is asked to post it as a message. It can't be cancelled any
    /// more: the message may be posted already.
    Confirming,
    /// Ended without a message. It stays listed until it is retried or cancelled.
    Failed(UploadError),
}

/// One file being uploaded, queued or failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upload {
    pub id: UploadId,
    pub room: RoomId,
    /// The parent of the thread it is posted in.
    pub thread: Option<MessageId>,
    /// The file's name, which is the name it has in the message.
    pub name: String,
    /// The file's size in bytes, as it was when the upload was asked for.
    pub size: u64,
    pub state: UploadState,
}
