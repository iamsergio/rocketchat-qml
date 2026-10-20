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

#[cfg(test)]
mod tests {
    use super::*;

    // ROOM-002
    #[test]
    fn each_kind_has_its_own_name() {
        let kinds = [RoomKind::Channel, RoomKind::Private, RoomKind::Direct];
        assert_eq!(kinds.map(kind_name), ["channel", "private", "direct"]);
    }
}
