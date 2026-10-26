// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the files that wait in a composer for Send (ACT-009), the room's or the thread's:
//! `rc_view::staged_view::Staged` shown as rows. Adding or taking out a file announces that row and
//! no other. A thumbnail picture is read once, when the file is added, and not with each row read.

use std::cell::RefCell;
use std::path::PathBuf;

use rc_view::staged_view::{Staged, StagedFile};
use slint::{Image, Model, ModelNotify, ModelTracker};

use crate::rows::load_image;
use crate::ui::StagedFileRow;

#[derive(Default)]
pub struct StagedModel {
    staged: RefCell<Staged>,
    /// The picture of each file's thumbnail, in the order of the files; none for a file that is
    /// not a picture, or does not load.
    pictures: RefCell<Vec<Option<Image>>>,
    notify: ModelNotify,
}

impl StagedModel {
    /// Adds the files, in order, except the ones that wait already.
    pub fn add(&self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            let added = self.staged.borrow_mut().add([path]);
            for file in added {
                let at = self.pictures.borrow().len();
                let picture = file
                    .thumbnail
                    .is_image
                    .then(|| load_image(&file.thumbnail.url))
                    .flatten();
                self.pictures.borrow_mut().push(picture);
                self.notify.row_added(at, 1);
            }
        }
    }

    /// Takes the file in the row out, if there is one.
    pub fn remove(&self, row: usize) {
        if self.staged.borrow_mut().remove(row).is_some() {
            self.pictures.borrow_mut().remove(row);
            self.notify.row_removed(row, 1);
        }
    }

    /// Takes all the files out.
    pub fn clear(&self) {
        let count = self.pictures.borrow().len();
        if count > 0 {
            self.staged.borrow_mut().take();
            self.pictures.borrow_mut().clear();
            self.notify.row_removed(0, count);
        }
    }

    /// The files, in the order they were attached.
    pub fn files(&self) -> Vec<StagedFile> {
        self.staged.borrow().files().to_vec()
    }
}

impl Model for StagedModel {
    type Data = StagedFileRow;

    fn row_count(&self) -> usize {
        self.pictures.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<StagedFileRow> {
        let staged = self.staged.borrow();
        let file = staged.files().get(row)?;
        let picture = self.pictures.borrow().get(row).cloned().flatten();
        Some(StagedFileRow {
            name: file.name.as_str().into(),
            size: file.size.as_str().into(),
            is_image: file.thumbnail.is_image,
            label: file.thumbnail.label.as_str().into(),
            thumbnail: picture.unwrap_or_default(),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(model: &StagedModel) -> Vec<String> {
        model.iter().map(|row| row.name.to_string()).collect()
    }

    // ACT-009
    #[test]
    fn files_are_added_in_order_without_repeats_and_taken_out_by_row() {
        let model = StagedModel::default();
        model.add([
            PathBuf::from("/nonexistent/a.txt"),
            PathBuf::from("/nonexistent/b.png"),
            PathBuf::from("/nonexistent/a.txt"),
        ]);
        assert_eq!(names(&model), ["a.txt", "b.png"]);
        let second = model.row_data(1).unwrap();
        assert!(second.is_image);
        assert_eq!(model.row_data(0).unwrap().label, "TXT");

        model.remove(0);
        assert_eq!(names(&model), ["b.png"]);
        model.remove(5);
        assert_eq!(model.files().len(), 1);
        model.clear();
        assert_eq!(model.row_count(), 0);
        assert!(model.files().is_empty());
    }
}
