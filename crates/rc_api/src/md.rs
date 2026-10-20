// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The parsed markdown (`md`) that the server attaches to a message: a tree of block nodes
//! holding inline nodes. Servers add node types over time and older clients meet them, so reading
//! is lenient. A node that isn't understood becomes `Unknown` with whatever text it carries, a
//! malformed one is read as far as it goes, and none of it fails the message.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        /// 1 to 6.
        level: u8,
        content: Vec<Inline>,
    },
    Code {
        /// `None` when the author gave none; the server says `none` then.
        language: Option<String>,
        lines: Vec<String>,
    },
    Quote(Vec<Block>),
    UnorderedList(Vec<ListItem>),
    OrderedList(Vec<ListItem>),
    Tasks(Vec<Task>),
    /// A message made only of emoji, which the server wants shown large. Holds `Inline::Emoji`.
    BigEmoji(Vec<Inline>),
    /// A formula in display mode (`\[ ... \]`).
    Katex(String),
    /// The server puts one between blocks where the author left a blank line.
    LineBreak,
    Unknown {
        kind: String,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// The number the author wrote, for ordered lists.
    pub number: Option<u32>,
    pub content: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub done: bool,
    pub content: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    /// The server's `BOLD` is what the author wrote with one or two asterisks.
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link {
        url: String,
        /// Is the URL itself for a bare link.
        label: Vec<Inline>,
    },
    Image {
        url: String,
        alt: String,
    },
    /// The username, which may be `all` or `here`.
    MentionUser(String),
    /// The channel name.
    MentionChannel(String),
    /// Either or both of the shortcode (`smile`, without the colons) and the character are set.
    Emoji {
        short_code: Option<String>,
        unicode: Option<String>,
    },
    /// A formula inside a line (`\( ... \)`).
    Katex(String),
    Color {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Timestamp {
        /// Seconds since the epoch, as the author wrote it.
        timestamp: String,
        /// The format letter of `<t:...:R>`.
        format: String,
        /// What the author typed, for a reader that can't format it.
        fallback: String,
    },
    Unknown {
        kind: String,
        text: String,
    },
}

/// Reads the `md` field. `None` if it isn't an array.
pub(crate) fn parse(value: &Value) -> Option<Vec<Block>> {
    Some(value.as_array()?.iter().map(block).collect())
}

fn str_field<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    node.get(name)?.as_str()
}

fn blocks(value: Option<&Value>) -> Vec<Block> {
    value
        .and_then(Value::as_array)
        .map(|nodes| nodes.iter().map(block).collect())
        .unwrap_or_default()
}

fn inlines(value: Option<&Value>) -> Vec<Inline> {
    match value {
        Some(Value::Array(nodes)) => nodes.iter().map(inline).collect(),
        Some(node @ Value::Object(_)) => vec![inline(node)],
        Some(Value::String(text)) => vec![Inline::Text(text.clone())],
        _ => Vec::new(),
    }
}

/// All the text a node carries, however it is nested and whatever it is called.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Array(nodes) => nodes.iter().map(plain).collect(),
        Value::Object(fields) => ["value", "label", "fallback"]
            .into_iter()
            .find_map(|name| fields.get(name))
            .map(plain)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn unknown_block(kind: &str, node: &Value) -> Block {
    Block::Unknown {
        kind: kind.to_owned(),
        text: plain(node),
    }
}

fn block(node: &Value) -> Block {
    let Some(kind) = str_field(node, "type") else {
        return unknown_block("", node);
    };
    let value = node.get("value");
    match kind {
        "PARAGRAPH" => Block::Paragraph(inlines(value)),
        "HEADING" => Block::Heading {
            level: node
                .get("level")
                .and_then(Value::as_u64)
                .map_or(1, |level| u8::try_from(level.clamp(1, 6)).unwrap_or(1)),
            content: inlines(value),
        },
        "CODE" => Block::Code {
            language: str_field(node, "language")
                .filter(|language| !language.is_empty() && *language != "none")
                .map(str::to_owned),
            lines: value
                .and_then(Value::as_array)
                .map(|lines| lines.iter().map(plain).collect())
                .unwrap_or_default(),
        },
        "QUOTE" => Block::Quote(blocks(value)),
        "UNORDERED_LIST" => Block::UnorderedList(list_items(value)),
        "ORDERED_LIST" => Block::OrderedList(list_items(value)),
        "TASKS" => Block::Tasks(
            value
                .and_then(Value::as_array)
                .map(|tasks| {
                    tasks
                        .iter()
                        .map(|task| Task {
                            done: task.get("status").and_then(Value::as_bool).unwrap_or(false),
                            content: inlines(task.get("value")),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "BIG_EMOJI" => Block::BigEmoji(inlines(value)),
        "KATEX" => Block::Katex(value.map(plain).unwrap_or_default()),
        "LINE_BREAK" => Block::LineBreak,
        _ => unknown_block(kind, node),
    }
}

fn channel_value(n: u64) -> u8 {
    u8::try_from(n).unwrap_or(u8::MAX)
}

fn list_items(value: Option<&Value>) -> Vec<ListItem> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| ListItem {
                    number: item
                        .get("number")
                        .and_then(Value::as_u64)
                        .and_then(|n| u32::try_from(n).ok()),
                    content: inlines(item.get("value")),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn inline(node: &Value) -> Inline {
    let Some(kind) = str_field(node, "type") else {
        return Inline::Unknown {
            kind: String::new(),
            text: plain(node),
        };
    };
    let value = node.get("value");
    match kind {
        "PLAIN_TEXT" => Inline::Text(value.map(plain).unwrap_or_default()),
        "BOLD" => Inline::Bold(inlines(value)),
        "ITALIC" => Inline::Italic(inlines(value)),
        "STRIKE" => Inline::Strike(inlines(value)),
        "INLINE_CODE" => Inline::Code(value.map(plain).unwrap_or_default()),
        "LINK" => {
            let url = value
                .and_then(|v| v.get("src"))
                .map(plain)
                .unwrap_or_default();
            let mut label = inlines(value.and_then(|v| v.get("label")));
            if label.is_empty() {
                label.push(Inline::Text(url.clone()));
            }
            Inline::Link { url, label }
        }
        "IMAGE" => Inline::Image {
            url: value
                .and_then(|v| v.get("src"))
                .map(plain)
                .unwrap_or_default(),
            alt: value
                .and_then(|v| v.get("label"))
                .map(plain)
                .unwrap_or_default(),
        },
        "MENTION_USER" => Inline::MentionUser(value.map(plain).unwrap_or_default()),
        "MENTION_CHANNEL" => Inline::MentionChannel(value.map(plain).unwrap_or_default()),
        "EMOJI" => {
            let non_empty = |name| str_field(node, name).filter(|s| !s.is_empty());
            let short_code = non_empty("shortCode")
                .map(str::to_owned)
                .or_else(|| value.map(plain).filter(|s| !s.is_empty()));
            Inline::Emoji {
                short_code,
                unicode: non_empty("unicode").map(str::to_owned),
            }
        }
        "INLINE_KATEX" => Inline::Katex(value.map(plain).unwrap_or_default()),
        "COLOR" => {
            let channel = |name| {
                value
                    .and_then(|v| v.get(name))
                    .and_then(Value::as_u64)
                    .map_or(0, channel_value)
            };
            Inline::Color {
                r: channel("r"),
                g: channel("g"),
                b: channel("b"),
                a: value
                    .and_then(|v| v.get("a"))
                    .and_then(Value::as_u64)
                    .map_or(255, channel_value),
            }
        }
        "TIMESTAMP" => Inline::Timestamp {
            timestamp: value
                .and_then(|v| v.get("timestamp"))
                .map(plain)
                .unwrap_or_default(),
            format: value
                .and_then(|v| v.get("format"))
                .map(plain)
                .unwrap_or_default(),
            fallback: node.get("fallback").map(plain).unwrap_or_default(),
        },
        _ => Inline::Unknown {
            kind: kind.to_owned(),
            text: plain(node),
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_owned())
    }

    fn read(value: &Value) -> Vec<Block> {
        parse(value).unwrap()
    }

    // RICH-001
    #[test]
    fn reads_a_paragraph_with_formatting() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"PLAIN_TEXT","value":"a "},
            {"type":"BOLD","value":[{"type":"PLAIN_TEXT","value":"b"}]},
            {"type":"INLINE_CODE","value":{"type":"PLAIN_TEXT","value":"c"}}
        ]}]);
        assert_eq!(
            read(&md),
            [Block::Paragraph(vec![
                text("a "),
                Inline::Bold(vec![text("b")]),
                Inline::Code("c".into())
            ])]
        );
    }

    // RICH-001
    #[test]
    fn a_code_block_without_a_language_has_none() {
        let md = json!([
            {"type":"CODE","language":"none","value":[{"type":"CODE_LINE","value":{"type":"PLAIN_TEXT","value":"x"}}]},
            {"type":"CODE","language":"rs","value":[]}
        ]);
        assert_eq!(
            read(&md),
            [
                Block::Code {
                    language: None,
                    lines: vec!["x".into()]
                },
                Block::Code {
                    language: Some("rs".into()),
                    lines: vec![]
                }
            ]
        );
    }

    // RICH-001
    #[test]
    fn a_link_label_may_be_a_list_or_missing() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"LINK","value":{"src":{"type":"PLAIN_TEXT","value":"https://a"},"label":[{"type":"PLAIN_TEXT","value":"A"}]}},
            {"type":"LINK","value":{"src":{"type":"PLAIN_TEXT","value":"https://b"},"label":{"type":"PLAIN_TEXT","value":"B"}}},
            {"type":"LINK","value":{"src":{"type":"PLAIN_TEXT","value":"https://c"}}}
        ]}]);
        let [Block::Paragraph(links)] = &read(&md)[..] else {
            panic!()
        };
        let link = |url: &str, label: &str| Inline::Link {
            url: url.into(),
            label: vec![text(label)],
        };
        assert_eq!(
            links,
            &[
                link("https://a", "A"),
                link("https://b", "B"),
                link("https://c", "https://c")
            ]
        );
    }

    // RICH-003
    #[test]
    fn reads_both_kinds_of_emoji() {
        let md = json!([{"type":"PARAGRAPH","value":[
            {"type":"EMOJI","value":{"type":"PLAIN_TEXT","value":"smile"},"shortCode":"smile"},
            {"type":"EMOJI","unicode":"😀"}
        ]}]);
        assert_eq!(
            read(&md),
            [Block::Paragraph(vec![
                Inline::Emoji {
                    short_code: Some("smile".into()),
                    unicode: None
                },
                Inline::Emoji {
                    short_code: None,
                    unicode: Some("😀".into())
                }
            ])]
        );
    }

    // RICH-001
    #[test]
    fn an_unknown_node_keeps_its_text() {
        let md = json!([
            {"type":"FANCY","value":[{"type":"PARAGRAPH","value":[{"type":"PLAIN_TEXT","value":"deep"}]}]},
            {"type":"PARAGRAPH","value":[
                {"type":"SPOILER","value":[{"type":"PLAIN_TEXT","value":"secret"}]},
                {"type":"WEIRD"},
                {"nonsense":1}
            ]}
        ]);
        assert_eq!(
            read(&md),
            [
                Block::Unknown {
                    kind: "FANCY".into(),
                    text: "deep".into()
                },
                Block::Paragraph(vec![
                    Inline::Unknown {
                        kind: "SPOILER".into(),
                        text: "secret".into()
                    },
                    Inline::Unknown {
                        kind: "WEIRD".into(),
                        text: String::new()
                    },
                    Inline::Unknown {
                        kind: String::new(),
                        text: String::new()
                    }
                ])
            ]
        );
    }

    // RICH-001
    #[test]
    fn malformed_nodes_are_read_as_far_as_they_go() {
        let md = json!([
            {"type":"HEADING","level":"x","value":7},
            {"type":"ORDERED_LIST","value":[{"value":[{"type":"PLAIN_TEXT","value":"a"}],"number":-3}]},
            {"type":"TASKS","value":[{"status":"yes"}]},
            {"type":"HEADING","level":99,"value":[]},
            "bare string",
            null
        ]);
        let blocks = read(&md);
        assert_eq!(
            blocks[0],
            Block::Heading {
                level: 1,
                content: vec![]
            }
        );
        assert_eq!(
            blocks[1],
            Block::OrderedList(vec![ListItem {
                number: None,
                content: vec![text("a")]
            }])
        );
        assert_eq!(
            blocks[2],
            Block::Tasks(vec![Task {
                done: false,
                content: vec![]
            }])
        );
        assert!(matches!(blocks[3], Block::Heading { level: 6, .. }));
        assert!(matches!(&blocks[4], Block::Unknown { text, .. } if text == "bare string"));
        assert!(matches!(&blocks[5], Block::Unknown { text, .. } if text.is_empty()));
    }

    // RICH-001
    #[test]
    fn anything_but_an_array_is_not_markdown() {
        assert_eq!(parse(&json!({"type":"PARAGRAPH"})), None);
        assert_eq!(parse(&json!("text")), None);
        assert_eq!(parse(&json!(null)), None);
        assert_eq!(parse(&json!([])), Some(vec![]));
    }
}
