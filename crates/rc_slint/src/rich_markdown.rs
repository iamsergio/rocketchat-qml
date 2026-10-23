// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Renders a `RichText` as the markdown subset that Slint's `StyledText` takes: the twin of
//! `rc_view::rich_html`, which does the same for Qt. What the subset lacks is written with what it
//! has (SLINT.md): a heading is a bold line, a code block one inline-code line per line, a quote its
//! lines behind a bar, a custom emoji its `:shortcode:`.
//!
//! Slint rejects the whole text for a heading, a code block, a block quote, an image or an HTML
//! tag other than `<font color>` and `<u>`, so everything the author typed goes through
//! `push_text`, which puts a `\` before each ASCII punctuation character; the only markup is what
//! this module writes. Paragraphs are separated by a blank line, but the items of a list by a
//! single newline: Slint turns a loose list's paragraph into an empty line of its own.
//!
//! Slint makes a paragraph of each hard line break, and a span that crosses one breaks its
//! ranges, so every wrapper (`wrap`) closes and opens again at each break. Emphasis needs its
//! delimiters next to text, so whitespace at the edge of a span is moved outside it.
//!
//! The logic that isn't about the markup (which URLs are links, the text of a timestamp, who a
//! mention is for) is `rc_view::rich_view`'s, so that both frontends agree on it.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use rc_core::emoji::Shown;
use rc_core::rich::{Block, Emoji, Inline, RichText};
use rc_view::rich_view::{color_hex, is_safe_url, mention_name, task_marker, time_text};
use rc_view::viewer::{Clock, Viewer};
use slint::{Color, StyledText};

use crate::ui::Theme;

/// A hard line break: Slint starts a new paragraph.
const BREAK: &str = "\\\n";
/// Before each line of a quote.
const QUOTE_BAR: &str = "\u{258e}";
/// Before each item of a list inside a quote.
const QUOTED_BULLET: &str = "\u{2022} ";

/// The colours the renderer writes: the rest of the text takes the element's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// `@someone`.
    pub mention: Color,
    /// `@all`, `@here` and a mention of the viewer.
    pub mention_me: Color,
    /// `#channel`.
    pub channel: Color,
    /// The bar before each line of a quote.
    pub quote_bar: Color,
}

impl Palette {
    /// The colours of the Qt app for the same things (`richTextStyle` in `Theme.qml`).
    #[must_use]
    pub fn from_theme(theme: &Theme<'_>) -> Self {
        Self {
            mention: theme.get_own(),
            mention_me: theme.get_signal(),
            channel: theme.get_own(),
            quote_bar: theme.get_raised_hover(),
        }
    }
}

/// The text as Slint's markdown subset.
#[must_use]
pub fn render(text: &RichText, viewer: &Viewer, clock: Clock, palette: &Palette) -> String {
    let renderer = Renderer {
        viewer,
        clock,
        palette,
    };
    let mut paragraphs = Vec::new();
    for block in &text.blocks {
        renderer.block(&mut paragraphs, block, false);
    }
    paragraphs.join("\n\n")
}

/// The text ready for a `StyledText` element. If Slint refuses the markdown, which would be a bug
/// of the renderer, the plain text.
#[must_use]
pub fn styled(text: &RichText, viewer: &Viewer, clock: Clock, palette: &Palette) -> StyledText {
    let markdown = render(text, viewer, clock, palette);
    StyledText::from_markdown(&markdown).unwrap_or_else(|error| {
        // Once: the same bug would repeat for every text.
        static WARNED: AtomicBool = AtomicBool::new(false);
        if !WARNED.swap(true, Ordering::Relaxed) {
            tracing::warn!(%error, %markdown, "Slint refused the markdown of a text: showing it plain");
        }
        StyledText::from_plain_text(&text.text)
    })
}

/// A room's topic, which is markdown. Nothing in it depends on the time.
#[must_use]
pub fn topic(topic: &str, viewer: &Viewer, palette: &Palette) -> StyledText {
    styled(
        &RichText::from_topic(topic),
        viewer,
        Clock::timeless(),
        palette,
    )
}

/// Whether the text is only emoji, which is shown larger.
#[must_use]
pub fn is_big_emoji(text: &RichText) -> bool {
    matches!(text.blocks.as_slice(), [Block::BigEmoji(_)])
}

/// `text` with a `\` before each ASCII punctuation character, so that none means markdown. A
/// newline is a hard break.
fn push_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '\n' => out.push_str(BREAK),
            '\r' => {}
            c if c.is_ascii_punctuation() => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
}

fn escaped(text: &str) -> String {
    let mut out = String::new();
    push_text(&mut out, text);
    out
}

/// A paragraph: its lines without the whitespace around them, and none empty. `None` for nothing
/// to show. Markdown would take leading whitespace (four spaces) for a code block.
fn finish(content: &str) -> Option<String> {
    let lines: Vec<&str> = content
        .split(BREAK)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join(BREAK))
}

/// `inner` between `open` and `close`, once for each of its lines. The whitespace at the edges of
/// a line stays outside, where it can't stop the delimiters from working.
fn wrap(inner: &str, open: &str, close: &str) -> String {
    inner
        .split(BREAK)
        .map(|line| {
            let core = line.trim();
            if core.is_empty() {
                return line.to_owned();
            }
            let lead = &line[..line.len() - line.trim_start().len()];
            let trail = &line[line.trim_end().len()..];
            format!("{lead}{open}{core}{close}{trail}")
        })
        .collect::<Vec<_>>()
        .join(BREAK)
}

fn rgb(color: Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        color.red(),
        color.green(),
        color.blue()
    )
}

fn colored(inner: &str, color: Color) -> String {
    wrap(
        inner,
        &format!("<font color=\"{}\">", rgb(color)),
        "</font>",
    )
}

/// Inline code: a fence longer than any run of backticks in the text, and a space inside each end
/// (markdown takes one off both). Nothing for text of only whitespace.
fn code_span(text: &str) -> String {
    let text = text.replace(['\n', '\r'], " ");
    if text.trim().is_empty() {
        return String::new();
    }
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest + 1);
    format!("{fence} {text} {fence}")
}

/// A URL as a link destination: what could end it or mean something is escaped, whitespace is
/// percent-encoded.
fn destination(url: &str) -> String {
    let mut out = String::new();
    for c in url.trim().chars() {
        match c {
            '\\' | '(' | ')' | '<' | '>' | '&' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_whitespace() || c.is_control() => {
                let mut bytes = [0; 4];
                for byte in c.encode_utf8(&mut bytes).bytes() {
                    let _ = write!(out, "%{byte:02X}");
                }
            }
            c => out.push(c),
        }
    }
    out
}

struct Renderer<'a> {
    viewer: &'a Viewer,
    clock: Clock<'a>,
    palette: &'a Palette,
}

impl Renderer<'_> {
    /// Adds the paragraphs of the block. A list is one, with an item a line, unless it is `quoted`:
    /// then it is a paragraph per item, with the marker written as text, to have a bar before it.
    fn block(&self, out: &mut Vec<String>, block: &Block, quoted: bool) {
        match block {
            Block::Paragraph(content) => out.extend(finish(&self.inlines(content))),
            Block::Heading { content, .. } => {
                out.extend(finish(&wrap(&self.inlines(content), "**", "**")));
            }
            Block::Code { text, .. } | Block::Math(text) => {
                out.extend(text.lines().map(code_span).filter(|line| !line.is_empty()));
            }
            Block::Quote(blocks) => {
                let mut inner = Vec::new();
                for block in blocks {
                    self.block(&mut inner, block, true);
                }
                let bar = format!(
                    "<font color=\"{}\">{QUOTE_BAR}</font> ",
                    rgb(self.palette.quote_bar)
                );
                out.extend(inner.iter().map(|paragraph| {
                    paragraph
                        .split(BREAK)
                        .map(|line| format!("{bar}{line}"))
                        .collect::<Vec<_>>()
                        .join(BREAK)
                }));
            }
            Block::BulletList(items) => {
                let items = items.iter().map(|item| finish(&self.inlines(item)));
                if quoted {
                    out.extend(items.flatten().map(|item| format!("{QUOTED_BULLET}{item}")));
                } else {
                    out.push(list(items.map(|item| ("-".to_owned(), item))));
                }
            }
            Block::NumberedList { start, items } => {
                let items = items.iter().map(|item| finish(&self.inlines(item)));
                if quoted {
                    out.extend(
                        (*start..)
                            .zip(items)
                            .filter_map(|(number, item)| Some(format!("{number}\\. {}", item?))),
                    );
                } else {
                    out.push(list(
                        (*start..)
                            .zip(items)
                            .map(|(number, item)| (format!("{number}."), item)),
                    ));
                }
            }
            Block::TaskList(items) => {
                for item in items {
                    let content = self.inlines(&item.content);
                    out.extend(finish(&format!("{}{content}", task_marker(item.done))));
                }
            }
            Block::BigEmoji(emoji) => {
                let text: String = emoji.iter().map(|emoji| self.emoji(emoji)).collect();
                out.extend(finish(&text));
            }
        }
    }

    fn inlines(&self, inlines: &[Inline]) -> String {
        inlines.iter().map(|inline| self.inline(inline)).collect()
    }

    fn inline(&self, inline: &Inline) -> String {
        match inline {
            Inline::Text(text) => escaped(text),
            Inline::Bold(content) => wrap(&self.inlines(content), "**", "**"),
            Inline::Italic(content) => wrap(&self.inlines(content), "*", "*"),
            Inline::Strike(content) => wrap(&self.inlines(content), "~~", "~~"),
            Inline::Code(text) | Inline::Math(text) => code_span(text),
            Inline::Link { url, label } => Self::link(url, &self.inlines(label)),
            Inline::Image { url, alt } => {
                Self::link(url, &escaped(if alt.is_empty() { url } else { alt }))
            }
            Inline::Mention(mention) => {
                let (name, me) = mention_name(mention, self.viewer);
                let at = escaped(&format!("@{name}"));
                if me {
                    colored(&wrap(&at, "**", "**"), self.palette.mention_me)
                } else {
                    colored(&at, self.palette.mention)
                }
            }
            Inline::ChannelMention(name) => {
                colored(&escaped(&format!("#{name}")), self.palette.channel)
            }
            Inline::Emoji(emoji) => self.emoji(emoji),
            Inline::Color { r, g, b, a } => colored(
                &escaped(&color_hex(*r, *g, *b, *a)),
                Color::from_rgb_u8(*r, *g, *b),
            ),
            Inline::Timestamp {
                time,
                format,
                fallback,
            } => escaped(&time_text(*time, format, fallback, self.clock)),
            Inline::LineBreak => BREAK.to_owned(),
        }
    }

    /// A link, or just the label if the URL isn't one to open.
    fn link(url: &str, label: &str) -> String {
        if is_safe_url(url) {
            wrap(label, "[", &format!("]({})", destination(url)))
        } else {
            label.to_owned()
        }
    }

    /// The character, or the `:shortcode:` for one Slint can't show: a custom emoji is an image.
    fn emoji(&self, emoji: &Emoji) -> String {
        match emoji.shown(&self.viewer.emoji) {
            Shown::Unicode(text) => escaped(text),
            Shown::Image { name, .. } | Shown::Unknown(name) => escaped(&format!(":{name}:")),
        }
    }
}

/// A list of `(marker, item)`, an item a line, none for the items that show nothing.
fn list(items: impl Iterator<Item = (String, Option<String>)>) -> String {
    items
        .map(|(marker, item)| match item {
            Some(item) => format!("{marker} {item}"),
            None => marker,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;
    use rc_core::emoji::CustomEmojis;
    use rc_core::rich::{Mention, TaskItem};
    use slint::ComponentHandle as _;

    use super::*;

    fn t(text: &str) -> Inline {
        Inline::Text(text.to_owned())
    }

    fn para(inlines: Vec<Inline>) -> Block {
        Block::Paragraph(inlines)
    }

    fn code(name: &str) -> Emoji {
        Emoji {
            short_code: Some(name.to_owned()),
            unicode: None,
        }
    }

    fn palette() -> Palette {
        Palette {
            mention: Color::from_rgb_u8(0x10, 0x20, 0x30),
            mention_me: Color::from_rgb_u8(0xa0, 0xb0, 0xc0),
            channel: Color::from_rgb_u8(0x11, 0x22, 0x33),
            quote_bar: Color::from_rgb_u8(0x44, 0x55, 0x66),
        }
    }

    fn viewer() -> Viewer {
        Viewer {
            username: Some("alice".into()),
            emoji: CustomEmojis::new(
                "https://chat.example.com",
                &[rc_api::CustomEmoji {
                    name: "party".into(),
                    aliases: Vec::new(),
                    extension: "gif".into(),
                }],
            ),
            ..Viewer::default()
        }
    }

    /// The markdown of the blocks, which Slint must accept; and what it makes of it, as its debug
    /// output (it has no other way to look inside).
    fn shown(blocks: Vec<Block>) -> (String, String) {
        let zone = TimeZone::get("Europe/Lisbon").unwrap();
        let clock = Clock {
            zone: &zone,
            now: "2026-10-20T12:00:00Z".parse().unwrap(),
        };
        let text = RichText {
            text: String::new(),
            blocks,
        };
        let markdown = render(&text, &viewer(), clock, &palette());
        let parsed = StyledText::from_markdown(&markdown)
            .unwrap_or_else(|error| panic!("Slint refuses {markdown:?}: {error}"));
        (markdown, format!("{parsed:?}"))
    }

    fn markdown(blocks: Vec<Block>) -> String {
        shown(blocks).0
    }

    fn inline(inlines: Vec<Inline>) -> String {
        markdown(vec![para(inlines)])
    }

    /// The text of the paragraphs Slint makes, as it shows them.
    fn texts(debug: &str) -> Vec<&str> {
        debug
            .split("text: \"")
            .skip(1)
            .map(|rest| rest.split("\", formatting").next().unwrap())
            .collect()
    }

    // RICH-001
    #[test]
    fn paragraphs_are_separated_by_a_blank_line_and_nothing_is_nothing() {
        let (markdown, debug) = shown(vec![para(vec![t("one")]), para(vec![t("two")])]);
        assert_eq!(markdown, "one\n\ntwo");
        assert_eq!(texts(&debug), ["one", "two"]);
        assert_eq!(self::markdown(Vec::new()), "");
        assert_eq!(inline(Vec::new()), "");
    }

    // RICH-001
    #[test]
    fn text_formatting_nests() {
        let (markdown, debug) = shown(vec![para(vec![Inline::Bold(vec![
            t("a "),
            Inline::Italic(vec![Inline::Strike(vec![t("b")])]),
        ])])]);
        assert_eq!(markdown, "**a *~~b~~***");
        assert_eq!(texts(&debug), ["a b"]);
        assert!(debug.contains("Strong") && debug.contains("Emphasis"));
        assert!(debug.contains("Strikethrough"));
    }

    // RICH-001
    #[test]
    fn whitespace_at_the_edge_of_a_span_stays_outside_it() {
        let (markdown, debug) = shown(vec![para(vec![
            t("a"),
            Inline::Bold(vec![t(" b ")]),
            t("c"),
        ])]);
        assert_eq!(markdown, "a **b** c");
        assert_eq!(texts(&debug), ["a b c"]);
        // A span of only whitespace is only whitespace.
        assert_eq!(
            inline(vec![t("x"), Inline::Bold(vec![t(" ")]), t("y")]),
            "x y"
        );
    }

    // RICH-001
    #[test]
    fn leading_whitespace_is_not_a_code_block() {
        let (markdown, debug) = shown(vec![
            para(vec![t("     indented")]),
            para(vec![t("   ")]),
            para(vec![t("\t\ttabbed")]),
        ]);
        assert_eq!(markdown, "indented\n\ntabbed");
        assert_eq!(texts(&debug), ["indented", "tabbed"]);
    }

    // RICH-001
    #[test]
    fn inline_code_keeps_its_text_whatever_it_holds() {
        let (markdown, debug) = shown(vec![para(vec![Inline::Code("a < b *c* `d`".into())])]);
        assert_eq!(markdown, "`` a < b *c* `d` ``");
        assert_eq!(texts(&debug), ["a < b *c* `d`"]);
        assert_eq!(inline(vec![Inline::Code("`".into())]), "`` ` ``");
        assert_eq!(inline(vec![Inline::Code("  ".into())]), "");
    }

    // RICH-001
    #[test]
    fn a_code_block_is_a_paragraph_of_inline_code_per_line() {
        let (markdown, debug) = shown(vec![Block::Code {
            language: Some("rust".into()),
            text: "fn main() {\n    1 < 2\n\n}".into(),
        }]);
        assert_eq!(markdown, "` fn main() { `\n\n`     1 < 2 `\n\n` } `");
        assert_eq!(texts(&debug), ["fn main() {", "    1 < 2", "}"]);
    }

    // RICH-001
    #[test]
    fn math_shows_as_its_tex_source_in_code() {
        let (block, debug) = shown(vec![Block::Math("x^2 < y".into())]);
        assert_eq!(block, "` x^2 < y `");
        assert_eq!(texts(&debug), ["x^2 < y"]);
        assert_eq!(inline(vec![Inline::Math("a_1".into())]), "` a_1 `");
    }

    // RICH-001
    #[test]
    fn a_heading_is_a_bold_line() {
        let (markdown, debug) = shown(vec![
            Block::Heading {
                level: 2,
                content: vec![t("Plans "), Inline::Italic(vec![t("now")])],
            },
            para(vec![t("text")]),
        ]);
        assert_eq!(markdown, "**Plans *now***\n\ntext");
        assert_eq!(texts(&debug), ["Plans now", "text"]);
    }

    // RICH-001
    #[test]
    fn a_quote_has_a_bar_in_the_quote_colour_before_each_line() {
        let (markdown, debug) = shown(vec![Block::Quote(vec![
            para(vec![t("one"), Inline::LineBreak, t("two")]),
            Block::BulletList(vec![vec![t("item")]]),
            Block::NumberedList {
                start: 3,
                items: vec![vec![t("x")]],
            },
            Block::Quote(vec![para(vec![t("inner")])]),
        ])]);
        let bar = "<font color=\"#445566\">\u{258e}</font> ";
        assert_eq!(
            markdown,
            format!("{bar}one\\\n{bar}two\n\n{bar}\u{2022} item\n\n{bar}3\\. x\n\n{bar}{bar}inner")
        );
        assert_eq!(
            texts(&debug),
            [
                "\u{258e} one",
                "\u{258e} two",
                "\u{258e} \u{2022} item",
                "\u{258e} 3. x",
                "\u{258e} \u{258e} inner"
            ]
        );
    }

    // RICH-001
    #[test]
    fn lists_are_bulleted_or_numbered_from_their_first_number() {
        let (markdown, debug) = shown(vec![
            Block::BulletList(vec![vec![t("one")], vec![Inline::Bold(vec![t("two")])]]),
            Block::NumberedList {
                start: 3,
                items: vec![vec![t("three")], vec![t("four")]],
            },
        ]);
        assert_eq!(markdown, "- one\n- **two**\n\n3. three\n4. four");
        assert!(texts(&debug).iter().any(|text| text.contains("two")));
        assert!(debug.contains("three") && debug.contains("four"));
        // A line break in an item continues it.
        let (markdown, _) = shown(vec![Block::BulletList(vec![vec![
            t("a"),
            Inline::LineBreak,
            t("b"),
        ]])]);
        assert_eq!(markdown, "- a\\\nb");
    }

    // RICH-001
    #[test]
    fn a_task_list_has_a_box_for_each_item() {
        let (markdown, debug) = shown(vec![Block::TaskList(vec![
            TaskItem {
                done: true,
                content: vec![t("done")],
            },
            TaskItem {
                done: false,
                content: vec![t("todo")],
            },
        ])]);
        assert_eq!(markdown, "\u{2611} done\n\n\u{2610} todo");
        assert_eq!(texts(&debug), ["\u{2611} done", "\u{2610} todo"]);
    }

    // RICH-001
    #[test]
    fn a_link_opens_its_url_and_shows_its_label() {
        let (markdown, debug) = shown(vec![para(vec![Inline::Link {
            url: "https://example.com/a_(b)?x=1&y=2".into(),
            label: vec![Inline::Bold(vec![t("the site")])],
        }])]);
        assert_eq!(
            markdown,
            "[**the site**](https://example.com/a_\\(b\\)?x=1\\&y=2)"
        );
        assert_eq!(texts(&debug), ["the site"]);
        assert!(
            debug.contains("https://example.com/a_(b)?x=1&y=2"),
            "{debug}"
        );
        let (_, debug) = shown(vec![para(vec![Inline::Link {
            url: "mailto:a b@example.com".into(),
            label: vec![t("mail")],
        }])]);
        assert!(debug.contains("mailto:a%20b@example.com"), "{debug}");
    }

    // RICH-001
    #[test]
    fn a_link_to_something_other_than_the_web_or_mail_is_only_its_label() {
        for url in ["javascript:alert(1)", "file:///etc/passwd", "/relative", ""] {
            let (markdown, debug) = shown(vec![para(vec![Inline::Link {
                url: url.into(),
                label: vec![t("click")],
            }])]);
            assert_eq!(markdown, "click", "{url}");
            assert!(!debug.contains("links: [("), "{url}: {debug}");
        }
    }

    // RICH-001
    #[test]
    fn an_image_is_a_link_to_it_with_its_alt_text() {
        assert_eq!(
            inline(vec![Inline::Image {
                url: "https://example.com/a.png".into(),
                alt: "a *cat*".into(),
            }]),
            "[a \\*cat\\*](https://example.com/a.png)"
        );
        assert_eq!(
            inline(vec![Inline::Image {
                url: "https://example.com/a.png".into(),
                alt: String::new(),
            }]),
            "[https\\:\\/\\/example\\.com\\/a\\.png](https://example.com/a.png)"
        );
        assert_eq!(
            inline(vec![Inline::Image {
                url: "data:image/png;base64,AA".into(),
                alt: "pixel".into(),
            }]),
            "pixel"
        );
    }

    // RICH-001
    #[test]
    fn a_line_break_starts_a_paragraph_and_spans_do_not_cross_it() {
        let (markdown, debug) = shown(vec![para(vec![
            t("a"),
            Inline::LineBreak,
            t("b"),
            Inline::Bold(vec![t("c"), Inline::LineBreak, t("d")]),
        ])]);
        assert_eq!(markdown, "a\\\nb**c**\\\n**d**");
        assert_eq!(texts(&debug), ["a", "bc", "d"]);
        // Nor at the end of a paragraph.
        assert_eq!(inline(vec![t("a"), Inline::LineBreak]), "a");
        // A newline in the text is a break too.
        assert_eq!(inline(vec![t("a\n   b")]), "a\\\nb");
    }

    // RICH-001
    #[test]
    fn a_color_shows_its_value_in_that_colour() {
        let (markdown, debug) = shown(vec![para(vec![
            Inline::Color {
                r: 255,
                g: 0,
                b: 16,
                a: 255,
            },
            t(" "),
            Inline::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        ])]);
        assert_eq!(
            markdown,
            "<font color=\"#ff0010\">\\#ff0010</font> <font color=\"#000000\">\\#00000000</font>"
        );
        assert_eq!(texts(&debug), ["#ff0010 #00000000"]);
    }

    // RICH-001
    #[test]
    fn a_timestamp_is_in_the_local_time_in_the_format_asked_for() {
        // 2026-10-20T14:35:09Z is 15:35:09 in Lisbon (WEST, UTC+1).
        let time = Some("2026-10-20T14:35:09Z".parse().unwrap());
        let shown = |format: &str| {
            inline(vec![Inline::Timestamp {
                time,
                format: format.into(),
                fallback: "<fallback>".into(),
            }])
        };
        assert_eq!(shown("t"), "15\\:35");
        assert_eq!(shown("T"), "15\\:35\\:09");
        assert_eq!(shown("d"), "2026\\-10\\-20");
        assert_eq!(shown("R"), "in 2 hours");
        assert_eq!(shown("?"), "\\<fallback\\>");
        assert_eq!(
            inline(vec![Inline::Timestamp {
                time: None,
                format: "t".into(),
                fallback: "<t:x:t>".into(),
            }]),
            "\\<t\\:x\\:t\\>"
        );
    }

    // RICH-001
    #[test]
    fn user_text_cannot_inject_markup() {
        let nasty = "*_\\`[]<>#!-+.~|&()\"'{}=:@ <b>bold</b> <font color=red>x</font> # h\n> q\n1. n\n- i\n```\n---";
        let (markdown, debug) = shown(vec![
            para(vec![t(nasty)]),
            Block::Heading {
                level: 1,
                content: vec![t(nasty)],
            },
            Block::BulletList(vec![vec![t(nasty)]]),
            Block::Quote(vec![para(vec![t(nasty)])]),
        ]);
        assert!(!markdown.contains("<b>"), "{markdown}");
        // Everything is text: no styles other than the heading's bold, and no links.
        assert!(
            !debug.contains("Link") && !debug.contains("Code"),
            "{debug}"
        );
        // The paragraph is the first line of the text as it is (compared as Slint prints them).
        let line = nasty.lines().next().unwrap();
        let plain = format!("{:?}", StyledText::from_plain_text(line));
        assert_eq!(texts(&debug)[0], texts(&plain)[0]);
        // Each line is a paragraph, and the ones that look like markdown are text.
        let all = texts(&debug);
        for markup in ["> q", "1. n", "- i", "```", "---"] {
            assert!(all.contains(&markup), "{markup:?} in {all:?}");
        }
    }

    // RICH-001
    #[test]
    fn user_text_is_escaped_in_every_place_it_appears() {
        let nasty = "<b>*";
        let (markdown, debug) = shown(vec![
            para(vec![
                Inline::Bold(vec![t(nasty)]),
                Inline::Link {
                    url: "https://example.com".into(),
                    label: vec![t(nasty)],
                },
                Inline::ChannelMention(nasty.into()),
                Inline::Mention(Mention::User(nasty.into())),
                Inline::Emoji(code(nasty)),
            ]),
            Block::TaskList(vec![TaskItem {
                done: false,
                content: vec![t(nasty)],
            }]),
        ]);
        assert!(!markdown.contains("<b>"), "{markdown}");
        assert!(texts(&debug)[0].contains("<b>*<b>*"), "{debug}");
    }

    // RICH-002
    #[test]
    fn mentions_stand_out_and_the_ones_of_the_viewer_more() {
        let (markdown, debug) = shown(vec![para(vec![
            Inline::Mention(Mention::User("bob".into())),
            t(" "),
            Inline::Mention(Mention::User("Alice".into())),
            t(" "),
            Inline::Mention(Mention::All),
            t(" "),
            Inline::Mention(Mention::Here),
            t(" "),
            Inline::Mention(Mention::User("a_b".into())),
        ])]);
        let other = "<font color=\"#102030\">\\@bob</font>";
        let me = |name: &str| format!("<font color=\"#a0b0c0\">**\\@{name}**</font>");
        assert_eq!(
            markdown,
            format!(
                "{other} {} {} {} <font color=\"#102030\">\\@a\\_b</font>",
                me("Alice"),
                me("all"),
                me("here")
            )
        );
        assert_eq!(texts(&debug), ["@bob @Alice @all @here @a_b"]);
    }

    // RICH-002
    #[test]
    fn nobody_is_mentioned_when_nobody_is_logged_in() {
        let text = RichText {
            text: String::new(),
            blocks: vec![para(vec![Inline::Mention(Mention::User("alice".into()))])],
        };
        assert_eq!(
            render(&text, &Viewer::default(), Clock::timeless(), &palette()),
            "<font color=\"#102030\">\\@alice</font>"
        );
    }

    // RICH-002
    #[test]
    fn a_channel_mention_is_marked_apart_from_user_mentions() {
        let (markdown, debug) = shown(vec![para(vec![Inline::ChannelMention("general".into())])]);
        assert_eq!(markdown, "<font color=\"#112233\">\\#general</font>");
        assert_eq!(texts(&debug), ["#general"]);
    }

    // RICH-003
    #[test]
    fn a_shortcode_shows_as_unicode_and_a_unicode_emoji_as_it_is() {
        let (markdown, debug) = shown(vec![para(vec![
            Inline::Emoji(code("rocket")),
            Inline::Emoji(Emoji {
                short_code: None,
                unicode: Some("\u{1F389}".into()),
            }),
            Inline::Emoji(Emoji {
                short_code: Some("smile".into()),
                unicode: Some("\u{1F604}".into()),
            }),
        ])]);
        assert_eq!(markdown, "\u{1F680}\u{1F389}\u{1F604}");
        assert_eq!(texts(&debug), ["\u{1F680}\u{1F389}\u{1F604}"]);
    }

    // RICH-003
    #[test]
    fn a_custom_or_unknown_emoji_is_its_shortcode() {
        let (markdown, debug) = shown(vec![para(vec![
            Inline::Emoji(code("party")),
            Inline::Emoji(code("no_such_emoji")),
        ])]);
        assert_eq!(markdown, "\\:party\\:\\:no\\_such\\_emoji\\:");
        assert_eq!(texts(&debug), [":party::no_such_emoji:"]);
    }

    // RICH-003
    #[test]
    fn a_message_of_nothing_but_emoji_is_plain_characters_and_says_it_is_big() {
        let big = RichText {
            text: String::new(),
            blocks: vec![Block::BigEmoji(vec![code("rocket"), code("party")])],
        };
        assert!(is_big_emoji(&big));
        assert!(!is_big_emoji(&RichText::plain("hi")));
        assert!(!is_big_emoji(&RichText {
            text: String::new(),
            blocks: vec![para(vec![Inline::Emoji(code("rocket"))])],
        }));
        let (markdown, debug) = shown(big.blocks);
        assert_eq!(markdown, "\u{1F680}\\:party\\:");
        assert_eq!(texts(&debug), ["\u{1F680}:party:"]);
    }

    // ROOM-009
    #[test]
    fn a_topic_is_markdown_shown_as_styled_text_and_empty_for_none() {
        let palette = palette();
        let topic = topic(
            "**Plans** for <b>May</b> & https://example.com :rocket:",
            &Viewer::default(),
            &palette,
        );
        let debug = format!("{topic:?}");
        assert_eq!(
            texts(&debug),
            ["Plans for <b>May</b> & https://example.com \u{1F680}"]
        );
        assert!(debug.contains("Strong"), "{debug}");
        assert!(debug.contains("links: [(") && debug.contains("\"https://example.com\""));
        assert_eq!(
            super::topic("", &Viewer::default(), &palette),
            StyledText::default()
        );
    }

    // ROOM-009
    #[test]
    fn a_topic_shows_a_mention_and_the_custom_emoji_as_text() {
        let topic = topic("ask @bob :party:", &viewer(), &palette());
        assert_eq!(texts(&format!("{topic:?}")), ["ask @bob :party:"]);
    }

    // ROOM-009
    #[test]
    fn the_palette_comes_from_the_theme() {
        i_slint_backend_testing::init_no_event_loop();
        let window = crate::new_main_window().unwrap();
        let theme = window.global::<Theme>();
        let palette = Palette::from_theme(&theme);
        assert_eq!(palette.mention, theme.get_own());
        assert_eq!(palette.channel, theme.get_own());
        assert_eq!(palette.mention_me, theme.get_signal());
        assert_eq!(palette.quote_bar, theme.get_raised_hover());
    }
}
