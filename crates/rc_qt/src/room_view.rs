// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `RoomList` model and the `Room` view model show for a room. Plain Rust, so it can be
//! unit-tested without Qt.

use rc_core::domain::{Room, RoomKind};
use rc_core::timeline::TimelineStatus;

/// How QML sees the kind of a room. A string rather than another Qt enum, which keeps the role a
/// plain `QVariant`.
pub fn kind_name(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Channel => "channel",
        RoomKind::Private => "private",
        RoomKind::Direct => "direct",
    }
}

/// The values of the `Room` properties: the open room, and how loading its messages is going.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomState {
    /// Empty when no room is open.
    pub room_id: String,
    /// The name and kind the room list shows; empty while the room isn't in it.
    pub name: String,
    pub kind: String,
    pub loading: bool,
    pub loading_older: bool,
    pub reached_start: bool,
    /// Why the last load failed, as it is to be shown. Empty if it didn't.
    pub error_text: String,
}

/// The state for the room `status` is about, with its name and kind taken from `rooms`.
#[must_use]
pub fn room_state(rooms: &[Room], status: &TimelineStatus) -> RoomState {
    let Some(id) = &status.room else {
        return RoomState::default();
    };
    let room = rooms.iter().find(|room| &room.id == id);
    RoomState {
        room_id: id.to_string(),
        name: room.map(|room| room.name.clone()).unwrap_or_default(),
        kind: room
            .map(|room| kind_name(room.kind).to_owned())
            .unwrap_or_default(),
        loading: status.loading,
        loading_older: status.loading_older,
        reached_start: status.reached_start,
        error_text: status
            .error
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
    }
}

#[cfg(test)]
mod tests {
    use rc_core::timeline::TimelineError;

    use super::*;

    // ROOM-002
    #[test]
    fn each_kind_has_its_own_name() {
        let kinds = [RoomKind::Channel, RoomKind::Private, RoomKind::Direct];
        assert_eq!(kinds.map(kind_name), ["channel", "private", "direct"]);
    }

    fn room(id: &str, name: &str, kind: RoomKind) -> Room {
        Room {
            id: id.to_owned().into(),
            kind,
            name: name.to_owned(),
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
        }
    }

    // MSG-001
    #[test]
    fn no_open_room_is_the_empty_state() {
        let rooms = [room("1", "general", RoomKind::Channel)];
        assert_eq!(
            room_state(&rooms, &TimelineStatus::default()),
            RoomState::default()
        );
    }

    // MSG-001
    #[test]
    fn the_open_room_takes_its_name_and_kind_from_the_list() {
        let rooms = [
            room("1", "general", RoomKind::Channel),
            room("2", "Bob", RoomKind::Direct),
        ];
        let status = TimelineStatus {
            room: Some("2".to_owned().into()),
            loading: true,
            ..TimelineStatus::default()
        };
        assert_eq!(
            room_state(&rooms, &status),
            RoomState {
                room_id: "2".into(),
                name: "Bob".into(),
                kind: "direct".into(),
                loading: true,
                ..RoomState::default()
            }
        );
    }

    // MSG-002
    #[test]
    fn the_state_carries_the_loading_flags_and_the_error() {
        let status = TimelineStatus {
            room: Some("9".to_owned().into()),
            loading: false,
            loading_older: true,
            reached_start: true,
            error: Some(TimelineError::Unreachable),
        };
        let state = room_state(&[], &status);
        assert_eq!(state.room_id, "9");
        assert_eq!(state.name, "");
        assert!(state.loading_older && state.reached_start && !state.loading);
        assert_eq!(state.error_text, "Can't reach the server");
    }
}
