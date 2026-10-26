// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the files being uploaded into the open room, or into the open thread (ACT-009), one
//! row each, in the order they were asked for. `show` brings it to what the cache says with
//! `upload_view::changes`, announced row by row through `row_ops`, so a progress repaints one row.
//! A thumbnail picture is read when its row is made and kept while the row stays.

use std::cell::RefCell;

use rc_core::uploads::UploadId;
use rc_view::row_ops::{RowOp, row_ops};
use rc_view::upload_view::{UploadRow, changes};
use slint::{Image, Model, ModelNotify, ModelTracker};

use crate::rows::load_image;
use crate::ui::UploadFileRow;

#[derive(Default)]
pub struct UploadModel {
    rows: RefCell<Vec<UploadRow>>,
    /// The picture of each row's thumbnail, in the order of the rows.
    pictures: RefCell<Vec<Option<Image>>>,
    notify: ModelNotify,
}

fn picture(row: &UploadRow) -> Option<Image> {
    row.thumbnail
        .is_image
        .then(|| load_image(&row.thumbnail.url))
        .flatten()
}

impl UploadModel {
    /// Brings the rows to `new`, announcing only what differs.
    pub fn show(&self, new: &[UploadRow]) {
        let all = changes(&self.rows.borrow(), new);
        for change in &all {
            match row_ops(change) {
                RowOp::Insert { row } => {
                    change.apply(&mut self.rows.borrow_mut());
                    let made = self.rows.borrow().get(row).and_then(picture);
                    self.pictures.borrow_mut().insert(row, made);
                    self.notify.row_added(row, 1);
                }
                RowOp::Update { row } => {
                    change.apply(&mut self.rows.borrow_mut());
                    self.notify.row_changed(row);
                }
                RowOp::Remove { row } => {
                    change.apply(&mut self.rows.borrow_mut());
                    self.pictures.borrow_mut().remove(row);
                    self.notify.row_removed(row, 1);
                }
                // `changes` makes none of these, but the model must stay right if it ever does.
                RowOp::Reset | RowOp::Move { .. } => {
                    change.apply(&mut self.rows.borrow_mut());
                    *self.pictures.borrow_mut() = self.rows.borrow().iter().map(picture).collect();
                    self.notify.reset();
                }
            }
        }
    }

    /// The upload in the row, if there is one.
    pub fn id_at(&self, row: usize) -> Option<UploadId> {
        self.rows.borrow().get(row).map(|row| row.id)
    }
}

impl Model for UploadModel {
    type Data = UploadFileRow;

    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<UploadFileRow> {
        let rows = self.rows.borrow();
        let held = rows.get(row)?;
        let thumbnail = self.pictures.borrow().get(row).cloned().flatten();
        Some(UploadFileRow {
            name: held.name.as_str().into(),
            status_line: held.status_line.as_str().into(),
            status: held.status.name().into(),
            #[allow(clippy::cast_possible_truncation)] // A fraction of a bar.
            progress: held.progress as f32,
            can_retry: held.can_retry,
            can_cancel: held.can_cancel,
            is_image: held.thumbnail.is_image,
            label: held.thumbnail.label.as_str().into(),
            thumbnail: thumbnail.unwrap_or_default(),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::uploads::{Upload, UploadState};
    use rc_view::upload_view::{Place, rows};

    use super::*;

    fn upload(id: u64, sent: u64) -> Upload {
        Upload {
            id: UploadId::for_test(id),
            room: "r".to_owned().into(),
            thread: None,
            name: format!("file{id}.txt"),
            path: format!("/nonexistent/file{id}.txt").into(),
            size: 2_048,
            state: UploadState::Uploading { sent },
        }
    }

    fn room(uploads: &[Upload]) -> Vec<UploadRow> {
        rows(uploads, Place::Room(&"r".to_owned().into()))
    }

    // ACT-009
    #[test]
    fn rows_come_and_go_by_upload_and_a_progress_changes_its_row() {
        let model = UploadModel::default();
        model.show(&room(&[upload(0, 0), upload(1, 0)]));
        assert_eq!(model.row_count(), 2);
        assert_eq!(model.id_at(1), Some(UploadId::for_test(1)));

        model.show(&room(&[upload(0, 1_024), upload(1, 0)]));
        let first = model.row_data(0).unwrap();
        assert_eq!(first.status_line, "1.0 of 2.0 KB");
        assert!((first.progress - 0.5).abs() < f32::EPSILON);

        model.show(&room(&[upload(1, 0)]));
        assert_eq!(model.row_count(), 1);
        assert_eq!(model.row_data(0).unwrap().name, "file1.txt");
        assert_eq!(model.id_at(1), None);
    }
}
