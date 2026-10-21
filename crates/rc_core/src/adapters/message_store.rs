// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A message store that lives as long as the process.

use std::collections::HashMap;

use crate::domain::RoomId;
use crate::ports::{MessageStore, RoomHistory};

#[derive(Default)]
pub struct MemoryMessageStore(HashMap<RoomId, RoomHistory>);

impl MessageStore for MemoryMessageStore {
    fn load(&self, room: &RoomId) -> RoomHistory {
        self.0.get(room).cloned().unwrap_or_default()
    }

    fn save(&mut self, room: &RoomId, history: &RoomHistory) {
        self.0.insert(room.clone(), history.clone());
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Author, Delivery, Message, MessageKind};

    fn history(id: &str) -> RoomHistory {
        RoomHistory {
            messages: vec![Message {
                id: id.to_owned().into(),
                room_id: "r".to_owned().into(),
                author: Author {
                    id: "u".to_owned().into(),
                    username: "u".into(),
                    display_name: None,
                },
                ts: jiff::Timestamp::UNIX_EPOCH,
                kind: MessageKind::Text("hi".into()),
                delivery: Delivery::Sent,
                attachments: Vec::new(),
                quotes: Vec::new(),
                edited: false,
                reactions: Vec::new(),
            }],
            reached_start: true,
            synced_at: Some(jiff::Timestamp::UNIX_EPOCH),
        }
    }

    // MSG-001
    #[test]
    fn keeps_each_rooms_messages_apart() {
        let mut store = MemoryMessageStore::default();
        let (a, b): (RoomId, RoomId) = ("a".to_owned().into(), "b".to_owned().into());
        assert_eq!(store.load(&a), RoomHistory::default());
        store.save(&a, &history("m1"));
        store.save(&b, &history("m2"));
        assert_eq!(store.load(&a), history("m1"));
        assert_eq!(store.load(&b), history("m2"));
        // Saving again replaces.
        store.save(&a, &history("m3"));
        assert_eq!(store.load(&a), history("m3"));
    }

    // MSG-001
    #[test]
    fn clearing_forgets_every_room() {
        let mut store = MemoryMessageStore::default();
        let a: RoomId = "a".to_owned().into();
        store.save(&a, &history("m1"));
        store.clear();
        assert_eq!(store.load(&a), RoomHistory::default());
    }
}
