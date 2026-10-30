// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Uploading files (ACT-009): the account's queue, and the two requests each upload takes. See
//! docs/design/uploads.md.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::task::JoinHandle;

use crate::client::Event;
use crate::domain::{Message, MessageId, RoomId};
use crate::ports::{ApiError, Connector, ServerApi};
use crate::session::SessionState;
use crate::uploads::{Upload, UploadError, UploadId, UploadState};

use super::{Actor, Done};

/// What the server says about uploads: its public settings, as far as the core applies them
/// itself. Until they are known the server decides, and its refusal is shown.
#[derive(Clone, Copy)]
struct Rules {
    enabled: bool,
    /// In bytes. `None` for no limit.
    max_bytes: Option<u64>,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bytes: None,
        }
    }
}

/// An upload with what the actor needs to run it.
struct Entry {
    upload: Upload,
    text: String,
    /// The server's id of the file, once it has it: a retry then only posts the message.
    file_id: Option<String>,
}

/// The account's uploads.
pub(super) struct Uploads {
    /// The number of the next [`UploadId`]. Not reset by a logout, so that an id is never two
    /// uploads.
    next: u64,
    /// In the order they were asked for, the failed ones too.
    entries: Vec<Entry>,
    /// The upload whose request is in flight, and its task. At most one: the others wait.
    running: Option<(UploadId, JoinHandle<()>)>,
    rules: Rules,
}

impl Uploads {
    pub(super) fn new() -> Self {
        Self {
            next: 0,
            entries: Vec::new(),
            running: None,
            rules: Rules::default(),
        }
    }

    fn is_running(&self, id: UploadId) -> bool {
        self.running
            .as_ref()
            .is_some_and(|(running, _)| *running == id)
    }

    fn position(&self, id: UploadId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.upload.id == id)
    }
}

/// The answer of the first request of an upload: the file is at the server, or why not.
pub(super) struct UploadStoredDone {
    /// The login attempt the upload belongs to: the answer is stale if another has begun since.
    attempt: u64,
    id: UploadId,
    outcome: Result<String, ApiError>,
}

/// The answer of the second request: the message, or why not.
pub(super) struct UploadConfirmedDone {
    attempt: u64,
    id: UploadId,
    outcome: Result<Message, ApiError>,
}

/// How far the first request has got.
pub(super) struct UploadProgressDone {
    attempt: u64,
    id: UploadId,
    sent: u64,
}

impl<C: Connector> Actor<C> {
    pub(super) fn upload_file(
        &mut self,
        room: RoomId,
        path: PathBuf,
        text: &str,
        thread: Option<MessageId>,
    ) {
        if self.own_user().is_none() {
            tracing::debug!("upload_file ignored: not logged in");
            return;
        }
        if self.rooms.get(&room).is_none() {
            tracing::debug!(%room, "upload_file ignored: not in the room list");
            return;
        }
        if let Some(parent) = &thread {
            let open = self.open_thread.as_ref().is_some_and(|thread| {
                &thread.parent.id == parent && thread.timeline.room() == &room
            });
            if !open {
                tracing::debug!(%parent, "upload_file ignored: not the open thread");
                return;
            }
        }
        let name = file_name(&path);
        let (size, state) = self.assess(&path);
        let id = UploadId(self.uploads.next);
        self.uploads.next += 1;
        self.uploads.entries.push(Entry {
            upload: Upload {
                id,
                room,
                thread,
                name,
                path,
                size,
                state,
            },
            text: text.trim().to_owned(),
            file_id: None,
        });
        self.start_next_upload();
        self.tell_uploads();
    }

    pub(super) fn cancel_upload(&mut self, id: UploadId) {
        let Some(index) = self.uploads.position(id) else {
            tracing::debug!(%id, "cancel_upload ignored: no such upload");
            return;
        };
        match self.uploads.entries[index].upload.state {
            // The message may be posted already.
            UploadState::Confirming => {
                tracing::debug!(%id, "cancel_upload ignored: confirming");
                return;
            }
            UploadState::Uploading { .. } => {
                if let Some((_, task)) = self.uploads.running.take() {
                    task.abort();
                }
            }
            UploadState::Queued | UploadState::Failed(_) => {}
        }
        self.uploads.entries.remove(index);
        self.start_next_upload();
        self.tell_uploads();
    }

    pub(super) fn retry_upload(&mut self, id: UploadId) {
        let Some(index) = self.uploads.position(id) else {
            tracing::debug!(%id, "retry_upload ignored: no such upload");
            return;
        };
        let entry = &self.uploads.entries[index];
        if !matches!(entry.upload.state, UploadState::Failed(_)) {
            tracing::debug!(%id, "retry_upload ignored: not failed");
            return;
        }
        // The file may have appeared, or changed, since; the server's copy is what it is.
        if entry.file_id.is_some() {
            self.uploads.entries[index].upload.state = UploadState::Queued;
        } else {
            let (size, state) = self.assess(&entry.upload.path);
            let upload = &mut self.uploads.entries[index].upload;
            upload.size = size;
            upload.state = state;
        }
        self.start_next_upload();
        self.tell_uploads();
    }

    /// The size of the file, and whether the upload is queued or fails at once: the checks that
    /// need no network. The size is 0 for a file that can't be read.
    fn assess(&self, path: &Path) -> (u64, UploadState) {
        // A stat of a local file: quick enough for the actor.
        let size = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => return failed(0, UploadError::Unreadable("not a file".to_owned())),
            Err(error) => return failed(0, UploadError::Unreadable(error.to_string())),
        };
        let rules = self.uploads.rules;
        if !rules.enabled {
            return failed(size, UploadError::Disabled);
        }
        match rules.max_bytes {
            Some(limit) if size > limit => failed(size, UploadError::TooLarge { limit }),
            _ => (size, UploadState::Queued),
        }
    }

    /// Takes in the server's upload settings, which apply to the uploads asked for from now on.
    pub(super) fn set_upload_rules(&mut self, settings: &rc_api::MessageSettings) {
        self.uploads.rules = Rules {
            enabled: settings.uploads_enabled,
            max_bytes: settings.upload_max_bytes,
        };
    }

    /// Starts the first queued upload, unless one is under way.
    fn start_next_upload(&mut self) {
        if self.uploads.running.is_some() {
            return;
        }
        if let Some(index) = (self.uploads.entries.iter())
            .position(|entry| entry.upload.state == UploadState::Queued)
        {
            self.run_upload(index);
        }
    }

    /// Runs the upload in a task: the file's request, or only the message's if the server has the
    /// file already.
    fn run_upload(&mut self, index: usize) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        let entry = &mut self.uploads.entries[index];
        let id = entry.upload.id;
        let room = entry.upload.room.clone();
        let task = if let Some(file) = entry.file_id.clone() {
            entry.upload.state = UploadState::Confirming;
            let text = entry.text.clone();
            let thread = entry.upload.thread.clone();
            tokio::spawn(async move {
                let outcome = api
                    .confirm_upload(&credentials, &room, &file, &text, thread.as_ref())
                    .await;
                // Fails only if the actor is gone.
                let _ = done.send(Done::UploadConfirmed(UploadConfirmedDone {
                    attempt,
                    id,
                    outcome,
                }));
            })
        } else {
            entry.upload.state = UploadState::Uploading { sent: 0 };
            let path = entry.upload.path.clone();
            let name = entry.upload.name.clone();
            let size = entry.upload.size;
            let progress_to = done.clone();
            // Every whole percent once: a big file would otherwise tell the frontend thousands of
            // times.
            let told = AtomicU64::new(0);
            let progress = move |sent: u64| {
                let percent = (sent.min(size).saturating_mul(100))
                    .checked_div(size)
                    .unwrap_or(100);
                if told.fetch_max(percent, Ordering::Relaxed) < percent {
                    let _ = progress_to.send(Done::UploadProgress(UploadProgressDone {
                        attempt,
                        id,
                        sent,
                    }));
                }
            };
            tokio::spawn(async move {
                let outcome = api
                    .upload_file(&credentials, &room, &path, &name, progress)
                    .await;
                let _ = done.send(Done::UploadStored(UploadStoredDone {
                    attempt,
                    id,
                    outcome,
                }));
            })
        };
        self.uploads.running = Some((id, task));
    }

    pub(super) fn handle_upload_progress(&mut self, done: &UploadProgressDone) {
        if done.attempt != self.attempt || !self.uploads.is_running(done.id) {
            return;
        }
        let Some(index) = self.uploads.position(done.id) else {
            return;
        };
        let state = &mut self.uploads.entries[index].upload.state;
        if matches!(state, UploadState::Uploading { .. }) {
            *state = UploadState::Uploading { sent: done.sent };
            (self.on_event)(Event::UploadProgress {
                id: done.id,
                sent: done.sent,
            });
        }
    }

    pub(super) fn handle_upload_stored(&mut self, done: UploadStoredDone) {
        if !self.current_upload(done.attempt, done.id) {
            tracing::debug!("answer of a stale upload dropped");
            return;
        }
        let Some(index) = self.uploads.position(done.id) else {
            return;
        };
        match done.outcome {
            Ok(file) => {
                // The message follows at once: the upload keeps its turn.
                self.uploads.entries[index].file_id = Some(file);
                self.uploads.running = None;
                self.run_upload(index);
            }
            Err(error) => self.upload_failed(index, &error),
        }
        self.tell_uploads_unless_expired();
    }

    pub(super) fn handle_upload_confirmed(&mut self, done: UploadConfirmedDone) {
        if !self.current_upload(done.attempt, done.id) {
            tracing::debug!("answer of a stale upload dropped");
            return;
        }
        let Some(index) = self.uploads.position(done.id) else {
            return;
        };
        match done.outcome {
            Ok(message) => {
                self.uploads.entries.remove(index);
                self.uploads.running = None;
                // Like the realtime echo of the message, which may come first or later: it finds
                // the message held, and nothing shows twice.
                self.add_live_message(&message);
                self.start_next_upload();
            }
            Err(error) => self.upload_failed(index, &error),
        }
        self.tell_uploads_unless_expired();
    }

    /// The answer is of the upload that runs now, in this session.
    fn current_upload(&self, attempt: u64, id: UploadId) -> bool {
        attempt == self.attempt
            && matches!(self.state, SessionState::LoggedIn(_))
            && self.uploads.is_running(id)
    }

    /// The running upload ended without a message. An expired token ends the session instead.
    fn upload_failed(&mut self, index: usize, error: &ApiError) {
        self.uploads.running = None;
        if *error == ApiError::Unauthorized {
            self.expire_session();
            return;
        }
        let entry = &mut self.uploads.entries[index];
        tracing::warn!(%error, upload = %entry.upload.id, "can't upload the file");
        entry.upload.state = UploadState::Failed(upload_error(error));
        self.start_next_upload();
    }

    /// Tells the list, unless the session ended meanwhile and took the uploads with it.
    fn tell_uploads_unless_expired(&mut self) {
        if matches!(self.state, SessionState::LoggedIn(_)) {
            self.tell_uploads();
        }
    }

    fn tell_uploads(&self) {
        let uploads = (self.uploads.entries.iter())
            .map(|entry| entry.upload.clone())
            .collect();
        (self.on_event)(Event::Uploads(uploads));
    }

    /// Stops the request in flight. For stopping the actor.
    pub(super) fn abort_uploads(&mut self) {
        if let Some((_, task)) = self.uploads.running.take() {
            task.abort();
        }
    }

    /// Forgets all the uploads, ends the request in flight, and tells the frontend, unless there
    /// were none. The server's settings are forgotten too.
    pub(super) fn clear_uploads(&mut self) {
        self.abort_uploads();
        self.uploads.rules = Rules::default();
        if !self.uploads.entries.is_empty() {
            self.uploads.entries.clear();
            (self.on_event)(Event::Uploads(Vec::new()));
        }
    }
}

fn failed(size: u64, error: UploadError) -> (u64, UploadState) {
    (size, UploadState::Failed(error))
}

/// What the file is called in the message: the last part of its path.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn upload_error(error: &ApiError) -> UploadError {
    match error {
        ApiError::Unreachable(_) => UploadError::Connection,
        ApiError::File(text) => UploadError::Unreadable(text.clone()),
        ApiError::Server { message, .. } => UploadError::Refused(message.clone()),
        ApiError::Unexpected(text) => UploadError::Refused(text.clone()),
        other => UploadError::Refused(other.to_string()),
    }
}
