// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The channel the scenarios scroll: fixed content, posted once per invocation of the runner and
//! shared by both apps and every run.
//!
//! The content is a function of the message's index only, so two invocations differ in ids,
//! timestamps and the unique names the server needs, nothing else.

use rc_test_server::{Admin, TestUser, unique};

/// Messages in the channel's main list. Thread replies come on top and are not in the list.
pub const MESSAGES: usize = 200;
/// How many kinds of message there are, which cycle: see [`kind`].
const KINDS: usize = 10;
/// The custom emoji of the content, a fixed name so that the server keeps one across invocations.
pub const CUSTOM_EMOJI: &str = "perf_party";

/// What was posted, and what the scenarios need to find it again.
#[derive(Debug)]
pub struct Channel {
    /// The user the app logs in as.
    pub viewer: TestUser,
    pub id: String,
    pub oldest_id: String,
    pub newest_id: String,
    /// How many messages the list shows when everything is loaded.
    pub messages: usize,
    /// How many of them have an image.
    pub images: usize,
}

/// The kinds of message, cycling by index.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Kind {
    Plain,
    Markdown,
    CodeBlock,
    Emoji,
    Reactions,
    ThreadParent,
    Image,
    List,
    Long,
    Mixed,
}

#[must_use]
pub fn kind(index: usize) -> Kind {
    match index % KINDS {
        0 => Kind::Plain,
        1 => Kind::Markdown,
        2 => Kind::CodeBlock,
        3 => Kind::Emoji,
        4 => Kind::Reactions,
        5 => Kind::ThreadParent,
        6 => Kind::Image,
        7 => Kind::List,
        8 => Kind::Long,
        _ => Kind::Mixed,
    }
}

/// The text of message `index`.
#[must_use]
pub fn text(index: usize) -> String {
    match kind(index) {
        Kind::Plain => format!(
            "Plain message {index}: the quick brown fox jumps over the lazy dog, and then does it \
             again so that the line wraps."
        ),
        Kind::Markdown => format!(
            "**Bold {index}**, _italic_, ~~struck~~, `inline code` and a [link](https://example.com/perf/{index})."
        ),
        Kind::CodeBlock => {
            format!("Snippet {index}:\n```\nfn answer() -> u32 {{\n    42 + {index}\n}}\n```")
        }
        Kind::Emoji => format!("Emoji {index} :tada: :+1: :smile: :{CUSTOM_EMOJI}: done"),
        Kind::Reactions => format!("Reacted message {index}: shipped"),
        Kind::ThreadParent => format!("Thread {index}: what do you think about this?"),
        Kind::Image => format!("Image {index}"),
        Kind::List => format!("> A quoted line for {index}\n\n- one\n- two\n- three"),
        Kind::Long => format!(
            "Long message {index}. First the setup, with enough words to fill a line and spill \
             over onto the next.\nThen a second paragraph that says something else entirely, and \
             a third sentence to be sure the row is tall."
        ),
        Kind::Mixed => {
            format!("Mixed {index}: see https://example.com/perf/{index} :rocket: and **more**")
        }
    }
}

/// The text of reply `n` (0 or 1) in the thread of message `index`.
#[must_use]
pub fn reply_text(index: usize, n: usize) -> String {
    format!("Reply {} to {index}: sounds good to me", n + 1)
}

/// The reactions of message `index` as (emoji, poster numbers who react); empty for kinds without.
#[must_use]
pub fn reactions(index: usize) -> Vec<(&'static str, Vec<usize>)> {
    match kind(index) {
        Kind::Reactions => vec![
            (":+1:", vec![0]),
            (":tada:", vec![1, 2]),
            (":perf_party:", vec![0]),
        ],
        Kind::Mixed => vec![(":rocket:", vec![1])],
        _ => Vec::new(),
    }
}

/// A PNG of `width` x `height`, a gradient with diagonal stripes whose colours depend on `index`:
/// the same bytes for the same index, a different picture for each message.
///
/// # Errors
///
/// If the encoder fails, which it can't for a buffer of the right size.
pub fn image(index: usize, width: u32, height: u32) -> Result<Vec<u8>, png::EncodingError> {
    let mut data = Vec::with_capacity(width as usize * height as usize * 3);
    let shift = u32::try_from(index % 256).unwrap_or(0);
    for y in 0..height {
        for x in 0..width {
            let stripe = if (x + y) / 24 % 2 == 0 { 0 } else { 48 };
            data.push(
                u8::try_from((x * 255 / width + shift) % 256)
                    .unwrap_or(0)
                    .saturating_add(stripe),
            );
            data.push(u8::try_from((y * 255 / height + shift * 3) % 256).unwrap_or(0));
            data.push(u8::try_from((x + y + shift * 7) % 256).unwrap_or(0));
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&data)?;
    Ok(bytes)
}

/// Posts the channel for the scenarios on the main channel: a viewer for the app and three authors with
/// avatars, who are all in a new channel.
///
/// # Panics
///
/// If the server refuses a call (the support crate's calls do).
pub fn main_channel(log: impl Fn(&str)) -> Result<Channel, png::EncodingError> {
    let admin = Admin::login();
    let viewer = admin.create_user_named("Perf Viewer");
    let authors: Vec<TestUser> = ["Perf Alice", "Perf Bob", "Perf Carol"]
        .iter()
        .map(|name| admin.create_user_named(name))
        .collect();
    let clients: Vec<Admin> = authors
        .iter()
        .map(|user| Admin::login_as(&user.username, &user.password))
        .collect();
    for author in &authors {
        admin.set_avatar(&author.username);
    }
    admin.create_emoji_if_missing(CUSTOM_EMOJI);
    let mut members = vec![viewer.username.as_str()];
    members.extend(authors.iter().map(|author| author.username.as_str()));
    let (id, _) = admin.create_channel_named(&format!("perf-scroll-{}", unique()), &members);

    let mut first = None;
    let mut last = None;
    let mut images = 0;
    for index in 0..MESSAGES {
        let author = &clients[index % clients.len()];
        let posted = if kind(index) == Kind::Image {
            images += 1;
            author.upload_file(
                &id,
                &format!("perf-{index}.png"),
                "image/png",
                &image(index, 480, 300)?,
                &text(index),
                "",
            )
        } else {
            author.post_message(&id, &text(index))
        };
        for (emoji, who) in reactions(index) {
            for number in who {
                clients[number].react(&posted.id, emoji, true);
            }
        }
        if kind(index) == Kind::ThreadParent {
            for n in 0..2 {
                let replier = &clients[(index + 1 + n) % clients.len()];
                replier.post_in_thread(&id, &posted.id, &reply_text(index, n), false);
            }
        }
        if index % 25 == 24 {
            log(&format!("seeded {} of {MESSAGES} messages", index + 1));
        }
        first.get_or_insert_with(|| posted.id.clone());
        last = Some(posted.id);
    }
    Ok(Channel {
        viewer,
        id,
        oldest_id: first.unwrap_or_default(),
        newest_id: last.unwrap_or_default(),
        messages: MESSAGES,
        images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-007
    #[test]
    fn the_content_depends_on_the_index_only() {
        for index in 0..MESSAGES {
            assert_eq!(text(index), text(index));
        }
        assert_ne!(text(0), text(10));
        assert_eq!(image(7, 48, 30).unwrap(), image(7, 48, 30).unwrap());
        assert_ne!(image(7, 48, 30).unwrap(), image(8, 48, 30).unwrap());
    }

    // INFRA-007
    #[test]
    fn the_kinds_cycle_and_every_one_is_in_the_channel() {
        let kinds: std::collections::BTreeSet<_> = (0..MESSAGES)
            .map(|index| format!("{:?}", kind(index)))
            .collect();
        assert_eq!(kinds.len(), KINDS);
        assert_eq!(kind(0), kind(KINDS));
        assert!(text(3).contains(CUSTOM_EMOJI));
        assert!(!reactions(4).is_empty());
        assert!(text(2).contains("```"));
    }

    // INFRA-007
    #[test]
    fn an_image_is_a_png() {
        let bytes = image(0, 16, 16).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }
}
