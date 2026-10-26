// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the frontends show of the files being uploaded (ACT-009): a row per upload of the open
//! room or the open thread, with the size and the error in words, and the fine-grained changes
//! that keep a model's rows in step as the core's list changes. Plain Rust, so each frontend shows
//! the same.

use rc_core::domain::{MessageId, RoomId};
use rc_core::list_change::ListChange;
use rc_core::uploads::{Upload, UploadError, UploadId, UploadState};

use crate::attachment_view::human_size;
use crate::staged_view::Thumbnail;

/// Where the uploads shown are posted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place<'a> {
    /// The open room: the uploads that are not in a thread.
    Room(&'a RoomId),
    /// The open thread, by its parent. The thread belongs to the open room.
    Thread(&'a MessageId),
}

/// Where an upload is, as a row says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadStatus {
    /// Waits for the uploads before it.
    Queued,
    Uploading,
    /// The server has the file and posts the message.
    Confirming,
    Failed,
}

impl UploadStatus {
    /// How a frontend names the status in its roles and element names.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Uploading => "uploading",
            Self::Confirming => "confirming",
            Self::Failed => "failed",
        }
    }
}

/// One row of the uploads under the messages.
#[derive(Clone, Debug, PartialEq)]
pub struct UploadRow {
    /// What the commands to cancel and retry take, and what the changes address the row by.
    pub id: UploadId,
    pub name: String,
    /// In words: `1.5 MB`.
    pub size: String,
    /// What the row says under the name: `1.8 of 4.2 MB`, `Waiting · 4.2 MB`, `Posting…`, or why
    /// it failed.
    pub status_line: String,
    /// What the thumbnail shows.
    pub thumbnail: Thumbnail,
    /// How much has gone, from 0 to 1. Nothing for a queued or a failed upload (0), all of it
    /// while the message is posted (1).
    pub progress: f64,
    pub status: UploadStatus,
    /// Why it failed, in words. Empty unless `status` is `Failed`.
    pub error: String,
    /// A failed upload can be tried again.
    pub can_retry: bool,
    /// Everything but an upload that is posting its message can be cancelled.
    pub can_cancel: bool,
}

/// The rows of the uploads that are posted at `place`, in the order the uploads were asked for.
#[must_use]
pub fn rows(uploads: &[Upload], place: Place<'_>) -> Vec<UploadRow> {
    uploads
        .iter()
        .filter(|upload| match place {
            Place::Room(room) => upload.thread.is_none() && upload.room == *room,
            Place::Thread(parent) => upload.thread.as_ref() == Some(parent),
        })
        .map(row)
        .collect()
}

/// The row of one upload.
#[must_use]
pub fn row(upload: &Upload) -> UploadRow {
    let size = human_size(upload.size);
    let (status, progress, error, status_line) = match &upload.state {
        UploadState::Queued => (
            UploadStatus::Queued,
            0.0,
            String::new(),
            format!("Waiting \u{b7} {size}"),
        ),
        UploadState::Uploading { sent } => (
            UploadStatus::Uploading,
            fraction(*sent, upload.size),
            String::new(),
            sent_of(*sent, upload.size),
        ),
        UploadState::Confirming => (
            UploadStatus::Confirming,
            1.0,
            String::new(),
            "Posting\u{2026}".to_owned(),
        ),
        UploadState::Failed(error) => {
            let words = error_in_words(error);
            (UploadStatus::Failed, 0.0, words.clone(), words)
        }
    };
    UploadRow {
        id: upload.id,
        name: upload.name.clone(),
        size,
        status_line,
        thumbnail: Thumbnail::of(&upload.name, &upload.path),
        progress,
        status,
        error,
        can_retry: status == UploadStatus::Failed,
        can_cancel: status != UploadStatus::Confirming,
    }
}

/// How much of a file has gone, in the unit of the whole: `1.8 of 4.2 MB`.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn sent_of(sent: u64, size: u64) -> String {
    let whole = human_size(size);
    let sent = sent.min(size);
    let Some((_, unit)) = whole.split_once(' ').filter(|(_, unit)| *unit != "B") else {
        return format!("{sent} of {whole}");
    };
    let power = ["KB", "MB", "GB", "TB"]
        .iter()
        .position(|name| *name == unit)
        .map_or(0, |index| index + 1);
    let scaled = sent as f64 / 1024_f64.powi(i32::try_from(power).unwrap_or(0));
    format!("{scaled:.1} of {whole}")
}

/// `sent` of `size`, from 0 to 1.
#[allow(clippy::cast_precision_loss)]
fn fraction(sent: u64, size: u64) -> f64 {
    if size == 0 {
        0.0
    } else {
        (sent as f64 / size as f64).clamp(0.0, 1.0)
    }
}

/// Why an upload failed, for the row.
#[must_use]
pub fn error_in_words(error: &UploadError) -> String {
    match error {
        UploadError::TooLarge { limit } => format!(
            "The file is larger than the server's limit of {}",
            // "100 MB", not "100.0 MB".
            human_size(*limit).replace(".0 ", " ")
        ),
        UploadError::Disabled => "The server doesn't take file uploads".to_owned(),
        UploadError::Unreadable(why) => format!("Can't read the file: {why}"),
        UploadError::Refused(why) => {
            format!("The server refused the file: {}", without_code(why))
        }
        UploadError::Connection => "Can't reach the server".to_owned(),
    }
}

/// The server's words without the code it ends them with in brackets: `File type is not accepted.
/// [error-invalid-file-type]` is for the logs, not for the user.
fn without_code(why: &str) -> &str {
    let why = why.trim();
    match why.strip_suffix(']').and_then(|rest| rest.rsplit_once('[')) {
        Some((words, _)) if !words.trim().is_empty() => words.trim_end(),
        _ => why,
    }
}

/// The changes that bring a model holding `old` to `new`: removals, then updates of the rows that
/// differ (a progress is one), then insertions, all by [`UploadId`]. Both lists are in the order of
/// the ids, as the core lists them. A row that keeps its place and its content costs nothing.
#[must_use]
pub fn changes(old: &[UploadRow], new: &[UploadRow]) -> Vec<ListChange<UploadRow>> {
    let mut changes = Vec::new();
    // From the end, so that an index is still right when its turn comes.
    for (index, row) in old.iter().enumerate().rev() {
        if !new.iter().any(|kept| kept.id == row.id) {
            changes.push(ListChange::Removed { index });
        }
    }
    let mut kept: Vec<&UploadRow> = old
        .iter()
        .filter(|row| new.iter().any(|new| new.id == row.id))
        .collect();
    for (index, row) in new.iter().enumerate() {
        match kept.get(index) {
            Some(held) if held.id == row.id => {
                if **held != *row {
                    changes.push(ListChange::Updated {
                        index,
                        item: row.clone(),
                    });
                }
            }
            // What is kept is a part of `new`, in order, so this one is not there yet.
            _ => {
                changes.push(ListChange::Inserted {
                    index,
                    item: row.clone(),
                });
                kept.insert(index, row);
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(id: u64, state: UploadState) -> Upload {
        Upload {
            id: UploadId::for_test(id),
            room: "r".to_owned().into(),
            thread: None,
            name: format!("file{id}.txt"),
            path: format!("/tmp/file{id}.txt").into(),
            size: 2_048,
            state,
        }
    }

    fn in_thread(upload: Upload) -> Upload {
        Upload {
            thread: Some("p".to_owned().into()),
            ..upload
        }
    }

    // ACT-009
    #[test]
    fn a_row_has_the_name_the_size_in_words_and_what_can_be_done() {
        let queued = row(&upload(0, UploadState::Queued));
        assert_eq!(
            queued,
            UploadRow {
                id: UploadId::for_test(0),
                name: "file0.txt".into(),
                size: "2.0 KB".into(),
                status_line: "Waiting \u{b7} 2.0 KB".into(),
                thumbnail: Thumbnail {
                    is_image: false,
                    url: String::new(),
                    label: "TXT".into()
                },
                progress: 0.0,
                status: UploadStatus::Queued,
                error: String::new(),
                can_retry: false,
                can_cancel: true,
            }
        );
        let uploading = row(&upload(1, UploadState::Uploading { sent: 512 }));
        assert_eq!(uploading.status, UploadStatus::Uploading);
        assert!((uploading.progress - 0.25).abs() < f64::EPSILON);
        assert!(!uploading.can_retry && uploading.can_cancel);
        let confirming = row(&upload(2, UploadState::Confirming));
        assert!((confirming.progress - 1.0).abs() < f64::EPSILON);
        assert!(!confirming.can_retry && !confirming.can_cancel);
        let failed = row(&upload(3, UploadState::Failed(UploadError::Connection)));
        assert_eq!(failed.status, UploadStatus::Failed);
        assert_eq!(failed.error, "Can't reach the server");
        assert!(failed.can_retry && failed.can_cancel);
    }

    // ACT-009
    #[test]
    fn the_status_line_says_what_the_row_waits_for() {
        assert_eq!(sent_of(1_887_437, 4_404_019), "1.8 of 4.2 MB");
        assert_eq!(sent_of(0, 900), "0 of 900 B");
        assert_eq!(sent_of(9_000_000, 4_404_019), "4.2 of 4.2 MB");
        assert_eq!(sent_of(512, 2_048), "0.5 of 2.0 KB");
        let line = |state| row(&upload(0, state)).status_line;
        assert_eq!(
            line(UploadState::Uploading { sent: 1_024 }),
            "1.0 of 2.0 KB"
        );
        assert_eq!(line(UploadState::Confirming), "Posting\u{2026}");
        assert_eq!(
            line(UploadState::Failed(UploadError::Disabled)),
            "The server doesn't take file uploads"
        );
    }

    // ACT-009
    #[test]
    fn the_thumbnail_is_the_picture_for_an_image_and_the_extension_for_a_file() {
        let mut picture = upload(0, UploadState::Queued);
        picture.name = "cat.png".into();
        picture.path = "/tmp/cat.png".into();
        let thumbnail = row(&picture).thumbnail;
        assert!(thumbnail.is_image);
        assert_eq!(thumbnail.url, "file:///tmp/cat.png?v=0");
    }

    // ACT-009
    #[test]
    fn the_progress_stays_within_0_and_1() {
        assert!(row(&upload(0, UploadState::Uploading { sent: 9_999 })).progress <= 1.0);
        let empty = Upload {
            size: 0,
            ..upload(0, UploadState::Uploading { sent: 5 })
        };
        assert!(row(&empty).progress.abs() < f64::EPSILON);
    }

    // ACT-009
    #[test]
    fn errors_are_in_words_and_a_limit_is_named() {
        assert_eq!(
            error_in_words(&UploadError::TooLarge { limit: 104_857_600 }),
            "The file is larger than the server's limit of 100 MB"
        );
        assert_eq!(
            error_in_words(&UploadError::TooLarge { limit: 1_572_864 }),
            "The file is larger than the server's limit of 1.5 MB"
        );
        assert_eq!(
            error_in_words(&UploadError::Disabled),
            "The server doesn't take file uploads"
        );
        assert_eq!(
            error_in_words(&UploadError::Unreadable("no such file".into())),
            "Can't read the file: no such file"
        );
        assert_eq!(
            error_in_words(&UploadError::Refused("File type is not accepted".into())),
            "The server refused the file: File type is not accepted"
        );
        assert_eq!(
            error_in_words(&UploadError::Refused(
                "File type is not accepted. [error-invalid-file-type]".into()
            )),
            "The server refused the file: File type is not accepted."
        );
        assert_eq!(
            error_in_words(&UploadError::Refused("[No file uploaded]".into())),
            "The server refused the file: [No file uploaded]"
        );
    }

    // ACT-009
    #[test]
    fn the_room_shows_its_uploads_and_the_thread_its_own() {
        let mut elsewhere = upload(2, UploadState::Queued);
        elsewhere.room = "other".to_owned().into();
        let uploads = [
            upload(0, UploadState::Queued),
            in_thread(upload(1, UploadState::Queued)),
            elsewhere,
        ];
        let room: RoomId = "r".to_owned().into();
        let parent: MessageId = "p".to_owned().into();

        let ids = |rows: Vec<UploadRow>| rows.into_iter().map(|row| row.id).collect::<Vec<_>>();

        assert_eq!(
            ids(rows(&uploads, Place::Room(&room))),
            [UploadId::for_test(0)]
        );
        assert_eq!(
            ids(rows(&uploads, Place::Thread(&parent))),
            [UploadId::for_test(1)]
        );
    }

    fn rows_of(uploads: &[Upload]) -> Vec<UploadRow> {
        uploads.iter().map(row).collect()
    }

    // ACT-009
    #[test]
    fn a_new_upload_is_an_insert_and_a_progress_is_an_update_of_one_row() {
        let mut shown = Vec::new();
        let mut uploads = vec![upload(0, UploadState::Uploading { sent: 0 })];
        let first = changes(&shown, &rows_of(&uploads));
        assert_eq!(
            first,
            [ListChange::Inserted {
                index: 0,
                item: row(&uploads[0])
            }]
        );
        for change in &first {
            change.apply(&mut shown);
        }

        uploads.push(upload(1, UploadState::Queued));
        uploads[0].state = UploadState::Uploading { sent: 1_024 };
        let next = changes(&shown, &rows_of(&uploads));
        assert_eq!(
            next,
            [
                ListChange::Updated {
                    index: 0,
                    item: row(&uploads[0])
                },
                ListChange::Inserted {
                    index: 1,
                    item: row(&uploads[1])
                },
            ]
        );
        for change in &next {
            change.apply(&mut shown);
        }
        assert_eq!(shown, rows_of(&uploads));
        assert!(changes(&shown, &rows_of(&uploads)).is_empty());
    }

    // ACT-009
    #[test]
    fn a_finished_upload_is_a_removal_and_the_rows_keep_their_order() {
        let uploads = vec![
            upload(0, UploadState::Confirming),
            upload(1, UploadState::Queued),
            upload(2, UploadState::Queued),
            upload(3, UploadState::Queued),
        ];
        let mut shown = rows_of(&uploads);
        // 0 and 2 go, 1 starts, 3 stays, and 4 is new.
        let now = vec![
            upload(1, UploadState::Uploading { sent: 0 }),
            upload(3, UploadState::Queued),
            upload(4, UploadState::Queued),
        ];

        let all = changes(&shown, &rows_of(&now));

        assert_eq!(
            &all[..2],
            [
                ListChange::Removed { index: 2 },
                ListChange::Removed { index: 0 }
            ]
        );
        for change in &all {
            change.apply(&mut shown);
        }
        assert_eq!(shown, rows_of(&now));
    }

    // ACT-009
    #[test]
    fn an_upload_inserted_between_the_others_goes_in_its_place() {
        let mut shown = rows_of(&[
            upload(0, UploadState::Queued),
            upload(2, UploadState::Queued),
        ]);
        let now = rows_of(&[
            upload(0, UploadState::Queued),
            upload(1, UploadState::Queued),
            upload(2, UploadState::Queued),
        ]);

        let all = changes(&shown, &now);

        assert_eq!(all.len(), 1);
        for change in &all {
            change.apply(&mut shown);
        }
        assert_eq!(shown, now);
    }
}
