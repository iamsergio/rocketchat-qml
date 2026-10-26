// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The commands and the views of the files a composer sends (ACT-009): choosing them, staging and
//! unstaging them, sending them with the text, and the uploads under way. Split from `app.rs` to
//! keep it by concern.

use std::path::PathBuf;
use std::rc::Rc;

use rc_core::domain::{MessageId, RoomId};
use rc_view::staged_view::{StagedFile, plan};
use rc_view::startup::test_attachments;
use slint::ComponentHandle;
use slint::winit_030::winit::event::WindowEvent;
use slint::winit_030::{EventResult, WinitWindowAccessor};

use super::App;
use crate::staged_model::StagedModel;
use crate::ui::MainWindow;

impl App {
    /// The files that wait in the composer of the thread, or of the room.
    fn staged(&self, for_thread: bool) -> &Rc<StagedModel> {
        if for_thread {
            &self.thread_staged
        } else {
            &self.room_staged
        }
    }

    /// The paperclip: lets the user choose files for the composer. A test names its files in
    /// `RC_TEST_ATTACH_FILES` (a build with `test-attach-files`), which are taken in place of the
    /// dialog, since a test cannot drive one.
    pub(super) fn attach(&mut self, for_thread: bool) {
        let paths = test_attachments();
        if !paths.is_empty() {
            self.staged(for_thread).add(paths);
            return;
        }
        if self.choosing_files {
            return;
        }
        self.choosing_files = true;
        // The dialog is the desktop's, asked through its portal on a thread of rfd's own; the
        // answer wakes the event loop, and the files go to whatever the composer is by then.
        let spawned = slint::spawn_local(async move {
            let chosen = rfd::AsyncFileDialog::new()
                .set_title("Attach files")
                .pick_files()
                .await;
            App::with(|app| {
                app.choosing_files = false;
                if let Some(chosen) = chosen {
                    app.staged(for_thread)
                        .add(chosen.iter().map(|file| file.path().to_path_buf()));
                }
            });
        });
        if let Err(error) = spawned {
            self.choosing_files = false;
            tracing::warn!(%error, "can't open the file dialog");
        }
    }

    /// The cross of the file in the row.
    pub(super) fn remove_staged(&self, for_thread: bool, row: i32) {
        if let Ok(row) = usize::try_from(row) {
            self.staged(for_thread).remove(row);
        }
    }

    /// Sends the text with the files that wait in the room's composer (`thread` is `None`), or in
    /// the thread's, as `plan` says: the files are uploaded in order, the text with the first one,
    /// or before them as a message of its own when it quotes. The files leave the composer. False
    /// if nothing was sent (no room or thread is open).
    pub(super) fn send_with_files(
        &self,
        for_thread: bool,
        text: &str,
        quoted: &str,
        also_to_channel: bool,
        files: &[StagedFile],
    ) -> bool {
        let cache = self.switchboard.accounts().current_cache();
        let (room, parent) = if for_thread {
            let state = cache.thread_state();
            if state.parent_id.is_empty() || state.room_id.is_empty() {
                return false;
            }
            (
                RoomId::from(state.room_id),
                Some(MessageId::from(state.parent_id)),
            )
        } else {
            let room_id = cache.room_state().room_id;
            if room_id.is_empty() {
                return false;
            }
            (RoomId::from(room_id), None)
        };
        let plan = plan(text, !quoted.is_empty(), files);
        let quote = (!quoted.is_empty()).then(|| MessageId::from(quoted.to_owned()));
        self.current_account("upload_file", |account| {
            if let Some(message) = plan.message {
                account.send_message(
                    room.clone(),
                    &message,
                    quote,
                    parent.clone(),
                    also_to_channel,
                );
            }
            for (path, text) in plan.uploads {
                account.upload_file(room.clone(), path, &text, parent.clone());
            }
        });
        self.staged(for_thread).clear();
        true
    }

    /// Gives up the upload in the row of the room's list, or of the thread's.
    pub(super) fn cancel_upload(&self, for_thread: bool, row: i32) {
        if let Some(id) = self.upload_at(for_thread, row) {
            self.current_account("cancel_upload", |account| account.cancel_upload(id));
        }
    }

    /// Tries the failed upload in the row again.
    pub(super) fn retry_upload(&self, for_thread: bool, row: i32) {
        if let Some(id) = self.upload_at(for_thread, row) {
            self.current_account("retry_upload", |account| account.retry_upload(id));
        }
    }

    fn upload_at(&self, for_thread: bool, row: i32) -> Option<rc_core::uploads::UploadId> {
        let uploads = if for_thread {
            &self.thread_uploads
        } else {
            &self.room_uploads
        };
        usize::try_from(row).ok().and_then(|row| uploads.id_at(row))
    }

    /// Shows the uploads of the open room and of the open thread, as the current account's cache
    /// says: only the rows that differ are announced.
    pub(super) fn show_uploads(&self) {
        let cache = self.switchboard.accounts().current_cache();
        self.room_uploads.show(&cache.room_upload_rows());
        self.thread_uploads.show(&cache.thread_upload_rows());
    }

    /// Whether files dropped on the window can be attached: a room is open and the user is in it.
    fn accepts_drops(window: &MainWindow) -> bool {
        !window.get_room_id().is_empty() && !window.get_room_preview()
    }
}

/// Lets files dragged from the desktop onto the window go to the open room's composer, and the
/// room shows where they would go while they are over it. It uses winit's events, which Slint hands
/// on through its `unstable-winit-030` feature; a window that winit does not back (the tests' one)
/// ignores it. winit reports no position for a drag, so the room's composer takes the files even
/// if they are dropped on the thread pane; and on Wayland winit 0.30 reports no drag at all.
pub(super) fn accept_drops(window: &MainWindow) {
    let weak = window.as_weak();
    window.window().on_winit_window_event(move |_, event| {
        let Some(window) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
            WindowEvent::HoveredFile(_) => window.set_dropping(App::accepts_drops(&window)),
            WindowEvent::HoveredFileCancelled => window.set_dropping(false),
            WindowEvent::DroppedFile(path) => {
                window.set_dropping(false);
                if App::accepts_drops(&window) {
                    let path: PathBuf = path.clone();
                    App::with(|app| app.room_staged.add([path]));
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}
