// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Emoji shortcodes: the standard ones (`:smile:`) turn into Unicode, and the server's custom ones
//! into the URL of their image. Also the catalog an emoji picker lists and searches.
//!
//! Two lists of standard emoji meet here. The `emojis` crate knows gemoji's names, which are what
//! messages typed in other clients use. Rocket.Chat itself names and validates reactions with
//! emojione 4.5.0 (`data/emojione.tsv`, made by `scripts/update_emojione.py`), which refuses about
//! a third of gemoji's names and calls the others differently, so reactions are made with its
//! names.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::rich::Emoji;

const EMOJIONE: &str = include_str!("../data/emojione.tsv");

/// An emoji of emojione.
struct Standard {
    /// The canonical shortcode with its colons.
    name: &'static str,
    /// The Unicode text.
    glyph: &'static str,
    /// Other shortcodes with their colons.
    alternates: Vec<&'static str>,
    /// A base form: not a skin tone, and not one of the modifiers or regional indicators.
    base: bool,
}

/// The emojione table, read once.
struct Table {
    /// By canonical name and alternate, with colons.
    by_name: HashMap<&'static str, usize>,
    /// The base forms by glyph without U+FE0F.
    by_glyph: HashMap<String, usize>,
    all: Vec<Standard>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    let mut table = Table {
        by_name: HashMap::new(),
        by_glyph: HashMap::new(),
        all: Vec::new(),
    };
    for line in EMOJIONE.lines() {
        let mut columns = line.split('\t');
        let (Some(name), Some(glyph), Some(alternates), Some(category)) = (
            columns.next(),
            columns.next(),
            columns.next(),
            columns.next(),
        ) else {
            continue;
        };
        let base = !is_skin_tone(name) && !matches!(category, "modifier" | "regional");
        let index = table.all.len();
        table.by_name.insert(name, index);
        let alternates: Vec<&str> = alternates.split(',').filter(|a| !a.is_empty()).collect();
        for alternate in &alternates {
            table.by_name.entry(alternate).or_insert(index);
        }
        if base {
            table.by_glyph.insert(plain(glyph), index);
        }
        table.all.push(Standard {
            name,
            glyph,
            alternates,
            base,
        });
    }
    table
});

/// Whether the canonical name is that of a variant with a skin tone (`:thumbsup_tone2:`).
fn is_skin_tone(name: &str) -> bool {
    name.strip_suffix(':')
        .and_then(|name| name.rsplit_once("_tone"))
        .is_some_and(|(_, tone)| matches!(tone, "1" | "2" | "3" | "4" | "5"))
}

/// The text without U+FE0F (the request for the colourful form), which the lists disagree on.
fn plain(glyph: &str) -> String {
    glyph.chars().filter(|&c| c != '\u{FE0F}').collect()
}

/// The Unicode text of the standard emoji with that shortcode (without colons), if there is one:
/// gemoji's names first, then emojione's, canonical or alternate, skin tones included.
#[must_use]
pub fn unicode(short_code: &str) -> Option<&'static str> {
    emojis::get_by_shortcode(short_code)
        .map(emojis::Emoji::as_str)
        .or_else(|| {
            let table = &*TABLE;
            let at = table.by_name.get(format!(":{short_code}:").as_str())?;
            Some(table.all[*at].glyph)
        })
}

/// A shortcode as a reaction names it, between colons: `smile` and `:smile:` both give `:smile:`.
/// Empty if there is no name.
#[must_use]
pub fn shortcode(text: &str) -> String {
    let name = text.trim().trim_matches(':').trim();
    if name.is_empty() {
        String::new()
    } else {
        format!(":{name}:")
    }
}

/// The custom emoji of a server, by name and alias.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomEmojis {
    urls: HashMap<String, String>,
    /// The name and extension of the image of each emoji, by name and alias.
    files: HashMap<String, (String, String)>,
    /// One per emoji, by name, for the picker.
    listed: Vec<Listed>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Listed {
    name: String,
    aliases: Vec<String>,
    url: String,
}

impl CustomEmojis {
    /// The emoji the server listed, whose images are under `server` (`<server>/emoji-custom/`).
    /// The images are public.
    #[must_use]
    pub fn new(server: &str, list: &[rc_api::CustomEmoji]) -> Self {
        let base = server.trim_end_matches('/');
        let mut urls = HashMap::new();
        let mut files = HashMap::new();
        let mut listed = Vec::new();
        for emoji in list {
            let url = format!("{base}/emoji-custom/{}.{}", emoji.name, emoji.extension);
            for name in emoji.aliases.iter().chain([&emoji.name]) {
                urls.insert(name.clone(), url.clone());
                files.insert(name.clone(), (emoji.name.clone(), emoji.extension.clone()));
            }
            listed.push(Listed {
                name: emoji.name.clone(),
                aliases: emoji.aliases.clone(),
                url,
            });
        }
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            urls,
            files,
            listed,
        }
    }

    /// The name (not an alias) and the extension of the image file of the emoji with that name or
    /// alias: what the image is downloaded and cached by.
    pub(crate) fn file(&self, name: &str) -> Option<(&str, &str)> {
        self.files
            .get(name)
            .map(|(name, extension)| (name.as_str(), extension.as_str()))
    }

    /// The URL of the image of the emoji with that name or alias.
    #[must_use]
    pub fn url(&self, name: &str) -> Option<&str> {
        self.urls.get(name).map(String::as_str)
    }
}

/// One emoji of the picker: a custom one of the server's, or a standard one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerEmoji {
    /// What a reaction is made of: the name between colons (`:smile:`).
    pub shortcode: String,
    /// The Unicode text of a standard emoji. Empty for a custom one.
    pub glyph: String,
    /// The URL of the image of a custom emoji. Empty for a standard one.
    pub image: String,
    /// Every shortcode it can be found by, without colons, the one in `shortcode` included.
    names: Vec<String>,
}

impl PickerEmoji {
    fn matches(&self, query: &str) -> bool {
        self.names
            .iter()
            .any(|name| name.to_lowercase().contains(query))
    }
}

/// Every emoji a user can pick: the server's custom ones first, by name, then the standard ones in
/// the order of the `emojis` crate. Only those that emojione has too are there, under its
/// canonical name, since that is what the server accepts and what other clients send; they are
/// found by its alternate names and by gemoji's. Skin tones are left out, and so are the
/// variants of the `emojis` crate with one.
#[must_use]
pub fn catalog(custom: &CustomEmojis) -> Vec<PickerEmoji> {
    let custom_ones = custom.listed.iter().map(|emoji| PickerEmoji {
        shortcode: format!(":{}:", emoji.name),
        glyph: String::new(),
        image: emoji.url.clone(),
        names: emoji.aliases.iter().chain([&emoji.name]).cloned().collect(),
    });
    let table = &*TABLE;
    let standard_ones = emojis::iter()
        .filter(|emoji| matches!(emoji.skin_tone(), None | Some(emojis::SkinTone::Default)))
        .filter_map(|emoji| {
            let standard = &table.all[*table.by_glyph.get(&plain(emoji.as_str()))?];
            debug_assert!(standard.base);
            let mut names: Vec<String> = [standard.name]
                .into_iter()
                .chain(standard.alternates.iter().copied())
                .map(|name| name.trim_matches(':').to_owned())
                .collect();
            names.extend(emoji.shortcodes().map(str::to_owned));
            Some(PickerEmoji {
                shortcode: standard.name.to_owned(),
                glyph: emoji.as_str().to_owned(),
                image: String::new(),
                names,
            })
        });
    custom_ones.chain(standard_ones).collect()
}

/// The emoji of `catalog` that have a shortcode containing `query`, ignoring case, in the order of
/// the catalog. A blank query finds everything.
#[must_use]
pub fn search<'a>(catalog: &'a [PickerEmoji], query: &str) -> Vec<&'a PickerEmoji> {
    let query = query.trim().trim_matches(':').to_lowercase();
    catalog
        .iter()
        .filter(|emoji| query.is_empty() || emoji.matches(&query))
        .collect()
}

/// What to show for an [`Emoji`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown<'a> {
    Unicode(&'a str),
    /// An image of the server's.
    Image {
        name: &'a str,
        url: &'a str,
    },
    /// A shortcode nobody knows: show it as typed, between colons.
    Unknown(&'a str),
}

impl Emoji {
    /// What to show for the emoji. A custom emoji wins over a standard one of the same name, as
    /// on the server.
    #[must_use]
    pub fn shown<'a>(&'a self, custom: &'a CustomEmojis) -> Shown<'a> {
        let Some(name) = self.short_code.as_deref() else {
            return Shown::Unicode(self.unicode.as_deref().unwrap_or_default());
        };
        if let Some(url) = custom.url(name) {
            return Shown::Image { name, url };
        }
        if let Some(text) = self.unicode.as_deref() {
            return Shown::Unicode(text);
        }
        unicode(name).map_or(Shown::Unknown(name), Shown::Unicode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(name: &str) -> Emoji {
        Emoji {
            short_code: Some(name.to_owned()),
            unicode: None,
        }
    }

    fn custom() -> CustomEmojis {
        CustomEmojis::new(
            "https://chat.example.com/",
            &[
                rc_api::CustomEmoji {
                    name: "party".into(),
                    aliases: vec!["celebrate".into()],
                    extension: "gif".into(),
                },
                rc_api::CustomEmoji {
                    name: "smile".into(),
                    aliases: Vec::new(),
                    extension: "png".into(),
                },
            ],
        )
    }

    // RICH-003
    #[test]
    fn standard_shortcodes_turn_into_unicode() {
        assert_eq!(unicode("smile"), Some("😄"));
        assert_eq!(unicode("rocket"), Some("🚀"));
        assert_eq!(unicode("+1"), Some("👍"));
        assert_eq!(unicode("thumbsup"), Some("👍"));
    }

    // ACT-003
    #[test]
    fn a_shortcode_is_normalised_to_a_name_between_colons() {
        assert_eq!(shortcode("smile"), ":smile:");
        assert_eq!(shortcode(":smile:"), ":smile:");
        assert_eq!(shortcode(" :smile "), ":smile:");
        assert_eq!(shortcode("::"), "");
        assert_eq!(shortcode(" "), "");
    }

    // RICH-003
    #[test]
    fn an_unknown_shortcode_has_no_unicode() {
        assert_eq!(unicode("no_such_emoji"), None);
        assert_eq!(unicode(""), None);
    }

    // ACT-003
    #[test]
    fn emojione_names_that_gemoji_lacks_have_unicode_too() {
        // A canonical name that gemoji spells otherwise, an alternate, and a skin tone.
        assert_eq!(unicode("hugging"), Some("🤗"));
        assert_eq!(unicode("thumbup"), Some("👍"));
        assert_eq!(unicode("thumbsup_tone2"), Some("👍🏼"));
        assert_eq!(unicode("not_an_emoji_at_all"), None);
    }

    // ACT-003
    #[test]
    fn gemoji_wins_over_emojione_when_both_know_the_name() {
        assert_eq!(
            unicode("smile"),
            emojis::get_by_shortcode("smile").map(emojis::Emoji::as_str)
        );
    }

    // RICH-003
    #[test]
    fn a_custom_emoji_is_found_by_name_and_by_alias_with_the_url_of_its_image() {
        let emoji = custom();
        let url = Some("https://chat.example.com/emoji-custom/party.gif");
        assert_eq!(emoji.url("party"), url);
        assert_eq!(emoji.url("celebrate"), url);
        assert_eq!(emoji.url("other"), None);
    }

    // RICH-003
    #[test]
    fn a_shortcode_shows_as_unicode_an_image_or_itself() {
        let none = CustomEmojis::default();
        assert_eq!(code("rocket").shown(&none), Shown::Unicode("🚀"));
        assert_eq!(
            code("party").shown(&custom()),
            Shown::Image {
                name: "party",
                url: "https://chat.example.com/emoji-custom/party.gif"
            }
        );
        assert_eq!(code("party").shown(&none), Shown::Unknown("party"));
    }

    // RICH-003
    #[test]
    fn a_custom_emoji_wins_over_a_standard_one_of_the_same_name() {
        assert!(matches!(
            code("smile").shown(&custom()),
            Shown::Image { .. }
        ));
    }

    // RICH-003
    #[test]
    fn an_emoji_typed_as_a_character_shows_as_it_is() {
        let emoji = Emoji {
            short_code: None,
            unicode: Some("🎉".into()),
        };
        assert_eq!(emoji.shown(&custom()), Shown::Unicode("🎉"));
    }

    // RICH-003
    #[test]
    fn an_emoji_with_both_shows_the_character() {
        let emoji = Emoji {
            short_code: Some("tada".into()),
            unicode: Some("🎉".into()),
        };
        assert_eq!(emoji.shown(&CustomEmojis::default()), Shown::Unicode("🎉"));
    }

    fn catalog_with_custom() -> Vec<PickerEmoji> {
        catalog(&custom())
    }

    // ACT-003
    #[test]
    fn the_catalog_lists_custom_emoji_by_name_first_then_the_standard_ones() {
        let all = catalog_with_custom();
        let shortcodes: Vec<&str> = all.iter().take(3).map(|e| e.shortcode.as_str()).collect();
        assert_eq!(shortcodes, [":party:", ":smile:", ":grinning:"]);
        assert_eq!(
            all[0].image,
            "https://chat.example.com/emoji-custom/party.gif"
        );
        assert_eq!(all[0].glyph, "");
        assert_eq!(all[2].glyph, "😀");
        assert_eq!(all[2].image, "");
    }

    // ACT-003
    #[test]
    fn the_catalog_has_standard_base_forms_with_shortcodes_only() {
        let all = catalog(&CustomEmojis::default());
        assert!(all.len() > 1000);
        assert!(all.iter().all(|e| e.shortcode.starts_with(':')
            && e.shortcode.ends_with(':')
            && !e.glyph.is_empty()));
        let glyphs = |glyph: &str| all.iter().filter(|e| e.glyph == glyph).count();
        assert_eq!(glyphs("👍"), 1);
        assert_eq!(glyphs("👍🏽"), 0);
        assert!(all.iter().all(|e| !is_skin_tone(&e.shortcode)));
        let mut unique: Vec<&str> = all.iter().map(|e| e.shortcode.as_str()).collect();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "a shortcode is there twice");
        assert!(!all.iter().any(|e| e.shortcode.contains("regional")));
        let shortcodes: Vec<&str> = all.iter().map(|e| e.shortcode.as_str()).collect();
        assert!(shortcodes.contains(&":smile:"));
    }

    // ACT-003
    #[test]
    fn a_search_matches_any_part_of_a_shortcode_ignoring_case() {
        let all = catalog_with_custom();
        let found = |query: &str| -> Vec<String> {
            search(&all, query)
                .into_iter()
                .map(|e| e.shortcode.clone())
                .collect()
        };
        assert_eq!(found("PARTY")[0], ":party:");
        assert!(found("mile").contains(&":smile:".to_owned()));
        assert!(found("mile").contains(&":smiley:".to_owned()));
        assert_eq!(found("no_such_emoji_at_all"), Vec::<String>::new());
    }

    // ACT-003
    #[test]
    fn a_search_finds_an_emoji_by_its_aliases_and_a_blank_one_finds_everything() {
        let all = catalog_with_custom();
        let found = search(&all, "celebr");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].shortcode, ":party:");
        let laughing = search(&all, "satisfied");
        assert_eq!(laughing.len(), 1);
        assert_eq!(laughing[0].shortcode, ":laughing:");
        assert_eq!(search(&all, "").len(), all.len());
        assert_eq!(search(&all, "  ").len(), all.len());
        assert_eq!(search(&all, ":party:")[0].shortcode, ":party:");
    }

    // ACT-003
    #[test]
    fn the_picker_uses_the_names_of_the_server_and_finds_them_by_the_other_spellings() {
        let all = catalog(&CustomEmojis::default());
        let first = |query: &str| search(&all, query)[0].shortcode.clone();
        // Rocket.Chat calls a thumbs-up `:thumbsup:`; gemoji's `+1` is how it is found too.
        assert_eq!(first("thumbsup"), ":thumbsup:");
        assert_eq!(first("+1"), ":thumbsup:");
        // A name only emojione has, found by gemoji's too.
        assert_eq!(first("hugging"), ":hugging:");
        assert_eq!(first("hugs"), ":hugging:");
        assert!(!all.iter().any(|e| e.shortcode == ":hugs:"));
    }
}
