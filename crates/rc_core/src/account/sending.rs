// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Sending messages and retrying the ones that failed.

use crate::domain::{Author, Delivery, Message, MessageId, MessageKind, Quote, RoomId};
use crate::ports::{ApiError, Connector, ServerApi, ThreadTarget};
use crate::quote::{permalink, wire_text};
use crate::session::SessionState;

use super::{Actor, Done, SendDone};

impl<C: Connector> Actor<C> {
    pub(super) fn send_message(
        &mut self,
        room: RoomId,
        text: &str,
        quote: Option<MessageId>,
        thread: Option<MessageId>,
        also_to_channel: bool,
    ) {
        let Some(me) = self.own_user() else {
            tracing::debug!("send_message ignored: not logged in");
            return;
        };
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.rooms.get(&room).is_none() {
            tracing::debug!(%room, "send_message ignored: not in the room list");
            return;
        }
        // A reply goes only to the thread that is open.
        let target = match thread {
            Some(parent) => {
                let open = self.open_thread.as_ref().is_some_and(|thread| {
                    thread.parent.id == parent && thread.timeline.room() == &room
                });
                if !open {
                    tracing::debug!(%parent, "send_message ignored: not the open thread");
                    return;
                }
                Some(ThreadTarget {
                    parent,
                    also_to_channel,
                })
            }
            None => None,
        };
        let quotes: Vec<Quote> = quote
            .and_then(|quoted| self.quote_of(&room, &quoted))
            .into_iter()
            .collect();
        let message = Message {
            id: MessageId::generate(),
            room_id: room.clone(),
            author: Author {
                id: me.id.clone(),
                username: me.username.clone(),
                display_name: me.display_name.clone(),
            },
            ts: jiff::Timestamp::now(),
            kind: MessageKind::Text(text.into()),
            delivery: Delivery::Pending,
            attachments: Vec::new(),
            quotes,
            edited: false,
            reactions: Vec::new(),
            thread_id: target.as_ref().map(|target| target.parent.clone()),
            thread_shown: target.as_ref().is_some_and(|target| target.also_to_channel),
            thread_replies: 0,
            thread_last_reply: None,
        };
        let id = message.id.clone();
        let wire = wire_text(&message.quotes, text);
        if target.is_some() {
            self.change_thread_timeline(|timeline| vec![timeline.add_unsent(message.clone())]);
        }
        if message.shows_in_room() {
            self.change_timeline(&room, |timeline| vec![timeline.add_unsent(message)]);
        }
        self.post(room, id, wire, target);
    }

    /// The quote of a message of the open room or thread, as a copy to show until the server's
    /// comes. `None` if the room doesn't hold the message, or the message isn't sent text:
    /// the server can't quote what it doesn't have, and a system message says nothing.
    fn quote_of(&self, room: &RoomId, quoted: &MessageId) -> Option<Quote> {
        let message = self
            .find_message(quoted)
            .filter(|message| &message.room_id == room && !message.is_unsent());
        let Some(Message {
            kind: MessageKind::Text(text),
            author,
            ts,
            ..
        }) = message
        else {
            tracing::debug!(message = %quoted, "no quote: not sent text of the open room");
            return None;
        };
        let Some(site) = self.quote_site() else {
            tracing::debug!("no quote: no server address to make the link from");
            return None;
        };
        let room_info = self.rooms.get(room)?;
        Some(Quote {
            message: Some(quoted.clone()),
            link: permalink(site, room_info, quoted),
            author: author.username.clone(),
            text: text.clone(),
            ts: Some(*ts),
        })
    }

    /// What the quote links must begin with: the server's own `Site_Url`, which is known from the
    /// login on, else the address that was used to log in.
    fn quote_site(&self) -> Option<&str> {
        self.session
            .as_ref()
            .map(|session| session.site_url.as_str())
    }

    pub(super) fn retry_message(&mut self, id: &MessageId) {
        let Some(Message {
            kind: MessageKind::Text(text),
            delivery: Delivery::Failed,
            quotes,
            room_id,
            thread_id,
            thread_shown,
            ..
        }) = self.find_message(id)
        else {
            tracing::debug!(message = %id, "retry_message ignored: not a failed message");
            return;
        };
        let room = room_id.clone();
        let text = wire_text(quotes, &text.text);
        let target = thread_id.clone().map(|parent| ThreadTarget {
            parent,
            also_to_channel: *thread_shown,
        });
        self.change_timelines(&room, |timeline| {
            timeline
                .set_delivery(id, Delivery::Pending)
                .into_iter()
                .collect()
        });
        self.post(room, id.clone(), text, target);
    }

    /// Sends the message in a task, which reports the outcome.
    fn post(&mut self, room: RoomId, id: MessageId, text: String, thread: Option<ThreadTarget>) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api
                .send_message(&credentials, &room, &id, &text, thread.as_ref())
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Send(SendDone {
                attempt,
                room,
                id,
                outcome,
            }));
        });
    }

    pub(super) fn handle_send_done(&mut self, done: SendDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale send dropped");
            return;
        }
        match done.outcome {
            Ok(message) => {
                // A page that is on its way may predate the message, and would drop it.
                self.hold_for_page(&message);
                self.change_timelines(&done.room, |timeline| {
                    timeline.settle(message.clone()).into_iter().collect()
                });
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't send the message");
                self.change_timelines(&done.room, |timeline| {
                    timeline
                        .set_delivery(&done.id, Delivery::Failed)
                        .into_iter()
                        .collect()
                });
            }
        }
    }
}
