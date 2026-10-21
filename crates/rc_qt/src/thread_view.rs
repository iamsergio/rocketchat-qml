// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `Thread` view model shows for the open thread. Plain Rust, so it can be unit-tested
//! without Qt.

use rc_core::domain::{MessageId, Room, RoomId};
use rc_core::timeline::ThreadStatus;

/// The values of the `Thread` properties: the open thread, and how loading its replies is going.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadState {
    /// The message the thread belongs to. Empty when no thread is open.
    pub parent_id: String,
    /// The open room, which holds the parent: replies go there. Empty when no room is open.
    pub room_id: String,
    /// The name of the open room, for the pane's heading and its check box. Empty while the room
    /// isn't in the room list.
    pub room_name: String,
    pub loading: bool,
    pub loading_older: bool,
    pub reached_start: bool,
    /// Why the last load failed, as it is to be shown. Empty if it didn't.
    pub error_text: String,
}

/// The state for the thread `status` is about, in the open room `room` (with its name taken from
/// `rooms`).
#[must_use]
pub fn thread_state(rooms: &[Room], room: Option<&RoomId>, status: &ThreadStatus) -> ThreadState {
    let name = room
        .and_then(|id| rooms.iter().find(|candidate| &candidate.id == id))
        .map(|room| room.name.clone())
        .unwrap_or_default();
    ThreadState {
        parent_id: status
            .parent
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
        room_id: room.map_or_else(String::new, ToString::to_string),
        room_name: name,
        loading: status.loading,
        loading_older: status.loading_older,
        reached_start: status.reached_start,
        error_text: status
            .error
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
    }
}

/// What a reply typed in the pane hands to the core: the room, the text, the message it quotes
/// (`quoted_id` is empty for none) and the thread (`parent_id`), or `None` if there is nothing to
/// send, because no thread is open or the text is blank.
#[must_use]
pub fn outgoing_reply(
    state: &ThreadState,
    text: &str,
    quoted_id: &str,
) -> Option<(RoomId, String, Option<MessageId>, MessageId)> {
    if state.parent_id.is_empty() || state.room_id.is_empty() || text.trim().is_empty() {
        return None;
    }
    let quoted = (!quoted_id.is_empty()).then(|| MessageId::from(quoted_id.to_owned()));
    Some((
        RoomId::from(state.room_id.clone()),
        text.to_owned(),
        quoted,
        MessageId::from(state.parent_id.clone()),
    ))
}

#[cfg(test)]
mod tests {
    use rc_core::domain::RoomKind;
    use rc_core::timeline::TimelineError;

    use super::*;

    fn room(id: &str, name: &str) -> Room {
        Room {
            id: id.to_owned().into(),
            kind: RoomKind::Channel,
            name: name.to_owned(),
            peer: None,
            avatar_etag: None,
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            favorite: false,
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    fn open(parent: &str) -> ThreadStatus {
        ThreadStatus {
            parent: Some(parent.to_owned().into()),
            ..ThreadStatus::default()
        }
    }

    // ACT-004
    #[test]
    fn no_thread_is_the_empty_state() {
        let rooms = [room("1", "general")];
        let room_id = RoomId::from("1".to_owned());
        let state = thread_state(&rooms, Some(&room_id), &ThreadStatus::default());
        // The room is known, the thread is not.
        assert_eq!(state.parent_id, "");
        assert_eq!(state.room_name, "general");
        assert_eq!(
            thread_state(&rooms, None, &ThreadStatus::default()),
            ThreadState::default()
        );
    }

    // ACT-004
    #[test]
    fn the_open_thread_takes_the_name_of_the_open_room_from_the_list() {
        let rooms = [room("1", "general"), room("2", "random")];
        let room_id = RoomId::from("2".to_owned());
        let status = ThreadStatus {
            loading: true,
            ..open("m7")
        };
        assert_eq!(
            thread_state(&rooms, Some(&room_id), &status),
            ThreadState {
                parent_id: "m7".into(),
                room_id: "2".into(),
                room_name: "random".into(),
                loading: true,
                ..ThreadState::default()
            }
        );
        // A room that isn't listed has no name, and the thread is still open.
        let unlisted = RoomId::from("9".to_owned());
        let state = thread_state(&rooms, Some(&unlisted), &open("m7"));
        assert_eq!(
            (state.parent_id.as_str(), state.room_name.as_str()),
            ("m7", "")
        );
    }

    // ACT-004
    #[test]
    fn the_state_carries_the_loading_flags_and_the_error() {
        let status = ThreadStatus {
            loading_older: true,
            reached_start: true,
            error: Some(TimelineError::Unreachable),
            ..open("m7")
        };
        let state = thread_state(&[], None, &status);
        assert!(state.loading_older && state.reached_start && !state.loading);
        assert_eq!(state.error_text, "Can't reach the server");
    }

    fn replying() -> ThreadState {
        ThreadState {
            parent_id: "m7".into(),
            room_id: "r1".into(),
            ..ThreadState::default()
        }
    }

    // ACT-004
    #[test]
    fn a_reply_goes_to_the_open_room_in_the_open_thread() {
        let (room, text, quoted, parent) = outgoing_reply(&replying(), "hello\n", "").unwrap();
        assert_eq!((room.as_str(), text.as_str()), ("r1", "hello\n"));
        assert_eq!(quoted, None);
        assert_eq!(parent.as_str(), "m7");
        let (_, _, quoted, _) = outgoing_reply(&replying(), "hello", "m9").unwrap();
        assert_eq!(quoted, Some(MessageId::from("m9".to_owned())));
    }

    // ACT-004
    #[test]
    fn nothing_is_sent_without_an_open_thread_or_with_blank_text() {
        assert!(outgoing_reply(&ThreadState::default(), "hello", "").is_none());
        let no_room = ThreadState {
            room_id: String::new(),
            ..replying()
        };
        assert!(outgoing_reply(&no_room, "hello", "").is_none());
        assert!(outgoing_reply(&replying(), "", "").is_none());
        assert!(outgoing_reply(&replying(), " \n\t", "m9").is_none());
    }
}
