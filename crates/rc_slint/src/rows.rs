// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The rows the GUI shows, from what `rc_view` says: a thin mapping to the Slint structs, with no
//! decisions of its own.

use rc_core::domain::Room;
use rc_view::accounts::{AccountView, Status};
use rc_view::avatar_view::{AvatarUrls, file_path, room_placeholder};
use rc_view::room_view::kind_name;
use slint::{Color, Image};

use crate::ui::{AccountTile, RoomRow, RoomSection};

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
    let image = (!url.is_empty())
        .then(|| file_path(url))
        .flatten()
        .and_then(|path| Image::load_from_path(&path).ok());
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
