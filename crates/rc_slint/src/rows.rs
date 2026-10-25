// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The rows the GUI shows, from what `rc_view` says: a thin mapping to the Slint structs, with no
//! decisions of its own.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::{Message, MessageKind, Room};
use rc_core::policy::can_react;
use rc_view::accounts::{AccountView, Status};
use rc_view::attachment_view::attachments;
use rc_view::avatar_view::{AvatarUrls, color, initials, message_owner, room_placeholder};
use rc_view::file_urls::file_path;
use rc_view::message_list::MessageList;
use rc_view::message_view::{
    author_name, body, can_delete, can_edit, can_quote, can_start_thread, day_label, delivery_name,
    in_thread_shown, mentions_me, system_text, thread_id, thread_last_reply_text, thread_replies,
    time_text,
};
use rc_view::quote_view::quotes;
use rc_view::reaction_view::reactions;
use rc_view::rich_html::{message_html, render};
use rc_view::room_view::kind_name;
use rc_view::thread_list_view::{row_summary, row_text};
use rc_view::viewer::{Clock, Viewer};
use slint::{Color, Image, ModelRc, StyledText, VecModel};

use crate::rich_markdown::{self, Palette};
use crate::ui::{
    AccountTile, AttachmentRow, MessageRow, QuoteRow, ReactionRow, RoomRow, RoomSection, ThreadRow,
};

/// Counts are `u32`, Slint's are `int`: saturate.
fn count(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The tile of an account in the rail.
pub fn tile(view: &AccountView) -> AccountTile {
    AccountTile {
        id: view.id.as_str().into(),
        monogram: view.monogram.as_str().into(),
        label: view.label.as_str().into(),
        fresh: view.fresh,
        current: view.current,
        unread: view.unread,
        busy: view.busy,
        expired: view.status == Status::Expired,
        mentions: count(view.mentions),
    }
}

/// The row of a room, with the avatar file it has now.
pub fn room_row(room: &Room, avatars: &AvatarUrls) -> RoomRow {
    let (initials, color) = room_placeholder(room);
    let url = avatars.url(&room.avatar_owner());
    let image = load_image(url);
    RoomRow {
        section: RoomSection::Room,
        id: room.id.as_str().into(),
        name: room.name.as_str().into(),
        kind: kind_name(room.kind).into(),
        unread: count(room.unread),
        mentions: count(room.mention_badge()),
        has_unread: room.has_unread(),
        favorite: room.favorite,
        can_leave: room.can_leave(),
        initials: initials.into(),
        initials_color: parse_color(&color),
        avatar_url: url.into(),
        has_avatar: image.is_some(),
        avatar: image.unwrap_or_default(),
    }
}

/// The row of a thread of the list, from its parent, with the avatar file the author has now. The
/// time of the last reply is read now, in `zone`.
pub fn thread_row(
    parent: &Message,
    avatars: &AvatarUrls,
    zone: &TimeZone,
    now: Timestamp,
) -> ThreadRow {
    let name = author_name(&parent.author);
    let url = message_owner(parent).map_or("", |owner| avatars.url(&owner));
    let image = load_image(url);
    ThreadRow {
        id: parent.id.as_str().into(),
        author: name.into(),
        last_reply: thread_last_reply_text(parent, zone, now).into(),
        text: row_text(parent).into(),
        summary: row_summary(parent).into(),
        initials: initials(name).into(),
        initials_color: parse_color(&color(&parent.author.username)),
        avatar_url: url.into(),
        has_avatar: image.is_some(),
        avatar: image.unwrap_or_default(),
        // The model knows from the room's subscription, not the parent.
        unread: false,
        mentioned: false,
    }
}

/// The picture in the file at `url`, if there is a url and the file loads.
pub fn load_image(url: &str) -> Option<Image> {
    (!url.is_empty())
        .then(|| file_path(url))
        .flatten()
        .and_then(|path| Image::load_from_path(&path).ok())
}

/// The row of the message at `row` of the list (newest first), with the avatar file it has now;
/// `None` for a row that isn't there. What depends on the time (the day's heading, the time of the
/// last reply) is read now. `in_pane` is for the rows of the thread pane: a message there has no
/// thread of its own to start, and a reply is not sent to the room again.
pub fn message_row(
    list: &MessageList,
    row: usize,
    palette: &Palette,
    in_pane: bool,
) -> Option<MessageRow> {
    let message = list.message(row)?;
    let now = Timestamp::now();
    let clock = Clock {
        zone: list.zone(),
        now,
    };
    let (body_styled, big_emoji) = match &message.kind {
        MessageKind::Text(text) => (
            rich_markdown::styled(text, list.viewer(), clock, palette),
            rich_markdown::is_big_emoji(text),
        ),
        MessageKind::System(_) => (StyledText::from_plain_text(""), false),
    };
    let body_html = message_html(message, list.viewer(), clock);
    let url = message_owner(message).map_or("", |owner| list.avatars().url(&owner));
    let image = load_image(url);
    let starts_thread = !in_pane && can_start_thread(message);
    Some(MessageRow {
        id: message.id.as_str().into(),
        author: author_name(&message.author).into(),
        time: time_text(message.ts, list.zone()).into(),
        body: body(message).into(),
        has_links: body_html.contains("<a href="),
        body_html: body_html.into(),
        body_styled,
        big_emoji,
        is_system: matches!(message.kind, MessageKind::System(_)),
        system_text: system_text(message).into(),
        first_of_day: list.first_of_day(row),
        day_label: day_label(message.ts, list.zone(), now).into(),
        edited: message.edited,
        mentions_me: mentions_me(message, list.viewer()),
        quotes: quote_rows(message, list.viewer(), clock, palette),
        attachments: attachment_rows(message, list),
        reactions: reaction_rows(message, list),
        can_react: can_react(message),
        can_start_thread: starts_thread,
        thread_replies: thread_replies(message),
        thread_last_reply: thread_last_reply_text(message, list.zone(), now).into(),
        thread_id: thread_id(message).into(),
        in_thread_shown: !in_pane && in_thread_shown(message),
        thread_parent_text: list.thread_parent_text(row).into(),
        has_menu: starts_thread
            || can_quote(message)
            || can_react(message)
            || can_edit(message, list.viewer(), now)
            || can_delete(message, list.viewer(), now),
        delivery: delivery_name(message.delivery).into(),
        initials: initials(author_name(&message.author)).into(),
        initials_color: parse_color(&color(&message.author.username)),
        avatar_url: url.into(),
        has_avatar: image.is_some(),
        avatar: image.unwrap_or_default(),
    })
}

/// The quotes of the message, as a model of their own; the empty model, which allocates nothing,
/// for the many messages that have none.
fn quote_rows(
    message: &Message,
    viewer: &Viewer,
    clock: Clock,
    palette: &Palette,
) -> ModelRc<QuoteRow> {
    if message.quotes.is_empty() {
        return ModelRc::default();
    }
    let rows: Vec<QuoteRow> = quotes(message, clock)
        .into_iter()
        .map(|quote| QuoteRow {
            body_html: render(&quote.text, viewer, clock).into(),
            body_styled: rich_markdown::styled(&quote.text, viewer, clock, palette),
            author: quote.author.into(),
            time: quote.time.into(),
            body: quote.body.into(),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// The files of the message, as a model of their own; the empty model for the many messages that
/// have none. An image shows its file if the core has downloaded it.
fn attachment_rows(message: &Message, list: &MessageList) -> ModelRc<AttachmentRow> {
    if message.attachments.is_empty() {
        return ModelRc::default();
    }
    let rows: Vec<AttachmentRow> =
        attachments(message, list.viewer().server.as_deref(), list.images())
            .into_iter()
            .map(|attachment| {
                let image = load_image(&attachment.image);
                AttachmentRow {
                    title: attachment.title.into(),
                    description: attachment.description.into(),
                    size: attachment.size.into(),
                    link: attachment.link.into(),
                    is_image: attachment.is_image,
                    image_url: attachment.image.into(),
                    has_image: image.is_some(),
                    image: image.unwrap_or_default(),
                    width: attachment.width,
                    height: attachment.height,
                }
            })
            .collect();
    ModelRc::new(VecModel::from(rows))
}

/// The reactions of the message, as a model of their own; the empty model for the many messages
/// that have none. A custom emoji shows its file if the core has downloaded it.
fn reaction_rows(message: &Message, list: &MessageList) -> ModelRc<ReactionRow> {
    if message.reactions.is_empty() {
        return ModelRc::default();
    }
    let rows: Vec<ReactionRow> = reactions(message, list.viewer())
        .into_iter()
        .map(|reaction| {
            let image = (!reaction.image.is_empty())
                .then(|| load_image(list.emoji_images().url(reaction.name())))
                .flatten();
            let label = if !reaction.glyph.is_empty() {
                reaction.glyph.clone()
            } else if !reaction.image.is_empty() {
                reaction.image.clone()
            } else {
                reaction.emoji.clone()
            };
            let text = if reaction.glyph.is_empty() {
                reaction.emoji.clone()
            } else {
                reaction.glyph
            };
            ReactionRow {
                emoji: reaction.emoji.into(),
                label: label.into(),
                text: text.into(),
                has_image: image.is_some(),
                image: image.unwrap_or_default(),
                count: reaction.count,
                mine: reaction.mine,
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// The colour of `#rrggbb`, which `rc_view` gives for the disc behind the initials; transparent if
/// it isn't one.
fn parse_color(hex: &str) -> Color {
    let channel = |at: usize| {
        hex.get(at..at + 2)
            .and_then(|digits| u8::from_str_radix(digits, 16).ok())
    };
    match (hex.starts_with('#'), channel(1), channel(3), channel(5)) {
        (true, Some(red), Some(green), Some(blue)) => Color::from_rgb_u8(red, green, blue),
        _ => Color::default(),
    }
}
