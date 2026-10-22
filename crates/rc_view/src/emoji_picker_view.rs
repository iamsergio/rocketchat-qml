// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `EmojiPickerModel` lists: the emoji of `rc_core::emoji::catalog` that match the
//! query. Plain Rust, so it can be unit-tested without Qt.

use rc_core::emoji::{CustomEmojis, PickerEmoji, catalog, search};

/// The picker's emoji and the part of them that the query lets through.
#[derive(Debug, Default)]
pub struct Picker {
    emoji: CustomEmojis,
    catalog: Vec<PickerEmoji>,
    query: String,
    shown: Vec<PickerEmoji>,
}

impl Picker {
    /// A picker of the standard emoji and `emoji`, with no query.
    #[must_use]
    pub fn with_emoji(emoji: CustomEmojis) -> Self {
        let mut picker = Self {
            catalog: catalog(&emoji),
            emoji,
            query: String::new(),
            shown: Vec::new(),
        };
        picker.filter();
        picker
    }

    /// The custom emoji it lists.
    #[must_use]
    pub fn emoji(&self) -> &CustomEmojis {
        &self.emoji
    }

    /// The query as it was given.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Lists what matches `query` from now on.
    pub fn set_query(&mut self, query: &str) {
        query.clone_into(&mut self.query);
        self.filter();
    }

    /// Takes the server's custom emoji in and keeps the query.
    pub fn set_emoji(&mut self, emoji: &CustomEmojis) {
        self.emoji = emoji.clone();
        self.catalog = catalog(emoji);
        self.filter();
    }

    /// How many emoji match.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shown.len()
    }

    /// Whether no emoji match.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }

    /// The emoji at `row` of the matches.
    #[must_use]
    pub fn get(&self, row: usize) -> Option<&PickerEmoji> {
        self.shown.get(row)
    }

    fn filter(&mut self) {
        self.shown = search(&self.catalog, &self.query)
            .into_iter()
            .cloned()
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom() -> CustomEmojis {
        CustomEmojis::new(
            "https://chat.example.com",
            &[rc_api::CustomEmoji {
                name: "zz_blob".into(),
                aliases: vec!["zz_alias".into()],
                extension: "gif".into(),
            }],
        )
    }

    fn shortcodes(picker: &Picker) -> Vec<&str> {
        (0..picker.len())
            .filter_map(|row| picker.get(row))
            .map(|emoji| emoji.shortcode.as_str())
            .collect()
    }

    // ACT-003
    #[test]
    fn without_a_query_the_picker_lists_everything_custom_emoji_first() {
        let picker = Picker::with_emoji(custom());
        assert_eq!(picker.query(), "");
        assert!(picker.len() > 1000);
        let first = picker.get(0).unwrap();
        assert_eq!(first.shortcode, ":zz_blob:");
        assert_eq!(first.glyph, "");
        assert_eq!(
            first.image,
            "https://chat.example.com/emoji-custom/zz_blob.gif"
        );
        let second = picker.get(1).unwrap();
        assert!(!second.glyph.is_empty());
        assert_eq!(second.image, "");
        assert!(picker.get(picker.len()).is_none());
    }

    // ACT-003
    #[test]
    fn the_query_filters_by_any_part_of_a_shortcode_and_an_alias() {
        let mut picker = Picker::with_emoji(custom());
        picker.set_query("ZZ_ALIAS");
        assert_eq!(shortcodes(&picker), [":zz_blob:"]);
        picker.set_query("rocket");
        assert!(shortcodes(&picker).contains(&":rocket:"));
        picker.set_query("no_such_emoji_at_all");
        assert_eq!(picker.len(), 0);
    }

    // ACT-003
    #[test]
    fn clearing_the_query_brings_everything_back() {
        let mut picker = Picker::with_emoji(CustomEmojis::default());
        let all = picker.len();
        picker.set_query("smile");
        assert!(picker.len() < all);
        picker.set_query("");
        assert_eq!(picker.len(), all);
    }

    // ACT-003
    #[test]
    fn new_custom_emoji_join_the_list_and_the_query_stays() {
        let mut picker = Picker::with_emoji(CustomEmojis::default());
        picker.set_query("zz_alias");
        assert_eq!(picker.len(), 0);

        picker.set_emoji(&custom());
        assert_eq!(shortcodes(&picker), [":zz_blob:"]);
        assert_eq!(picker.query(), "zz_alias");

        picker.set_emoji(&CustomEmojis::default());
        assert_eq!(picker.len(), 0);
    }
}
