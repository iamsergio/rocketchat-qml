// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Renders a message's `RichText` as the HTML subset that Qt's rich text (`Text.RichText`)
//! supports. Plain Rust, so it can be unit-tested without Qt. Everything the author typed goes
//! through [`escape`]; the only markup is what this module writes.
//!
//! No colour is a palette colour: highlights are translucent, so they read on light and dark
//! themes alike, and links take QML's `linkColor`.

use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::emoji::{CustomEmojis, Shown};
use rc_core::rich::{Block, Emoji, Inline, Mention, RichText};

const CODE_BACKGROUND: &str = "rgba(127,127,127,0.2)";
/// `CODE_BACKGROUND` as `#AARRGGBB`, for a `bgcolor` attribute.
const CODE_BLOCK_BACKGROUND: &str = "#337f7f7f";
const QUOTE_BAR: &str = "rgba(127,127,127,0.7)";
const MENTION: &str = "rgba(33,150,243,0.3)";
/// The user, `@all` and `@here`.
const MENTION_ME: &str = "rgba(255,152,0,0.55)";
const CHANNEL: &str = "rgba(76,175,80,0.3)";

/// The edge of a custom emoji image inline, and in a message of nothing but emoji.
const EMOJI_SIZE: u32 = 20;
const BIG_EMOJI_SIZE: u32 = 40;
/// A message of nothing but emoji, in pixels.
const BIG_EMOJI_FONT: u32 = 32;

/// Whoever looks at the messages: it decides which mentions stand out, which emoji exist and where
/// the links of attachments lead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Viewer {
    /// The logged-in user, if any.
    pub username: Option<String>,
    /// The URL the user logged in with: attachment links are relative to it.
    pub server: Option<String>,
    pub emoji: CustomEmojis,
}

/// Where times are read and when it is now, so that tests don't depend on either.
#[derive(Clone, Copy)]
pub struct Clock<'a> {
    pub zone: &'a TimeZone,
    pub now: Timestamp,
}

/// The HTML of the body, for `Text.RichText`.
#[must_use]
pub fn render(text: &RichText, viewer: &Viewer, clock: Clock) -> String {
    let mut out = String::new();
    let renderer = Renderer { viewer, clock };
    for block in &text.blocks {
        renderer.block(&mut out, block);
    }
    out
}

/// `text` with the characters that mean something in HTML replaced.
fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// Whether a link to `url` is worth making: other schemes (`file:`, `javascript:`, ...) are not
/// something a message should be able to open.
fn is_safe_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

pub(crate) fn is_web_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

struct Renderer<'a> {
    viewer: &'a Viewer,
    clock: Clock<'a>,
}

impl Renderer<'_> {
    fn block(&self, out: &mut String, block: &Block) {
        match block {
            // The server sends one paragraph per line: no space between them.
            Block::Paragraph(content) => {
                out.push_str("<p style=\"margin-top:0px; margin-bottom:0px\">");
                self.inlines(out, content);
                out.push_str("</p>");
            }
            Block::Heading { level, content } => {
                let level = (*level).clamp(1, 6);
                let _ = write!(
                    out,
                    "<h{level} style=\"margin-top:6px; margin-bottom:2px\">"
                );
                self.inlines(out, content);
                let _ = write!(out, "</h{level}>");
            }
            Block::Code { text, .. } | Block::Math(text) => code_block(out, text),
            Block::Quote(blocks) => {
                let _ = write!(
                    out,
                    "<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:2px; \
                     margin-bottom:2px\"><tr><td width=\"3\" \
                     style=\"background-color:{QUOTE_BAR}\"></td><td style=\"padding-left:8px\">"
                );
                for block in blocks {
                    self.block(out, block);
                }
                out.push_str("</td></tr></table>");
            }
            Block::BulletList(items) => {
                out.push_str("<ul style=\"margin-top:2px; margin-bottom:2px\">");
                for item in items {
                    out.push_str("<li>");
                    self.inlines(out, item);
                    out.push_str("</li>");
                }
                out.push_str("</ul>");
            }
            Block::NumberedList { start, items } => {
                let _ = write!(
                    out,
                    "<ol start=\"{start}\" style=\"margin-top:2px; margin-bottom:2px\">"
                );
                for item in items {
                    out.push_str("<li>");
                    self.inlines(out, item);
                    out.push_str("</li>");
                }
                out.push_str("</ol>");
            }
            Block::TaskList(items) => {
                for item in items {
                    out.push_str(
                        "<p style=\"margin-top:0px; margin-bottom:0px; margin-left:16px\">",
                    );
                    out.push_str(if item.done { "\u{2611} " } else { "\u{2610} " });
                    self.inlines(out, &item.content);
                    out.push_str("</p>");
                }
            }
            Block::BigEmoji(emoji) => {
                let _ = write!(
                    out,
                    "<p style=\"margin-top:0px; margin-bottom:0px; font-size:{BIG_EMOJI_FONT}px\">"
                );
                for emoji in emoji {
                    self.emoji(out, emoji, BIG_EMOJI_SIZE);
                }
                out.push_str("</p>");
            }
        }
    }

    fn inlines(&self, out: &mut String, inlines: &[Inline]) {
        for inline in inlines {
            self.inline(out, inline);
        }
    }

    fn inline(&self, out: &mut String, inline: &Inline) {
        match inline {
            Inline::Text(text) => push_escaped(out, text),
            Inline::Bold(content) => self.wrapped(out, "b", content),
            Inline::Italic(content) => self.wrapped(out, "i", content),
            Inline::Strike(content) => self.wrapped(out, "s", content),
            Inline::Code(text) | Inline::Math(text) => {
                let _ = write!(
                    out,
                    "<code style=\"font-family:monospace; background-color:{CODE_BACKGROUND}\">"
                );
                push_escaped(out, text);
                out.push_str("</code>");
            }
            Inline::Link { url, label } => Self::link(out, url, |out| self.inlines(out, label)),
            Inline::Image { url, alt } => Self::link(out, url, |out| {
                push_escaped(out, if alt.is_empty() { url } else { alt });
            }),
            Inline::Mention(mention) => self.mention(out, mention),
            Inline::ChannelMention(name) => {
                let _ = write!(out, "<span style=\"background-color:{CHANNEL}\">#");
                push_escaped(out, name);
                out.push_str("</span>");
            }
            Inline::Emoji(emoji) => self.emoji(out, emoji, EMOJI_SIZE),
            Inline::Color { r, g, b, a } => color(out, *r, *g, *b, *a),
            Inline::Timestamp {
                time,
                format,
                fallback,
            } => push_escaped(out, &time_text(*time, format, fallback, self.clock)),
            Inline::LineBreak => out.push_str("<br/>"),
        }
    }

    fn wrapped(&self, out: &mut String, tag: &str, content: &[Inline]) {
        let _ = write!(out, "<{tag}>");
        self.inlines(out, content);
        let _ = write!(out, "</{tag}>");
    }

    /// A link, or just the label if the URL isn't one to open.
    fn link(out: &mut String, url: &str, label: impl FnOnce(&mut String)) {
        if is_safe_url(url) {
            out.push_str("<a href=\"");
            push_escaped(out, url);
            out.push_str("\">");
            label(out);
            out.push_str("</a>");
        } else {
            label(out);
        }
    }

    fn mention(&self, out: &mut String, mention: &Mention) {
        let (name, me) = match mention {
            Mention::All => ("all", true),
            Mention::Here => ("here", true),
            Mention::User(name) => (
                name.as_str(),
                self.viewer
                    .username
                    .as_deref()
                    .is_some_and(|me| me.eq_ignore_ascii_case(name)),
            ),
        };
        if me {
            let _ = write!(out, "<span style=\"background-color:{MENTION_ME}\"><b>@");
            push_escaped(out, name);
            out.push_str("</b></span>");
        } else {
            let _ = write!(out, "<span style=\"background-color:{MENTION}\">@");
            push_escaped(out, name);
            out.push_str("</span>");
        }
    }

    fn emoji(&self, out: &mut String, emoji: &Emoji, size: u32) {
        match emoji.shown(&self.viewer.emoji) {
            Shown::Unicode(text) => push_escaped(out, text),
            Shown::Image { url, .. } if is_web_url(url) => {
                out.push_str("<img src=\"");
                push_escaped(out, url);
                let _ = write!(
                    out,
                    "\" width=\"{size}\" height=\"{size}\" style=\"vertical-align:middle\"/>"
                );
            }
            Shown::Image { name, .. } | Shown::Unknown(name) => {
                out.push(':');
                push_escaped(out, name);
                out.push(':');
            }
        }
    }
}

/// Monospace on a background, padded by a one-cell table: block backgrounds have no padding. The
/// background is on the table: on the cell, Qt paints it again behind each line of the `<pre>`.
fn code_block(out: &mut String, text: &str) {
    let _ = write!(
        out,
        "<table width=\"100%\" cellspacing=\"0\" cellpadding=\"6\" \
         bgcolor=\"{CODE_BLOCK_BACKGROUND}\" style=\"margin-top:2px; margin-bottom:2px\"><tr><td>\
         <pre style=\"margin:0px; font-family:monospace\">"
    );
    push_escaped(out, text);
    out.push_str("</pre></td></tr></table>");
}

fn color(out: &mut String, r: u8, g: u8, b: u8, a: u8) {
    let hex = if a == u8::MAX {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    };
    let css = if a == u8::MAX {
        hex.clone()
    } else {
        format!("rgba({r},{g},{b},{})", f64::from(a) / 255.0)
    };
    let _ = write!(
        out,
        "<span style=\"background-color:{css}\">&nbsp;&nbsp;&nbsp;</span> {hex}"
    );
}

/// A timestamp the way the author's format letter asks (`t` time, `T` with seconds, `d` date, `D`
/// long date, `f` and `F` date and time, `R` relative), in the user's time zone. The author's own
/// text if it isn't a time or the format is unknown.
fn time_text(time: Option<Timestamp>, format: &str, fallback: &str, clock: Clock) -> String {
    let Some(time) = time else {
        return fallback.to_owned();
    };
    let zoned = time.to_zoned(clock.zone.clone());
    let pattern = match format {
        "t" => "%H:%M",
        "T" => "%H:%M:%S",
        "d" => "%Y-%m-%d",
        "D" => "%-d %B %Y",
        "f" => "%-d %B %Y %H:%M",
        "F" => "%A, %-d %B %Y %H:%M",
        "R" => return relative(time, clock.now),
        _ => return fallback.to_owned(),
    };
    zoned.strftime(pattern).to_string()
}

/// `3 hours ago`, `in 2 days`: the largest unit that fits at least once.
fn relative(time: Timestamp, now: Timestamp) -> String {
    let seconds = now.as_second() - time.as_second();
    let amount = seconds.unsigned_abs();
    let units = [(86_400, "day"), (3_600, "hour"), (60, "minute")];
    let (count, unit) = units
        .iter()
        .find(|(size, _)| amount >= *size)
        .map_or((amount, "second"), |(size, name)| (amount / size, *name));
    let plural = if count == 1 { "" } else { "s" };
    if seconds >= 0 {
        format!("{count} {unit}{plural} ago")
    } else {
        format!("in {count} {unit}{plural}")
    }
}

#[cfg(test)]
mod tests {
    use rc_core::rich::TaskItem;

    use super::*;

    fn t(text: &str) -> Inline {
        Inline::Text(text.to_owned())
    }

    fn para(inlines: Vec<Inline>) -> Block {
        Block::Paragraph(inlines)
    }

    fn user(name: &str) -> Inline {
        Inline::Mention(Mention::User(name.to_owned()))
    }

    fn code(name: &str) -> Emoji {
        Emoji {
            short_code: Some(name.to_owned()),
            unicode: None,
        }
    }

    fn viewer() -> Viewer {
        Viewer {
            username: Some("alice".into()),
            server: None,
            emoji: CustomEmojis::new(
                "https://chat.example.com",
                &[rc_api::CustomEmoji {
                    name: "party".into(),
                    aliases: Vec::new(),
                    extension: "gif".into(),
                }],
            ),
        }
    }

    fn html_with(blocks: Vec<Block>, viewer: &Viewer) -> String {
        let zone = TimeZone::get("Europe/Lisbon").unwrap();
        let clock = Clock {
            zone: &zone,
            now: "2026-10-20T12:00:00Z".parse().unwrap(),
        };
        let text = RichText {
            text: String::new(),
            blocks,
        };
        render(&text, viewer, clock)
    }

    fn html(blocks: Vec<Block>) -> String {
        html_with(blocks, &viewer())
    }

    fn inline_html(inlines: Vec<Inline>) -> String {
        html(vec![para(inlines)])
    }

    /// The HTML of a paragraph, without the paragraph.
    fn body(inlines: Vec<Inline>) -> String {
        let html = inline_html(inlines);
        html.strip_prefix("<p style=\"margin-top:0px; margin-bottom:0px\">")
            .and_then(|html| html.strip_suffix("</p>"))
            .unwrap()
            .to_owned()
    }

    // RICH-001
    #[test]
    fn a_paragraph_has_no_space_around_it() {
        assert_eq!(
            html(vec![para(vec![t("one")]), para(vec![t("two")])]),
            "<p style=\"margin-top:0px; margin-bottom:0px\">one</p>\
             <p style=\"margin-top:0px; margin-bottom:0px\">two</p>"
        );
    }

    // RICH-001
    #[test]
    fn nothing_renders_as_nothing() {
        assert_eq!(html(Vec::new()), "");
    }

    // RICH-001
    #[test]
    fn text_formatting_nests() {
        assert_eq!(
            body(vec![Inline::Bold(vec![
                t("a "),
                Inline::Italic(vec![Inline::Strike(vec![t("b")])]),
            ])]),
            "<b>a <i><s>b</s></i></b>"
        );
    }

    // RICH-001
    #[test]
    fn inline_code_is_monospace_on_a_background() {
        assert_eq!(
            body(vec![Inline::Code("a < b".into())]),
            "<code style=\"font-family:monospace; background-color:rgba(127,127,127,0.2)\">\
             a &lt; b</code>"
        );
    }

    // RICH-001
    #[test]
    fn a_code_block_is_monospace_on_a_background_and_keeps_its_lines() {
        let html = html(vec![Block::Code {
            language: Some("rust".into()),
            text: "fn main() {\n    1 < 2\n}".into(),
        }]);
        assert!(html.contains("bgcolor=\"#337f7f7f\""));
        assert!(html.contains("<pre style=\"margin:0px; font-family:monospace\">"));
        assert!(html.contains("fn main() {\n    1 &lt; 2\n}</pre>"));
    }

    // RICH-001
    #[test]
    fn math_shows_as_its_tex_source_in_monospace() {
        let block = html(vec![Block::Math("x^2 < y".into())]);
        assert!(block.contains("font-family:monospace"));
        assert!(block.contains("x^2 &lt; y"));
        let inline = body(vec![Inline::Math("a_1".into())]);
        assert!(inline.starts_with("<code style=\"font-family:monospace"));
        assert!(inline.contains(">a_1</code>"));
    }

    // RICH-001
    #[test]
    fn a_quote_is_marked_with_a_bar_and_holds_blocks() {
        let html = html(vec![Block::Quote(vec![
            para(vec![t("quoted")]),
            Block::BulletList(vec![vec![t("item")]]),
        ])]);
        assert!(html.starts_with("<table"));
        assert!(html.contains("<td width=\"3\" style=\"background-color:rgba(127,127,127,0.7)\">"));
        assert!(html.contains(">quoted</p>"));
        assert!(html.contains("<li>item</li>"));
        assert!(html.ends_with("</td></tr></table>"));
    }

    // RICH-001
    #[test]
    fn lists_are_bulleted_or_numbered_from_their_first_number() {
        let bullets = html(vec![Block::BulletList(vec![vec![t("a")], vec![t("b")]])]);
        assert!(bullets.starts_with("<ul"));
        assert!(bullets.contains("<li>a</li><li>b</li></ul>"));
        let numbers = html(vec![Block::NumberedList {
            start: 3,
            items: vec![vec![t("c")], vec![Inline::Bold(vec![t("d")])]],
        }]);
        assert!(numbers.starts_with("<ol start=\"3\""));
        assert!(numbers.contains("<li>c</li><li><b>d</b></li></ol>"));
    }

    // RICH-001
    #[test]
    fn a_task_list_has_a_box_for_each_item() {
        let html = html(vec![Block::TaskList(vec![
            TaskItem {
                done: true,
                content: vec![t("done")],
            },
            TaskItem {
                done: false,
                content: vec![t("todo")],
            },
        ])]);
        assert!(!html.contains("<li>"));
        assert!(html.contains(">\u{2611} done</p>") && html.contains(">\u{2610} todo</p>"));
    }

    // RICH-001
    #[test]
    fn a_heading_has_its_level() {
        let html = html(vec![
            Block::Heading {
                level: 2,
                content: vec![t("Title")],
            },
            Block::Heading {
                level: 9,
                content: vec![t("Clamped")],
            },
        ]);
        assert!(html.contains("<h2 style=\"margin-top:6px; margin-bottom:2px\">Title</h2>"));
        assert!(html.contains(">Clamped</h6>"));
    }

    // RICH-001
    #[test]
    fn a_link_opens_its_url_and_shows_its_label() {
        assert_eq!(
            body(vec![Inline::Link {
                url: "https://example.com/?a=1&b=\"2\"".into(),
                label: vec![Inline::Bold(vec![t("a <site>")])],
            }]),
            "<a href=\"https://example.com/?a=1&amp;b=&quot;2&quot;\"><b>a &lt;site&gt;</b></a>"
        );
    }

    // RICH-001
    #[test]
    fn a_link_to_something_other_than_the_web_or_mail_is_only_its_label() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "  JavaScript:alert(1)",
            "data:text/html,x",
            "/relative",
        ] {
            assert_eq!(
                body(vec![Inline::Link {
                    url: url.into(),
                    label: vec![t("click")],
                }]),
                "click",
                "{url}"
            );
        }
        assert!(
            body(vec![Inline::Link {
                url: "MAILTO:a@example.com".into(),
                label: vec![t("mail")],
            }])
            .starts_with("<a href=")
        );
    }

    // RICH-001
    #[test]
    fn an_image_is_a_link_to_it_with_its_alt_text() {
        assert_eq!(
            body(vec![Inline::Image {
                url: "https://example.com/a.png".into(),
                alt: "a <cat>".into(),
            }]),
            "<a href=\"https://example.com/a.png\">a &lt;cat&gt;</a>"
        );
        assert_eq!(
            body(vec![Inline::Image {
                url: "https://example.com/a.png".into(),
                alt: String::new(),
            }]),
            "<a href=\"https://example.com/a.png\">https://example.com/a.png</a>"
        );
    }

    // RICH-001
    #[test]
    fn a_line_break_is_a_br() {
        assert_eq!(body(vec![t("a"), Inline::LineBreak, t("b")]), "a<br/>b");
    }

    // RICH-001
    #[test]
    fn a_color_shows_a_chip_and_its_value() {
        let opaque = body(vec![Inline::Color {
            r: 255,
            g: 0,
            b: 16,
            a: 255,
        }]);
        assert_eq!(
            opaque,
            "<span style=\"background-color:#ff0010\">&nbsp;&nbsp;&nbsp;</span> #ff0010"
        );
        let clear = body(vec![Inline::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }]);
        assert!(clear.contains("rgba(0,0,0,0)"));
        assert!(clear.ends_with(" #00000000"));
    }

    // RICH-001
    #[test]
    fn a_timestamp_is_in_the_local_time_in_the_format_asked_for() {
        // 2026-10-20T14:35:09Z is 15:35:09 in Lisbon (WEST, UTC+1).
        let time = Some("2026-10-20T14:35:09Z".parse().unwrap());
        let shown = |format: &str| {
            body(vec![Inline::Timestamp {
                time,
                format: format.into(),
                fallback: "<fallback>".into(),
            }])
        };
        assert_eq!(shown("t"), "15:35");
        assert_eq!(shown("T"), "15:35:09");
        assert_eq!(shown("d"), "2026-10-20");
        assert_eq!(shown("D"), "20 October 2026");
        assert_eq!(shown("f"), "20 October 2026 15:35");
        assert_eq!(shown("F"), "Tuesday, 20 October 2026 15:35");
        assert_eq!(shown("R"), "in 2 hours");
        assert_eq!(shown("?"), "&lt;fallback&gt;");
    }

    // RICH-001
    #[test]
    fn a_timestamp_that_is_not_a_time_shows_what_the_author_typed() {
        assert_eq!(
            body(vec![Inline::Timestamp {
                time: None,
                format: "t".into(),
                fallback: "<t:x:t>".into(),
            }]),
            "&lt;t:x:t&gt;"
        );
    }

    // RICH-001
    #[test]
    fn a_relative_time_uses_the_largest_unit_and_looks_both_ways() {
        let now: Timestamp = "2026-10-20T12:00:00Z".parse().unwrap();
        let at = |text: &str| relative(text.parse().unwrap(), now);
        assert_eq!(at("2026-10-20T11:59:30Z"), "30 seconds ago");
        assert_eq!(at("2026-10-20T11:59:00Z"), "1 minute ago");
        assert_eq!(at("2026-10-20T09:00:00Z"), "3 hours ago");
        assert_eq!(at("2026-10-18T12:00:00Z"), "2 days ago");
        assert_eq!(at("2026-10-20T14:00:00Z"), "in 2 hours");
        assert_eq!(at("2026-10-20T12:00:00Z"), "0 seconds ago");
    }

    // RICH-001
    #[test]
    fn a_message_of_nothing_but_emoji_is_large() {
        let html = html(vec![Block::BigEmoji(vec![
            code("rocket"),
            code("party"),
            Emoji {
                short_code: None,
                unicode: Some("🎉".into()),
            },
        ])]);
        assert!(html.contains("font-size:32px"));
        assert!(html.contains("🚀"));
        assert!(html.contains("width=\"40\" height=\"40\""));
        assert!(html.contains("🎉"));
    }

    // RICH-001
    #[test]
    fn user_text_cannot_inject_markup() {
        let evil = "<img src=x onerror=alert(1)>&amp;\"'";
        let escaped = "&lt;img src=x onerror=alert(1)&gt;&amp;amp;&quot;'";
        let user_text = |inline| body(vec![inline]);
        assert_eq!(user_text(t(evil)), escaped);
        assert_eq!(
            user_text(Inline::Code(evil.into()))
                .matches(escaped)
                .count(),
            1
        );
        assert!(user_text(Inline::Math(evil.into())).contains(escaped));
        assert!(user_text(Inline::ChannelMention(evil.into())).contains(escaped));
        assert!(user_text(user(evil)).contains(escaped));
        assert!(user_text(Inline::Emoji(code(evil))).contains(escaped));
        assert!(
            user_text(Inline::Timestamp {
                time: None,
                format: String::new(),
                fallback: evil.into(),
            })
            .contains(escaped)
        );
        assert!(
            user_text(Inline::Link {
                url: format!("https://example.com/{evil}"),
                label: vec![t(evil)],
            })
            .contains(escaped)
        );
        let block = html(vec![
            Block::Code {
                language: Some(evil.into()),
                text: evil.into(),
            },
            Block::Math(evil.into()),
            Block::Heading {
                level: 1,
                content: vec![t(evil)],
            },
            Block::BulletList(vec![vec![t(evil)]]),
        ]);
        assert!(!block.contains("<img"));
        assert_eq!(block.matches(escaped).count(), 4);
    }

    // RICH-001
    #[test]
    fn escaping_replaces_the_characters_of_markup() {
        assert_eq!(body(vec![t("a<b>&\"c\"")]), "a&lt;b&gt;&amp;&quot;c&quot;");
        assert_eq!(body(vec![t("plain 🚀")]), "plain 🚀");
    }

    // RICH-002
    #[test]
    fn mentions_stand_out_and_the_ones_of_the_viewer_more() {
        let other = body(vec![user("bob")]);
        let me = body(vec![user("Alice")]);
        assert_eq!(
            other,
            "<span style=\"background-color:rgba(33,150,243,0.3)\">@bob</span>"
        );
        assert_eq!(
            me,
            "<span style=\"background-color:rgba(255,152,0,0.55)\"><b>@Alice</b></span>"
        );
        for everybody in [Mention::All, Mention::Here] {
            let shown = body(vec![Inline::Mention(everybody)]);
            assert!(shown.contains("rgba(255,152,0,0.55)"), "{shown}");
        }
    }

    // RICH-002
    #[test]
    fn nobody_is_mentioned_when_nobody_is_logged_in() {
        let nobody = Viewer::default();
        let shown = html_with(vec![para(vec![user("alice")])], &nobody);
        assert!(shown.contains("rgba(33,150,243,0.3)"));
    }

    // RICH-002
    #[test]
    fn a_channel_mention_is_marked_apart_from_user_mentions() {
        assert_eq!(
            body(vec![Inline::ChannelMention("general".into())]),
            "<span style=\"background-color:rgba(76,175,80,0.3)\">#general</span>"
        );
    }

    // RICH-003
    #[test]
    fn a_shortcode_shows_as_unicode() {
        assert_eq!(body(vec![Inline::Emoji(code("smile"))]), "😄");
    }

    // RICH-003
    #[test]
    fn a_unicode_emoji_shows_as_it_is() {
        let emoji = Emoji {
            short_code: None,
            unicode: Some("😀".into()),
        };
        assert_eq!(body(vec![Inline::Emoji(emoji)]), "😀");
    }

    // RICH-003
    #[test]
    fn a_custom_emoji_is_an_image_of_a_fixed_size() {
        assert_eq!(
            body(vec![Inline::Emoji(code("party"))]),
            "<img src=\"https://chat.example.com/emoji-custom/party.gif\" width=\"20\" \
             height=\"20\" style=\"vertical-align:middle\"/>"
        );
    }

    // RICH-003
    #[test]
    fn an_unknown_shortcode_stays_as_typed() {
        assert_eq!(body(vec![Inline::Emoji(code("no_such"))]), ":no_such:");
    }

    // RICH-003
    #[test]
    fn a_custom_emoji_that_is_not_on_the_web_stays_as_typed() {
        let viewer = Viewer {
            username: None,
            server: None,
            emoji: CustomEmojis::new(
                "file:///etc",
                &[rc_api::CustomEmoji {
                    name: "party".into(),
                    aliases: Vec::new(),
                    extension: "gif".into(),
                }],
            ),
        };
        let shown = html_with(vec![para(vec![Inline::Emoji(code("party"))])], &viewer);
        assert!(shown.contains(":party:"));
        assert!(!shown.contains("<img"));
    }
}
