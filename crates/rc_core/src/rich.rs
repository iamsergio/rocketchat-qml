// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The body of a message as blocks of formatted text, for any frontend to render. Built from the
//! server's parsed markdown (`rc_api::md`), or from the plain text when the server sent none.

use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rc_api::md;

/// A message body: the text the author wrote and what it looks like formatted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RichText {
    /// The text as the author wrote it, markdown source included. For previews, notifications and
    /// search.
    pub text: String,
    /// Empty only if the text is.
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        /// 1 to 6.
        level: u8,
        content: Vec<Inline>,
    },
    Code {
        language: Option<String>,
        /// The lines joined by `\n`.
        text: String,
    },
    Quote(Vec<Block>),
    BulletList(Vec<Vec<Inline>>),
    NumberedList {
        /// The number of the first item.
        start: u32,
        items: Vec<Vec<Inline>>,
    },
    TaskList(Vec<TaskItem>),
    /// A message of nothing but emoji, to be shown large.
    BigEmoji(Vec<Emoji>),
    /// A formula in display mode, as TeX.
    Math(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskItem {
    pub done: bool,
    pub content: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link {
        url: String,
        /// Never empty; the URL itself for a bare link.
        label: Vec<Inline>,
    },
    Image {
        url: String,
        alt: String,
    },
    Mention(Mention),
    /// The channel's name, without the `#`.
    ChannelMention(String),
    Emoji(Emoji),
    /// A formula inside a line, as TeX.
    Math(String),
    Color {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Timestamp {
        /// `None` if the number is not a time.
        time: Option<jiff::Timestamp>,
        /// The format letter the author gave (`R` for relative, `f` for full...).
        format: String,
        /// What the author typed; shown when there is no time.
        fallback: String,
    },
    /// Only made for text that has no markup. The server sends one paragraph per line.
    LineBreak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mention {
    /// The username, without the `@`.
    User(String),
    All,
    Here,
}

/// At least one of the two is set. The server sends the character for emoji typed as such, and only
/// the shortcode (`smile`, no colons) for the ones typed by name, which may also be a custom emoji
/// of the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emoji {
    pub short_code: Option<String>,
    pub unicode: Option<String>,
}

impl RichText {
    /// A body that has no markup: one paragraph.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        let blocks = if text.is_empty() {
            Vec::new()
        } else {
            vec![Block::Paragraph(plain_inlines(&text))]
        };
        Self { text, blocks }
    }

    /// The body of a message with that text and parsed markdown. Without markup, or when it
    /// leaves nothing to show, it is the plain text.
    #[must_use]
    pub fn from_markdown(text: String, md: Option<&[md::Block]>) -> Self {
        let blocks: Vec<Block> = md.unwrap_or_default().iter().filter_map(block).collect();
        if blocks.is_empty() {
            return Self::plain(text);
        }
        Self { text, blocks }
    }
}

impl RichText {
    /// The body without the links that start it and stand for the messages it quotes: an
    /// empty-label link (`[ ](url)`, or its `Link` node in the first paragraph) to one of `links`,
    /// with the whitespace after it. The server wants them in the text to make a quote, and a
    /// reader doesn't want to see them. Only links that match are dropped, so one the server
    /// didn't make a quote of stays.
    #[must_use]
    pub fn without_quote_links(self, links: &[String]) -> Self {
        if links.is_empty() {
            return self;
        }
        let text = strip_quote_links_from_text(&self.text, links);
        let changed = text.len() != self.text.len();
        let text = text.to_owned();
        let mut blocks = self.blocks;
        let blocks_changed = strip_quote_links_from_blocks(&mut blocks, links);
        if !changed && !blocks_changed {
            return Self {
                text: self.text,
                blocks,
            };
        }
        // Without markdown the blocks were made from the text, links included.
        if !blocks_changed || blocks.is_empty() || text.is_empty() {
            return Self::plain(text);
        }
        Self { text, blocks }
    }
}

/// The text after the quote links it starts with, or all of it if it doesn't start with one.
fn strip_quote_links_from_text<'a>(text: &'a str, links: &[String]) -> &'a str {
    let mut rest = text;
    let mut stripped = false;
    while let Some(after) = strip_quote_link(rest, links) {
        rest = after.trim_start();
        stripped = true;
    }
    if stripped { rest } else { text }
}

/// What follows a leading `[ ](url)` with a url in `links`.
fn strip_quote_link<'a>(text: &'a str, links: &[String]) -> Option<&'a str> {
    let (label, rest) = text.strip_prefix('[')?.split_once("](")?;
    if !label.trim().is_empty() {
        return None;
    }
    let (url, rest) = rest.split_once(')')?;
    links.iter().any(|link| link == url).then_some(rest)
}

/// Drops the quote links that start the first paragraph, and the paragraph if nothing else is in
/// it. Returns whether there were any.
fn strip_quote_links_from_blocks(blocks: &mut Vec<Block>, links: &[String]) -> bool {
    let Some(Block::Paragraph(inlines)) = blocks.first_mut() else {
        return false;
    };
    let mut dropped = false;
    loop {
        match inlines.first() {
            Some(Inline::Link { url, label })
                if links.contains(url) && label.iter().all(is_blank) =>
            {
                dropped = true;
            }
            Some(inline) if dropped && is_blank(inline) => {}
            _ => break,
        }
        inlines.remove(0);
    }
    if !dropped {
        return false;
    }
    if let Some(Inline::Text(text)) = inlines.first_mut() {
        *text = text.trim_start().to_owned();
    }
    if inlines.is_empty() {
        blocks.remove(0);
    }
    true
}

fn is_blank(inline: &Inline) -> bool {
    matches!(inline, Inline::Text(text) if text.trim().is_empty())
}

impl RichText {
    /// Whether the body calls `username` (`@username`, any case) or everybody (`@all`, `@here`).
    #[must_use]
    pub fn mentions(&self, username: &str) -> bool {
        self.blocks.iter().any(|block| block.mentions(username))
    }
}

impl Block {
    fn mentions(&self, username: &str) -> bool {
        match self {
            Self::Paragraph(content) | Self::Heading { content, .. } => {
                mentions_in(content, username)
            }
            Self::Quote(blocks) => blocks.iter().any(|block| block.mentions(username)),
            Self::BulletList(items) | Self::NumberedList { items, .. } => {
                items.iter().any(|item| mentions_in(item, username))
            }
            Self::TaskList(items) => items
                .iter()
                .any(|item| mentions_in(&item.content, username)),
            Self::Code { .. } | Self::BigEmoji(_) | Self::Math(_) => false,
        }
    }
}

fn mentions_in(inlines: &[Inline], username: &str) -> bool {
    inlines.iter().any(|inline| match inline {
        Inline::Mention(Mention::All | Mention::Here) => true,
        Inline::Mention(Mention::User(name)) => name.eq_ignore_ascii_case(username),
        Inline::Bold(content) | Inline::Italic(content) | Inline::Strike(content) => {
            mentions_in(content, username)
        }
        Inline::Link { label, .. } => mentions_in(label, username),
        _ => false,
    })
}

impl From<String> for RichText {
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

impl From<&str> for RichText {
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

impl RichText {
    /// A room's topic, as the markdown the server stores it in, which it doesn't parse for us:
    /// one paragraph, as the web client shows it in a line. Bold, italic, strikethrough, code and
    /// links are kept, as are bare `http(s)://` and `www.` addresses (made links) and emoji
    /// shortcodes (left to the viewer, who knows the server's custom emoji). Newlines and block
    /// markup (headings, lists, quotes...) end up as spaces and text, images as their description,
    /// and raw HTML is text. There are no mentions.
    #[must_use]
    pub fn from_topic(markdown: &str) -> Self {
        let mut parse = TopicParse::default();
        let options = Options::ENABLE_STRIKETHROUGH;
        for event in Parser::new_ext(markdown, options) {
            parse.event(event);
        }
        let content = parse.finish();
        Self {
            text: markdown.to_owned(),
            blocks: if content.is_empty() {
                Vec::new()
            } else {
                vec![Block::Paragraph(content)]
            },
        }
    }
}

/// What a span of inlines is inside of, until its end event closes it.
enum Open {
    Bold,
    Italic,
    Strike,
    Link(String),
}

/// Turns the events of the markdown of a topic into inlines.
#[derive(Default)]
struct TopicParse {
    /// The inlines of the spans that are open, outermost first; the last one is being filled.
    stack: Vec<(Open, Vec<Inline>)>,
    /// What is closed at the top level.
    done: Vec<Inline>,
    /// Text not yet turned into inlines: the parser splits text at characters that could be
    /// markup, which would cut an address or a shortcode in two.
    pending: String,
    /// A code block: its text isn't searched for addresses.
    in_code_block: bool,
}

impl TopicParse {
    fn event(&mut self, event: Event) {
        match event {
            Event::Text(text) if self.in_code_block => {
                self.flush();
                self.push(Inline::Text(text.replace('\n', " ")));
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                self.pending.push_str(&text.replace('\n', " "));
            }
            Event::SoftBreak | Event::HardBreak => self.pending.push(' '),
            Event::Code(code) => {
                self.flush();
                self.push(Inline::Code(code.into_string()));
            }
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            // Other events (rules, task markers, footnotes...) have no text of their own.
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        let open = match tag {
            Tag::Strong => Open::Bold,
            Tag::Emphasis => Open::Italic,
            Tag::Strikethrough => Open::Strike,
            Tag::Link { dest_url, .. } => Open::Link(dest_url.into_string()),
            Tag::CodeBlock(_) => {
                self.flush();
                self.in_code_block = true;
                return;
            }
            // Everything else shows as the text it holds.
            _ => return,
        };
        self.flush();
        self.stack.push((open, Vec::new()));
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                self.flush();
                let Some((open, mut content)) = self.stack.pop() else {
                    return;
                };
                self.push(match open {
                    Open::Bold => Inline::Bold(content),
                    Open::Italic => Inline::Italic(content),
                    Open::Strike => Inline::Strike(content),
                    Open::Link(url) => {
                        if content.is_empty() {
                            content.push(Inline::Text(url.clone()));
                        }
                        Inline::Link {
                            url,
                            label: content,
                        }
                    }
                });
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.pending.push(' ');
            }
            // Where one block ends, the next one starts after a space.
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::Item
            | TagEnd::List(_) => self.pending.push(' '),
            _ => {}
        }
    }

    /// Adds `inline` to the span that is open, else to the line.
    fn push(&mut self, inline: Inline) {
        if matches!(&inline, Inline::Text(text) if text.is_empty()) {
            return;
        }
        let into = self.stack.last_mut().map_or(&mut self.done, |(_, c)| c);
        match (into.last_mut(), inline) {
            (Some(Inline::Text(last)), Inline::Text(text)) => last.push_str(&text),
            (_, inline) => into.push(inline),
        }
    }

    /// Turns the pending text into text, links and emoji.
    fn flush(&mut self) {
        let text = std::mem::take(&mut self.pending);
        let in_link = self
            .stack
            .iter()
            .any(|(open, _)| matches!(open, Open::Link(_)));
        if in_link {
            self.push_with_emoji(&text);
            return;
        }
        let mut finder = LinkFinder::new();
        finder.kinds(&[LinkKind::Url]).url_must_have_scheme(false);
        for span in finder.spans(&text) {
            let shown = span.as_str();
            let lower = shown.to_ascii_lowercase();
            let url = if span.kind().is_none() {
                None
            } else if lower.starts_with("http://") || lower.starts_with("https://") {
                Some(shown.to_owned())
            } else if lower.starts_with("www.") {
                Some(format!("http://{shown}"))
            } else {
                None
            };
            match url {
                Some(url) => self.push(Inline::Link {
                    url,
                    label: vec![Inline::Text(shown.to_owned())],
                }),
                None => self.push_with_emoji(shown),
            }
        }
    }

    /// Adds `text`, with each `:shortcode:` in it as an emoji.
    fn push_with_emoji(&mut self, text: &str) {
        let mut rest = text;
        while let Some(start) = rest.find(':') {
            let after = &rest[start + 1..];
            let name_len = after
                .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '+' | '-')))
                .unwrap_or(after.len());
            if name_len > 0 && after[name_len..].starts_with(':') {
                self.push(Inline::Text(rest[..start].to_owned()));
                self.push(Inline::Emoji(Emoji {
                    short_code: Some(after[..name_len].to_owned()),
                    unicode: None,
                }));
                rest = &after[name_len + 1..];
            } else {
                // Not a shortcode, but the colon after the name may open one.
                self.push(Inline::Text(rest[..=start].to_owned()));
                rest = after;
            }
        }
        self.push(Inline::Text(rest.to_owned()));
    }

    /// The line, without the spaces the block ends left at its ends or the empty texts.
    fn finish(mut self) -> Vec<Inline> {
        self.flush();
        let mut line = self.done;
        if let Some(Inline::Text(first)) = line.first_mut() {
            *first = first.trim_start().to_owned();
        }
        if let Some(Inline::Text(last)) = line.last_mut() {
            *last = last.trim_end().to_owned();
        }
        line.retain(|inline| !matches!(inline, Inline::Text(text) if text.is_empty()));
        line
    }
}

fn plain_inlines(text: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            inlines.push(Inline::LineBreak);
        }
        if !line.is_empty() {
            inlines.push(Inline::Text(line.to_owned()));
        }
    }
    inlines
}

fn text_block(text: &str) -> Option<Block> {
    (!text.is_empty()).then(|| Block::Paragraph(vec![Inline::Text(text.to_owned())]))
}

/// `None` for what shows nothing: the blank-line marker between blocks, which paragraphs already
/// stand apart for, and an unknown block without text.
fn block(node: &md::Block) -> Option<Block> {
    Some(match node {
        md::Block::Paragraph(content) => Block::Paragraph(inlines(content)),
        md::Block::Heading { level, content } => Block::Heading {
            level: *level,
            content: inlines(content),
        },
        md::Block::Code { language, lines } => Block::Code {
            language: language.clone(),
            text: lines.join("\n"),
        },
        md::Block::Quote(content) => Block::Quote(content.iter().filter_map(block).collect()),
        md::Block::UnorderedList(items) => {
            Block::BulletList(items.iter().map(|item| inlines(&item.content)).collect())
        }
        md::Block::OrderedList(items) => Block::NumberedList {
            start: items.first().and_then(|item| item.number).unwrap_or(1),
            items: items.iter().map(|item| inlines(&item.content)).collect(),
        },
        md::Block::Tasks(tasks) => Block::TaskList(
            tasks
                .iter()
                .map(|task| TaskItem {
                    done: task.done,
                    content: inlines(&task.content),
                })
                .collect(),
        ),
        md::Block::BigEmoji(content) => big_emoji(content),
        md::Block::Katex(tex) => Block::Math(tex.clone()),
        md::Block::LineBreak => return None,
        md::Block::Unknown { kind, text } => {
            tracing::debug!(kind, "markdown block of an unknown type");
            return text_block(text);
        }
    })
}

/// A block of emoji and nothing else; if the server put anything else in it, a paragraph.
fn big_emoji(content: &[md::Inline]) -> Block {
    let emoji: Option<Vec<Emoji>> = content
        .iter()
        .map(|node| match node {
            md::Inline::Emoji {
                short_code,
                unicode,
            } => Some(Emoji {
                short_code: short_code.clone(),
                unicode: unicode.clone(),
            }),
            _ => None,
        })
        .collect();
    match emoji {
        Some(emoji) => Block::BigEmoji(emoji),
        None => Block::Paragraph(inlines(content)),
    }
}

fn inlines(nodes: &[md::Inline]) -> Vec<Inline> {
    nodes.iter().filter_map(inline).collect()
}

fn inline(node: &md::Inline) -> Option<Inline> {
    Some(match node {
        md::Inline::Text(text) => Inline::Text(text.clone()),
        md::Inline::Bold(content) => Inline::Bold(inlines(content)),
        md::Inline::Italic(content) => Inline::Italic(inlines(content)),
        md::Inline::Strike(content) => Inline::Strike(inlines(content)),
        md::Inline::Code(text) => Inline::Code(text.clone()),
        md::Inline::Link { url, label } => {
            let mut label = inlines(label);
            if label.is_empty() {
                label.push(Inline::Text(url.clone()));
            }
            Inline::Link {
                url: url.clone(),
                label,
            }
        }
        md::Inline::Image { url, alt } => Inline::Image {
            url: url.clone(),
            alt: alt.clone(),
        },
        md::Inline::MentionUser(name) => Inline::Mention(match name.as_str() {
            "all" => Mention::All,
            "here" => Mention::Here,
            _ => Mention::User(name.clone()),
        }),
        md::Inline::MentionChannel(name) => Inline::ChannelMention(name.clone()),
        md::Inline::Emoji {
            short_code,
            unicode,
        } => Inline::Emoji(Emoji {
            short_code: short_code.clone(),
            unicode: unicode.clone(),
        }),
        md::Inline::Katex(tex) => Inline::Math(tex.clone()),
        md::Inline::Color { r, g, b, a } => Inline::Color {
            r: *r,
            g: *g,
            b: *b,
            a: *a,
        },
        md::Inline::Timestamp {
            timestamp,
            format,
            fallback,
        } => Inline::Timestamp {
            time: timestamp
                .parse()
                .ok()
                .and_then(|seconds| jiff::Timestamp::from_second(seconds).ok()),
            format: format.clone(),
            fallback: fallback.clone(),
        },
        md::Inline::Unknown { kind, text } => {
            tracing::debug!(kind, "markdown inline of an unknown type");
            return (!text.is_empty()).then(|| Inline::Text(text.clone()));
        }
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const HISTORY_RICH: &str = include_str!("../../rc_api/tests/fixtures/history_rich.json");

    /// The body of the fixture's message with that text, as the server parsed it.
    fn rich(text: &str) -> RichText {
        let history: Value = serde_json::from_str(HISTORY_RICH).unwrap();
        let message = history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["msg"] == text)
            .unwrap_or_else(|| panic!("no message {text:?} in the fixture"));
        from_json(text, &message["md"])
    }

    fn from_json(text: &str, md: &Value) -> RichText {
        RichText::from_markdown(text.to_owned(), md::parse(md).as_deref())
    }

    fn t(text: &str) -> Inline {
        Inline::Text(text.to_owned())
    }

    fn para(inlines: Vec<Inline>) -> Block {
        Block::Paragraph(inlines)
    }

    fn body(blocks: Vec<Block>) -> RichText {
        RichText {
            text: String::new(),
            blocks,
        }
    }

    fn user(name: &str) -> Inline {
        Inline::Mention(Mention::User(name.to_owned()))
    }

    // RICH-002
    #[test]
    fn a_mention_of_the_user_is_found_in_any_case() {
        let text = body(vec![para(vec![t("hi "), user("Alice")])]);
        assert!(text.mentions("alice"));
        assert!(!text.mentions("bob"));
    }

    // RICH-002
    #[test]
    fn all_and_here_mention_everybody() {
        for mention in [Mention::All, Mention::Here] {
            let text = body(vec![para(vec![Inline::Mention(mention)])]);
            assert!(text.mentions("anybody"));
        }
    }

    // RICH-002
    #[test]
    fn a_mention_is_found_inside_nested_blocks_and_formatting() {
        let nested = [
            Block::Quote(vec![para(vec![Inline::Bold(vec![user("me")])])]),
            Block::BulletList(vec![vec![Inline::Italic(vec![user("me")])]]),
            Block::NumberedList {
                start: 1,
                items: vec![vec![t("x")], vec![user("me")]],
            },
            Block::TaskList(vec![TaskItem {
                done: false,
                content: vec![user("me")],
            }]),
            Block::Heading {
                level: 2,
                content: vec![Inline::Strike(vec![user("me")])],
            },
            para(vec![Inline::Link {
                url: "https://example.com".into(),
                label: vec![user("me")],
            }]),
        ];
        for block in nested {
            assert!(body(vec![block.clone()]).mentions("me"), "{block:?}");
        }
    }

    // RICH-002
    #[test]
    fn code_and_channels_mention_nobody() {
        let text = body(vec![
            Block::Code {
                language: None,
                text: "@me".into(),
            },
            para(vec![
                Inline::Code("@me".into()),
                Inline::ChannelMention("me".into()),
                t("@me"),
            ]),
        ]);
        assert!(!text.mentions("me"));
    }

    // RICH-002
    #[test]
    fn the_mentions_of_the_server_are_found() {
        let text = rich("hello @admin and @all and @here");
        assert!(text.mentions("admin"));
        assert!(text.mentions("nobody"));
    }

    // RICH-001
    #[test]
    fn plain_text_is_one_paragraph() {
        let body = RichText::plain("hello");
        assert_eq!(body.text, "hello");
        assert_eq!(body.blocks, [para(vec![t("hello")])]);
    }

    // RICH-001
    #[test]
    fn plain_text_keeps_its_lines_and_blanks() {
        assert_eq!(
            RichText::plain("a\n\nb\n").blocks,
            [para(vec![
                t("a"),
                Inline::LineBreak,
                Inline::LineBreak,
                t("b"),
                Inline::LineBreak
            ])]
        );
    }

    // RICH-001
    #[test]
    fn empty_text_has_no_blocks() {
        assert_eq!(RichText::plain("").blocks, []);
        assert_eq!(RichText::from_markdown(String::new(), Some(&[])).blocks, []);
    }

    // RICH-001
    #[test]
    fn without_markdown_the_text_is_shown() {
        let expected = RichText::plain("*hi* there");
        assert_eq!(RichText::from_markdown("*hi* there".into(), None), expected);
        assert_eq!(
            RichText::from_markdown("*hi* there".into(), Some(&[])),
            expected
        );
        assert_eq!(from_json("*hi* there", &json!("garbage")), expected);
        assert_eq!(
            from_json("*hi* there", &json!([{"type":"LINE_BREAK"}])),
            expected
        );
        assert_eq!(
            from_json("*hi* there", &json!([{"type":"X","value":[]}])),
            expected
        );
    }

    // RICH-001
    #[test]
    fn a_paragraph_keeps_the_text_the_author_wrote() {
        let body = rich("plain paragraph");
        assert_eq!(body.text, "plain paragraph");
        assert_eq!(body.blocks, [para(vec![t("plain paragraph")])]);
    }

    // RICH-001
    #[test]
    fn blank_lines_between_blocks_are_dropped() {
        assert_eq!(
            rich("two\n\nparagraphs").blocks,
            [para(vec![t("two")]), para(vec![t("paragraphs")])]
        );
        let body = rich("hello\n```\ncode\n```\nbye");
        assert_eq!(
            body.blocks,
            [
                para(vec![t("hello")]),
                Block::Code {
                    language: None,
                    text: "code".into()
                },
                para(vec![t("bye")])
            ]
        );
    }

    // RICH-001
    #[test]
    fn formatting_and_code() {
        assert_eq!(
            rich("**bold** *italic* ~strike~ _it2_ __b2__ ~~st2~~").blocks,
            [para(vec![
                Inline::Bold(vec![t("bold")]),
                t(" "),
                Inline::Bold(vec![t("italic")]),
                t(" "),
                Inline::Strike(vec![t("strike")]),
                t(" "),
                Inline::Italic(vec![t("it2")]),
                t(" "),
                Inline::Italic(vec![t("b2")]),
                t(" "),
                Inline::Strike(vec![t("st2")]),
            ])]
        );
        assert_eq!(
            rich("`inline code` here").blocks,
            [para(vec![Inline::Code("inline code".into()), t(" here")])]
        );
        assert_eq!(
            rich("```js\nconsole.log(1)\n```").blocks,
            [Block::Code {
                language: Some("js".into()),
                text: "console.log(1)".into()
            }]
        );
        assert_eq!(
            rich("```\nno lang\nblock\n```").blocks,
            [Block::Code {
                language: None,
                text: "no lang\nblock".into()
            }]
        );
    }

    // RICH-001
    #[test]
    fn nested_formatting_keeps_its_nesting() {
        let body = rich("mixed **bold [link](https://example.com) text**");
        assert_eq!(
            body.blocks,
            [para(vec![
                t("mixed "),
                Inline::Bold(vec![
                    t("bold "),
                    Inline::Link {
                        url: "https://example.com".into(),
                        label: vec![t("link")]
                    },
                    t(" text")
                ])
            ])]
        );
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"ITALIC","value":[{"type":"BOLD","value":[
                {"type":"STRIKE","value":[{"type":"PLAIN_TEXT","value":"deep"}]}
            ]}]}
        ]}]);
        assert_eq!(
            from_json("x", &md).blocks,
            [para(vec![Inline::Italic(vec![Inline::Bold(vec![
                Inline::Strike(vec![t("deep")])
            ])])])]
        );
    }

    // RICH-001
    #[test]
    fn links_images_and_headings() {
        assert_eq!(
            rich("https://example.com/a?b=1").blocks,
            [para(vec![Inline::Link {
                url: "https://example.com/a?b=1".into(),
                label: vec![t("https://example.com/a?b=1")]
            }])]
        );
        assert_eq!(
            rich("![alt](https://example.com/i.png)").blocks,
            [para(vec![Inline::Image {
                url: "https://example.com/i.png".into(),
                alt: "alt".into()
            }])]
        );
        let body = rich("# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6");
        let levels: Vec<_> = body
            .blocks
            .iter()
            .map(|b| match b {
                Block::Heading { level, .. } => *level,
                Block::Paragraph(_) => 0,
                other => panic!("{other:?}"),
            })
            .collect();
        // The server's parser stops at level four; the rest are paragraphs.
        assert_eq!(levels, [1, 2, 3, 4, 0, 0]);
    }

    // RICH-001
    #[test]
    fn quotes_hold_blocks() {
        assert_eq!(
            rich("> quote **bold**\n> - item").blocks,
            [Block::Quote(vec![
                para(vec![t("quote "), Inline::Bold(vec![t("bold")])]),
                para(vec![t("- item")])
            ])]
        );
        assert_eq!(
            rich("> quote\n\nafter").blocks,
            [
                Block::Quote(vec![para(vec![t("quote")])]),
                para(vec![t("after")])
            ]
        );
    }

    // RICH-001
    #[test]
    fn lists() {
        assert_eq!(
            rich("1. one\n2. two\n3. three").blocks,
            [Block::NumberedList {
                start: 1,
                items: vec![vec![t("one")], vec![t("two")], vec![t("three")]]
            }]
        );
        assert_eq!(
            rich("3. starts at three\n4. four").blocks,
            [Block::NumberedList {
                start: 3,
                items: vec![vec![t("starts at three")], vec![t("four")]]
            }]
        );
        assert_eq!(
            rich("- [ ] todo\n- [x] done").blocks,
            [Block::TaskList(vec![
                TaskItem {
                    done: false,
                    content: vec![t("todo")]
                },
                TaskItem {
                    done: true,
                    content: vec![t("done")]
                }
            ])]
        );
        // The server doesn't nest: the indented item comes out as text between two lists.
        assert_eq!(
            rich("- a\n- b\n  - nested\n- c").blocks,
            [
                Block::BulletList(vec![vec![t("a")], vec![t("b")]]),
                para(vec![t("  - nested")]),
                Block::BulletList(vec![vec![t("c")]])
            ]
        );
    }

    // RICH-002
    #[test]
    fn mentions() {
        assert_eq!(
            rich("hello @admin and @all and @here").blocks,
            [para(vec![
                t("hello "),
                Inline::Mention(Mention::User("admin".into())),
                t(" and "),
                Inline::Mention(Mention::All),
                t(" and "),
                Inline::Mention(Mention::Here),
            ])]
        );
        assert_eq!(
            rich("see #general").blocks,
            [para(vec![
                t("see "),
                Inline::ChannelMention("general".into())
            ])]
        );
    }

    // RICH-003
    #[test]
    fn emoji() {
        let short = |name: &str| Emoji {
            short_code: Some(name.to_owned()),
            unicode: None,
        };
        let unicode = |c: &str| Emoji {
            short_code: None,
            unicode: Some(c.to_owned()),
        };
        assert_eq!(
            rich("use :smile: and :+1: here").blocks,
            [para(vec![
                t("use "),
                Inline::Emoji(short("smile")),
                t(" and "),
                Inline::Emoji(short("+1")),
                t(" here")
            ])]
        );
        assert_eq!(
            rich("unicode 😀 emoji 👍").blocks,
            [para(vec![
                t("unicode "),
                Inline::Emoji(unicode("😀")),
                t(" emoji "),
                Inline::Emoji(unicode("👍"))
            ])]
        );
        assert_eq!(rich("😀").blocks, [Block::BigEmoji(vec![unicode("😀")])]);
        assert_eq!(
            rich("😀😀 :smile:").blocks,
            [Block::BigEmoji(vec![
                unicode("😀"),
                unicode("😀"),
                short("smile")
            ])]
        );
        assert_eq!(
            rich(":smile:").blocks,
            [Block::BigEmoji(vec![short("smile")])]
        );
    }

    // RICH-001
    #[test]
    fn big_emoji_with_anything_else_is_a_paragraph() {
        let md = json!([{"type":"BIG_EMOJI","value":[
            {"type":"EMOJI","unicode":"😀"},{"type":"PLAIN_TEXT","value":"x"}
        ]}]);
        assert_eq!(
            from_json("😀x", &md).blocks,
            [para(vec![
                Inline::Emoji(Emoji {
                    short_code: None,
                    unicode: Some("😀".into())
                }),
                t("x")
            ])]
        );
    }

    // RICH-001
    #[test]
    fn math_color_and_timestamps() {
        assert_eq!(
            rich("\\[\\frac{1}{2}\\]").blocks,
            [Block::Math("\\frac{1}{2}".into())]
        );
        assert_eq!(
            rich("$$x^2 + y^2$$ and \\(a+b\\)").blocks,
            [para(vec![
                t("$$x^2 + y^2$$ and "),
                Inline::Math("a+b".into())
            ])]
        );
        assert_eq!(
            rich("color:#ff0000").blocks,
            [para(vec![Inline::Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }])]
        );
        let time = Some("2025-01-01T00:00:00Z".parse().unwrap());
        assert_eq!(
            rich("<t:1735689600:R> <t:1735689600:f>").blocks,
            [para(vec![
                Inline::Timestamp {
                    time,
                    format: "R".into(),
                    fallback: "<t:1735689600:R>".into()
                },
                t(" "),
                Inline::Timestamp {
                    time,
                    format: "f".into(),
                    fallback: "<t:1735689600:f>".into()
                }
            ])]
        );
    }

    // RICH-001
    #[test]
    fn a_timestamp_that_is_no_time_has_none() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"TIMESTAMP","value":{"timestamp":"soon","format":"R"},
             "fallback":{"type":"PLAIN_TEXT","value":"<t:soon:R>"}},
            {"type":"TIMESTAMP","value":{"timestamp":"99999999999999999","format":"R"}}
        ]}]);
        let [Block::Paragraph(inlines)] = &from_json("x", &md).blocks[..] else {
            panic!()
        };
        assert!(matches!(&inlines[0], Inline::Timestamp { time: None, .. }));
        assert!(matches!(&inlines[1], Inline::Timestamp { time: None, .. }));
    }

    // RICH-001
    #[test]
    fn unknown_nodes_become_their_text() {
        let md = json!([
            {"type":"FANCY","value":[{"type":"PARAGRAPH","value":[{"type":"PLAIN_TEXT","value":"deep"}]}]},
            {"type":"EMPTY_THING"},
            {"type":"PARAGRAPH","value":[
                t_json("a "),
                {"type":"SPOILER","value":[{"type":"PLAIN_TEXT","value":"secret"}]},
                {"type":"MYSTERY"},
                t_json(" b")
            ]}
        ]);
        assert_eq!(
            from_json("x", &md).blocks,
            [
                para(vec![t("deep")]),
                para(vec![t("a "), t("secret"), t(" b")])
            ]
        );
    }

    fn t_json(text: &str) -> Value {
        json!({"type":"PLAIN_TEXT","value":text})
    }

    // RICH-001
    #[test]
    fn a_link_without_a_label_shows_its_url() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"LINK","value":{"src":{"type":"PLAIN_TEXT","value":"https://a"},"label":[]}}
        ]}]);
        assert_eq!(
            from_json("x", &md).blocks,
            [para(vec![Inline::Link {
                url: "https://a".into(),
                label: vec![t("https://a")]
            }])]
        );
    }

    // RICH-001
    #[test]
    fn every_message_of_the_fixture_maps_to_blocks() {
        let history: Value = serde_json::from_str(HISTORY_RICH).unwrap();
        for message in history["messages"].as_array().unwrap() {
            let text = message["msg"].as_str().unwrap();
            let body = rich(text);
            assert_eq!(body.text, text);
            assert!(!body.blocks.is_empty(), "{text:?}");
        }
    }

    fn quote_link(url: &str) -> Inline {
        Inline::Link {
            url: url.into(),
            label: vec![t(" ")],
        }
    }

    fn links(urls: &[&str]) -> Vec<String> {
        urls.iter().map(|&url| url.to_owned()).collect()
    }

    // ACT-005
    #[test]
    fn the_quote_link_of_a_recorded_message_leaves_its_text_and_paragraph() {
        let history: Value = serde_json::from_str(include_str!(
            "../../rc_api/tests/fixtures/history_quotes.json"
        ))
        .unwrap();
        let message = history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| {
                m["msg"]
                    .as_str()
                    .is_some_and(|text| text.ends_with(") first reply"))
            })
            .unwrap();
        let text = message["msg"].as_str().unwrap();
        let link = message["attachments"][0]["message_link"].as_str().unwrap();
        let body = from_json(text, &message["md"]);
        assert_eq!(body.blocks.len(), 1);

        let stripped = body.without_quote_links(&links(&[link]));

        assert_eq!(stripped.text, "first reply");
        assert_eq!(stripped.blocks, [para(vec![t("first reply")])]);
    }

    // ACT-005
    #[test]
    fn several_quote_links_go_with_the_whitespace_between_them() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"LINK","value":{"src":t_json("http://s/a"),"label":[t_json(" ")]}},
            t_json(" "),
            {"type":"LINK","value":{"src":t_json("http://s/b"),"label":[t_json(" ")]}},
            t_json("  hello"),
            {"type":"BOLD","value":[t_json("there")]}
        ]}]);
        let body = from_json("[ ](http://s/a) [ ](http://s/b)  hello**there**", &md);

        let stripped = body.without_quote_links(&links(&["http://s/a", "http://s/b"]));

        assert_eq!(stripped.text, "hello**there**");
        assert_eq!(
            stripped.blocks,
            [para(vec![t("hello"), Inline::Bold(vec![t("there")])])]
        );
    }

    // ACT-005
    #[test]
    fn a_link_that_is_not_a_quote_stays() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"LINK","value":{"src":t_json("http://s/other"),"label":[t_json(" ")]}},
            t_json(" hello")
        ]}]);
        let text = "[ ](http://s/other) hello";
        let body = from_json(text, &md);

        let stripped = body.clone().without_quote_links(&links(&["http://s/a"]));

        assert_eq!(stripped, body);
        assert_eq!(body.clone().without_quote_links(&[]), body);
    }

    // ACT-005
    #[test]
    fn a_link_with_a_label_or_not_at_the_start_stays() {
        let labelled = RichText {
            text: "[see](http://s/a) hi".into(),
            blocks: vec![para(vec![
                Inline::Link {
                    url: "http://s/a".into(),
                    label: vec![t("see")],
                },
                t(" hi"),
            ])],
        };
        assert_eq!(
            labelled
                .clone()
                .without_quote_links(&links(&["http://s/a"])),
            labelled
        );
        let later = RichText {
            text: "hi [ ](http://s/a)".into(),
            blocks: vec![para(vec![t("hi "), quote_link("http://s/a")])],
        };
        assert_eq!(
            later.clone().without_quote_links(&links(&["http://s/a"])),
            later
        );
    }

    // ACT-005
    #[test]
    fn a_body_of_only_quote_links_ends_up_empty() {
        let body = RichText {
            text: "[ ](http://s/a) ".into(),
            blocks: vec![para(vec![quote_link("http://s/a"), t(" ")])],
        };

        let stripped = body.without_quote_links(&links(&["http://s/a"]));

        assert_eq!(stripped, RichText::plain(""));
        assert!(stripped.blocks.is_empty());
    }

    // ACT-005
    #[test]
    fn a_body_without_markdown_loses_the_link_from_its_text_too() {
        let body = RichText::plain("[ ](http://s/a) hello\nthere");

        let stripped = body.without_quote_links(&links(&["http://s/a"]));

        assert_eq!(stripped, RichText::plain("hello\nthere"));
    }

    // ACT-005
    #[test]
    fn later_paragraphs_keep_their_links() {
        let body = RichText {
            text: "[ ](http://s/a) one\n[ ](http://s/a) two".into(),
            blocks: vec![
                para(vec![quote_link("http://s/a"), t(" one")]),
                para(vec![quote_link("http://s/a"), t(" two")]),
            ],
        };

        let stripped = body.without_quote_links(&links(&["http://s/a"]));

        assert_eq!(stripped.text, "one\n[ ](http://s/a) two");
        assert_eq!(stripped.blocks[0], para(vec![t("one")]));
        assert_eq!(stripped.blocks.len(), 2);
    }

    fn topic(markdown: &str) -> Vec<Inline> {
        match RichText::from_topic(markdown).blocks.as_slice() {
            [Block::Paragraph(content)] => content.clone(),
            [] => Vec::new(),
            other => panic!("{other:?}"),
        }
    }

    fn text(text: &str) -> Inline {
        Inline::Text(text.to_owned())
    }

    fn link(url: &str, label: &str) -> Inline {
        Inline::Link {
            url: url.to_owned(),
            label: vec![text(label)],
        }
    }

    // ROOM-009
    #[test]
    fn a_topic_keeps_bold_italic_strikethrough_and_code() {
        assert_eq!(
            topic("**a** __b__ *c* _d_ ~~e~~ ~f~ `g`"),
            [
                Inline::Bold(vec![text("a")]),
                text(" "),
                Inline::Bold(vec![text("b")]),
                text(" "),
                Inline::Italic(vec![text("c")]),
                text(" "),
                Inline::Italic(vec![text("d")]),
                text(" "),
                Inline::Strike(vec![text("e")]),
                text(" "),
                Inline::Strike(vec![text("f")]),
                text(" "),
                Inline::Code("g".into()),
            ]
        );
        assert_eq!(
            topic("**bold and _both_**"),
            [Inline::Bold(vec![
                text("bold and "),
                Inline::Italic(vec![text("both")])
            ])]
        );
    }

    // ROOM-009
    #[test]
    fn a_topic_keeps_links_and_makes_bare_addresses_links() {
        assert_eq!(
            topic("[the plan](https://example.com/plan)"),
            [link("https://example.com/plan", "the plan")]
        );
        assert_eq!(
            topic("see https://example.com/a_b_c, or www.example.org."),
            [
                text("see "),
                link("https://example.com/a_b_c", "https://example.com/a_b_c"),
                text(", or "),
                link("http://www.example.org", "www.example.org"),
                text("."),
            ]
        );
        // A word that is not an address stays text, and so does an address in a link's label.
        assert_eq!(topic("notes.txt"), [text("notes.txt")]);
        assert_eq!(
            topic("[https://a.example](https://b.example)"),
            [link("https://b.example", "https://a.example")]
        );
        assert_eq!(
            topic("[ ](https://b.example)"),
            [link("https://b.example", " ")]
        );
    }

    // ROOM-009
    #[test]
    fn a_topic_leaves_emoji_shortcodes_to_the_viewer() {
        let emoji = |name: &str| {
            Inline::Emoji(Emoji {
                short_code: Some(name.to_owned()),
                unicode: None,
            })
        };
        assert_eq!(
            topic("Go :rocket: now :+1::smile: at 10:30 :"),
            [
                text("Go "),
                emoji("rocket"),
                text(" now "),
                emoji("+1"),
                emoji("smile"),
                text(" at 10:30 :"),
            ]
        );
        // Inside code they are what was typed.
        assert_eq!(topic("`:smile:`"), [Inline::Code(":smile:".into())]);
    }

    // ROOM-009
    #[test]
    fn raw_html_in_a_topic_is_text() {
        assert_eq!(
            topic("a <b>bold</b> <script>x</script>"),
            [text("a <b>bold</b> <script>x</script>")]
        );
        assert_eq!(topic("<div>\nblock\n</div>"), [text("<div> block </div>")]);
    }

    // ROOM-009
    #[test]
    fn a_topic_is_one_line() {
        assert_eq!(
            topic("one\ntwo  \nthree\n\nfour"),
            [text("one two three four")]
        );
        assert_eq!(topic("  padded \n"), [text("padded")]);
        assert!(topic("").is_empty());
        assert!(topic(" \n ").is_empty());
        assert!(RichText::from_topic("").blocks.is_empty());
        assert_eq!(RichText::from_topic("**a**").text, "**a**");
    }

    // ROOM-009
    #[test]
    fn what_a_topic_cannot_show_degrades_to_its_text() {
        assert_eq!(topic("# Title\nbody"), [text("Title body")]);
        assert_eq!(topic("- one\n- two"), [text("one two")]);
        assert_eq!(topic("1. one\n2. two"), [text("one two")]);
        assert_eq!(topic("> quoted"), [text("quoted")]);
        assert_eq!(topic("```\ncode here\n```"), [text("code here")]);
        assert_eq!(
            topic("![a cat](https://example.com/cat.png)"),
            [text("a cat")]
        );
        assert_eq!(topic("@alice in #general"), [text("@alice in #general")]);
        assert_eq!(topic("a \\*b\\* &amp; c"), [text("a *b* & c")]);
    }
}
