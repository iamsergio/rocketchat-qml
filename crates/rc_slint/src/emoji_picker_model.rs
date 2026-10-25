// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the emoji picker: the emoji that match the query, in rows of as many cells as the
//! picker holds. Slint has no grid view and the catalogue has about 1900 emoji, so the view is a
//! lazy list of these rows, and a row's cells are made when the view asks for the row. Nothing is
//! kept per emoji besides the `rc_view` picker's own lists.
//!
//! A new query or a change of the custom emoji is a reset; a custom emoji's file arriving changes
//! the rows that show it, which are at the start since custom emoji come first.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use rc_core::emoji::CustomEmojis;
use rc_view::emoji_image_view::EmojiImageUrls;
use rc_view::emoji_picker_view::Picker;
use slint::{Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use crate::rows::load_image;
use crate::ui::EmojiCell;

pub struct EmojiPickerModel {
    picker: RefCell<Picker>,
    images: RefCell<EmojiImageUrls>,
    columns: Cell<usize>,
    notify: ModelNotify,
}

impl EmojiPickerModel {
    /// A model of the standard emoji, in rows of `columns` cells (at least one).
    pub fn new(columns: usize) -> Self {
        Self {
            picker: RefCell::new(Picker::with_emoji(CustomEmojis::default())),
            images: RefCell::new(EmojiImageUrls::default()),
            columns: Cell::new(columns.max(1)),
            notify: ModelNotify::default(),
        }
    }

    /// Lists what matches `query` from now on.
    pub fn set_query(&self, query: &str) {
        if self.picker.borrow().query() != query {
            self.picker.borrow_mut().set_query(query);
            self.notify.reset();
        }
    }

    /// Takes the server's custom emoji in, with the image files known so far, and keeps the query.
    pub fn show(&self, emoji: &CustomEmojis, images: EmojiImageUrls) {
        self.picker.borrow_mut().set_emoji(emoji);
        *self.images.borrow_mut() = images;
        self.notify.reset();
    }

    /// The names of all the custom emoji, for the core to download.
    pub fn custom_names(&self) -> Vec<String> {
        self.picker.borrow().custom_names()
    }

    /// Takes new custom emoji image files in, and tells the view which rows show them.
    pub fn set_emoji_images(&self, changes: &[(String, String)]) {
        self.images.borrow_mut().set(changes);
        for row in self.rows_showing(changes) {
            self.notify.row_changed(row);
        }
    }

    /// The rows that hold one of the custom emoji of `updates`, in order.
    fn rows_showing(&self, updates: &[(String, String)]) -> Vec<usize> {
        let named: HashSet<&str> = updates.iter().map(|(name, _)| name.as_str()).collect();
        let columns = self.columns.get();
        let picker = self.picker.borrow();
        let mut rows: Vec<usize> = Vec::new();
        // The custom emoji are the first of the matches.
        for at in 0.. {
            let Some(emoji) = picker.get(at) else { break };
            if emoji.image.is_empty() {
                break;
            }
            if named.contains(emoji.shortcode.trim_matches(':'))
                && rows.last() != Some(&(at / columns))
            {
                rows.push(at / columns);
            }
        }
        rows
    }

    fn cell(&self, at: usize) -> Option<EmojiCell> {
        let picker = self.picker.borrow();
        let emoji = picker.get(at)?;
        let name = emoji.shortcode.trim_matches(':');
        let image = (!emoji.image.is_empty())
            .then(|| load_image(self.images.borrow().url(name)))
            .flatten();
        Some(EmojiCell {
            name: name.into(),
            emoji: emoji.shortcode.as_str().into(),
            glyph: emoji.glyph.as_str().into(),
            has_image: image.is_some(),
            image: image.unwrap_or_default(),
        })
    }
}

impl Model for EmojiPickerModel {
    type Data = ModelRc<EmojiCell>;

    fn row_count(&self) -> usize {
        self.picker.borrow().len().div_ceil(self.columns.get())
    }

    fn row_data(&self, row: usize) -> Option<Self::Data> {
        let first = row * self.columns.get();
        let cells: Vec<EmojiCell> = (first..first + self.columns.get())
            .map_while(|at| self.cell(at))
            .collect();
        (!cells.is_empty()).then(|| ModelRc::new(VecModel::from(cells)))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn custom() -> CustomEmojis {
        CustomEmojis::new(
            "https://chat.example.com",
            &["aa_party", "bb_wave"].map(|name| rc_api::CustomEmoji {
                name: name.into(),
                aliases: Vec::new(),
                extension: "svg".into(),
            }),
        )
    }

    fn names(model: &EmojiPickerModel, row: usize) -> Vec<String> {
        let cells = model.row_data(row).unwrap();
        cells.iter().map(|cell| cell.name.to_string()).collect()
    }

    // ACT-003
    #[test]
    fn the_emoji_are_in_rows_of_as_many_cells_as_there_are_columns() {
        let model = EmojiPickerModel::new(8);
        let total = model.picker.borrow().len();
        assert_eq!(model.row_count(), total.div_ceil(8));
        assert_eq!(names(&model, 0).len(), 8);
        // The last row holds what is left, and there is none after it.
        assert_eq!(
            names(&model, model.row_count() - 1).len(),
            total - (total / 8) * 8
        );
        assert!(model.row_data(model.row_count()).is_none());
    }

    // ACT-003
    #[test]
    fn the_query_filters_the_rows_and_clearing_it_brings_them_back() {
        let model = EmojiPickerModel::new(8);
        let all = model.row_count();
        model.set_query("rocket");
        let rows = model.row_count();
        assert!((1..all).contains(&rows));
        assert!(names(&model, 0).contains(&"rocket".to_owned()));
        model.set_query("no_such_emoji_at_all");
        assert_eq!(model.row_count(), 0);
        model.set_query("");
        assert_eq!(model.row_count(), all);
    }

    // ACT-003
    #[test]
    fn custom_emoji_come_first_and_all_are_named_whatever_the_query() {
        let model = EmojiPickerModel::new(8);
        model.show(&custom(), EmojiImageUrls::default());
        assert_eq!(&names(&model, 0)[..2], ["aa_party", "bb_wave"]);
        model.set_query("bb_wave");
        assert_eq!(names(&model, 0), ["bb_wave"]);
        assert_eq!(model.custom_names(), ["aa_party", "bb_wave"]);
    }

    // ACT-003
    #[test]
    fn a_new_emoji_file_changes_the_rows_that_show_it_and_no_others() {
        let dir = std::env::temp_dir().join(format!("rc_slint_picker_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bb_wave.svg");
        std::fs::write(
            &file,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#,
        )
        .unwrap();
        // One cell per row, so that the two custom emoji are in rows of their own.
        let model = Rc::new(EmojiPickerModel::new(1));
        model.show(&custom(), EmojiImageUrls::default());
        let change = |name: &str| vec![(name.to_owned(), String::new())];
        assert_eq!(model.rows_showing(&change("bb_wave")), [1]);
        assert_eq!(model.rows_showing(&change("aa_party")), [0]);
        assert!(model.rows_showing(&change("rocket")).is_empty());
        let url = format!("file://{}?v=1", file.display());
        let cell = |row: usize| model.row_data(row).unwrap().row_data(0).unwrap();
        assert!(!cell(1).has_image);

        model.set_emoji_images(&[("bb_wave".to_owned(), url)]);

        assert!(cell(1).has_image);
        assert!(!cell(0).has_image);
        assert_eq!(cell(1).emoji, ":bb_wave:");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
