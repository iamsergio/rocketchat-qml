// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The roles of a room row, which `RoomDelegate` reads, for the models that list rooms: `RoomList`
//! and `RoomSearch`.

use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QString, QVariant};
use rc_core::domain::Room;
use rc_view::avatar_view::{AvatarUrls, room_placeholder};
use rc_view::room_view::kind_name;

/// `Qt::DisplayRole`, which shows the name too.
const DISPLAY_ROLE: i32 = 0;

// The custom roles, from `Qt::UserRole` on. Plain constants, as cxx-qt's `qenum` can't set values
// and the default ones would start at `Qt::DisplayRole`.
const ROOM_ID_ROLE: i32 = 256;
const NAME_ROLE: i32 = 257;
const KIND_ROLE: i32 = 258;
const UNREAD_ROLE: i32 = 259;
const USER_MENTIONS_ROLE: i32 = 260;
const GROUP_MENTIONS_ROLE: i32 = 261;
const ALERT_ROLE: i32 = 262;
/// The only role that changes when an avatar file arrives.
pub const AVATAR_ROLE: i32 = 263;
const AVATAR_INITIALS_ROLE: i32 = 264;
const AVATAR_COLOR_ROLE: i32 = 265;
const FAVORITE_ROLE: i32 = 266;
const HAS_UNREAD_ROLE: i32 = 267;
const CAN_LEAVE_ROLE: i32 = 268;

/// Counts are `u32`, Qt's are `int`: saturate.
fn count(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// `index` as the `int` Qt wants. Rooms never come near `i32::MAX`.
#[must_use]
pub fn row(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

/// What `room` shows in `role`, with the avatar files of `avatars`.
#[must_use]
pub fn room_data(room: &Room, avatars: &AvatarUrls, role: i32) -> QVariant {
    match role {
        DISPLAY_ROLE | NAME_ROLE => QVariant::from(&QString::from(&room.name)),
        ROOM_ID_ROLE => QVariant::from(&QString::from(room.id.as_str())),
        KIND_ROLE => QVariant::from(&QString::from(kind_name(room.kind))),
        UNREAD_ROLE => QVariant::from(&count(room.unread)),
        USER_MENTIONS_ROLE => QVariant::from(&count(room.mention_badge())),
        GROUP_MENTIONS_ROLE => QVariant::from(&count(room.group_mentions)),
        ALERT_ROLE => QVariant::from(&room.alert),
        FAVORITE_ROLE => QVariant::from(&room.favorite),
        HAS_UNREAD_ROLE => QVariant::from(&room.has_unread()),
        CAN_LEAVE_ROLE => QVariant::from(&room.can_leave()),
        AVATAR_ROLE => QVariant::from(&QString::from(avatars.url(&room.avatar_owner()))),
        AVATAR_INITIALS_ROLE => QVariant::from(&QString::from(&room_placeholder(room).0)),
        AVATAR_COLOR_ROLE => QVariant::from(&QString::from(&room_placeholder(room).1)),
        _ => QVariant::default(),
    }
}

/// The names of the roles in QML.
#[must_use]
pub fn room_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut names = QHash::<QHashPair_i32_QByteArray>::default();
    for (role, name) in [
        (DISPLAY_ROLE, "display"),
        (ROOM_ID_ROLE, "roomId"),
        (NAME_ROLE, "name"),
        (KIND_ROLE, "kind"),
        (UNREAD_ROLE, "unread"),
        (USER_MENTIONS_ROLE, "userMentions"),
        (GROUP_MENTIONS_ROLE, "groupMentions"),
        (ALERT_ROLE, "alert"),
        (FAVORITE_ROLE, "favorite"),
        (HAS_UNREAD_ROLE, "hasUnread"),
        (CAN_LEAVE_ROLE, "canLeave"),
        (AVATAR_ROLE, "avatar"),
        (AVATAR_INITIALS_ROLE, "avatarInitials"),
        (AVATAR_COLOR_ROLE, "avatarColor"),
    ] {
        names.insert(role, QByteArray::from(name));
    }
    names
}
