// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The rows the GUI shows, from what `rc_view` says: a thin mapping to the Slint structs, with no
//! decisions of its own.

use jiff::Timestamp;
use rc_core::domain::{MessageKind, Room};
use rc_view::accounts::{AccountView, Status};
use rc_view::avatar_view::{
    AvatarUrls, color, file_path, initials, message_owner, room_placeholder,
};
use rc_view::message_list::MessageList;
use rc_view::message_view::{author_name, body, day_label, mentions_me, system_text, time_text};
use rc_view::rich_html::message_html;
use rc_view::room_view::kind_name;
use rc_view::viewer::Clock;
use slint::{Color, Image, StyledText};

use crate::rich_markdown::{self, Palette};
use crate::ui::{AccountTile, MessageRow, RoomRow, RoomSection};

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
    let image = avatar_image(url);
    RoomRow {
        section: RoomSection::Room,
        id: room.id.as_str().into(),
        name: room.name.as_str().into(),
        kind: kind_name(room.kind).into(),
        unread: count(room.unread),
        mentions: count(room.mention_badge()),
        has_unread: room.has_unread(),
        initials: initials.into(),
        initials_color: parse_color(&color),
        avatar_url: url.into(),
        has_avatar: image.is_some(),
        avatar: image.unwrap_or_default(),
    }
}

/// The picture in the avatar file at `url`, if there is a url and the file loads.
fn avatar_image(url: &str) -> Option<Image> {
    (!url.is_empty())
        .then(|| file_path(url))
        .flatten()
        .and_then(|path| Image::load_from_path(&path).ok())
}

/// The row of the message at `row` of the list (newest first), with the avatar file it has now;
/// `None` for a row that isn't there. What depends on the time (the day's heading) is read now.
pub fn message_row(list: &MessageList, row: usize, palette: &Palette) -> Option<MessageRow> {
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
    let url = message_owner(message).map_or("", |owner| list.avatars().url(&owner));
    let image = avatar_image(url);
    Some(MessageRow {
        id: message.id.as_str().into(),
        author: author_name(&message.author).into(),
        time: time_text(message.ts, list.zone()).into(),
        body: body(message).into(),
        body_html: message_html(message, list.viewer(), clock).into(),
        body_styled,
        big_emoji,
        is_system: matches!(message.kind, MessageKind::System(_)),
        system_text: system_text(message).into(),
        first_of_day: list.first_of_day(row),
        day_label: day_label(message.ts, list.zone(), now).into(),
        mentions_me: mentions_me(message, list.viewer()),
        initials: initials(author_name(&message.author)).into(),
        initials_color: parse_color(&color(&message.author.username)),
        avatar_url: url.into(),
        has_avatar: image.is_some(),
        avatar: image.unwrap_or_default(),
    })
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
