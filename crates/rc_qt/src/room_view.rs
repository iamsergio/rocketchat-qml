// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `RoomList` model shows for a room. Plain Rust, so it can be unit-tested without Qt.

use rc_core::domain::RoomKind;

/// How QML sees the kind of a room. A string rather than another Qt enum, which keeps the role a
/// plain `QVariant`.
pub fn kind_name(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Channel => "channel",
        RoomKind::Private => "private",
        RoomKind::Direct => "direct",
    }
}

/// The text that marks the kind of a room in the list: `#` for a channel, a lock for a private
/// group and `@` for a direct message. Decided here so that QML only shows it.
pub fn kind_marker(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Channel => "#",
        RoomKind::Private => "\u{1F512}",
        RoomKind::Direct => "@",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ROOM-002
    #[test]
    fn each_kind_has_its_own_name_and_marker() {
        let kinds = [RoomKind::Channel, RoomKind::Private, RoomKind::Direct];
        assert_eq!(kinds.map(kind_name), ["channel", "private", "direct"]);
        assert_eq!(kinds.map(kind_marker), ["#", "\u{1F512}", "@"]);
    }
}
