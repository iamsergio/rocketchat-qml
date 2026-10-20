// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Emoji shortcodes: the standard ones (`:smile:`) turn into Unicode, and the server's custom ones
//! into the URL of their image.

use std::collections::HashMap;

use crate::rich::Emoji;

/// The Unicode text of the standard emoji with that shortcode (without colons), if there is one.
#[must_use]
pub fn unicode(short_code: &str) -> Option<&'static str> {
    emojis::get_by_shortcode(short_code).map(emojis::Emoji::as_str)
}

/// The custom emoji of a server, by name and alias.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomEmojis {
    urls: HashMap<String, String>,
}

impl CustomEmojis {
    /// The emoji the server listed, whose images are under `server` (`<server>/emoji-custom/`).
    /// The images are public.
    #[must_use]
    pub fn new(server: &str, list: &[rc_api::CustomEmoji]) -> Self {
        let base = server.trim_end_matches('/');
        let mut urls = HashMap::new();
        for emoji in list {
            let url = format!("{base}/emoji-custom/{}.{}", emoji.name, emoji.extension);
            for name in emoji.aliases.iter().chain([&emoji.name]) {
                urls.insert(name.clone(), url.clone());
            }
        }
        Self { urls }
    }

    /// The URL of the image of the emoji with that name or alias.
    #[must_use]
    pub fn url(&self, name: &str) -> Option<&str> {
        self.urls.get(name).map(String::as_str)
    }
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

    // RICH-003
    #[test]
    fn an_unknown_shortcode_has_no_unicode() {
        assert_eq!(unicode("no_such_emoji"), None);
        assert_eq!(unicode(""), None);
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
}
