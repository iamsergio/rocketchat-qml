// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Editing, deleting and reacting to messages.

use crate::client::Event;
use crate::domain::{Message, MessageId, MessageKind, RoomId};
use crate::policy::{ActionError, MessageAction, can_react};
use crate::ports::{ApiError, Connector, ServerApi};
use crate::quote::wire_text;
use crate::session::SessionState;

use super::{ActionDone, Actor, Applied, Done};

impl<C: Connector> Actor<C> {
    /// Answers an edit or a deletion the frontend asked for.
    pub(super) fn answer(
        &self,
        id: MessageId,
        action: MessageAction,
        outcome: Result<(), ActionError>,
    ) {
        (self.on_event)(Event::MessageAction {
            id,
            action,
            outcome,
        });
    }

    /// The message of the open room or thread, if it is one the user may do `action` to. Answers
    /// with a refusal otherwise.
    fn changeable(&self, id: &MessageId, action: MessageAction) -> Option<&Message> {
        let me = self.own_user()?;
        let message = self.find_message(id).filter(|message| {
            let now = jiff::Timestamp::now();
            match action {
                MessageAction::Edit => self.policy.can_edit(message, &me.id, now),
                MessageAction::Delete => self.policy.can_delete(message, &me.id, now),
                MessageAction::React => can_react(message),
            }
        });
        if message.is_none() {
            tracing::debug!(message = %id, ?action, "refused: not a message the user can change");
            self.answer(id.clone(), action, Err(ActionError::NotAllowed(action)));
        }
        message
    }

    pub(super) fn edit_own_message(&mut self, id: MessageId, text: &str) {
        let action = MessageAction::Edit;
        if self.own_user().is_none() {
            tracing::debug!("edit_message ignored: not logged in");
            return;
        }
        let Some(message) = self.changeable(&id, action) else {
            return;
        };
        let room = message.room_id.clone();
        let text = text.trim();
        if text.is_empty() {
            self.answer(id, action, Err(ActionError::Empty));
        } else if matches!(&message.kind, MessageKind::Text(held) if held.text == text) {
            self.answer(id, action, Ok(()));
        } else {
            // The links of the quotes are part of the text on the server: without them it
            // drops the quotes.
            let wire = wire_text(&message.quotes, text);
            self.request_action(room, id, action, Some(wire));
        }
    }

    pub(super) fn delete_own_message(&mut self, id: MessageId) {
        let action = MessageAction::Delete;
        if self.own_user().is_none() {
            tracing::debug!("delete_message ignored: not logged in");
            return;
        }
        if let Some(message) = self.changeable(&id, action) {
            let room = message.room_id.clone();
            self.request_action(room, id, action, None);
        }
    }

    pub(super) fn react_to_message(&mut self, id: MessageId, emoji: String, react: bool) {
        let action = MessageAction::React;
        if self.own_user().is_none() {
            tracing::debug!("react ignored: not logged in");
            return;
        }
        if emoji.is_empty() {
            self.answer(id, action, Err(ActionError::NotAllowed(action)));
            return;
        }
        if let Some(message) = self.changeable(&id, action) {
            let room = message.room_id.clone();
            self.request_reaction(room, id, emoji, react);
        }
    }

    /// Asks the server for the reaction, in a task that reports the outcome.
    fn request_reaction(&self, room: RoomId, id: MessageId, emoji: String, react: bool) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api
                .react(&credentials, &id, &emoji, react)
                .await
                .map(|()| Applied::Reacted { emoji, react });
            // Fails only if the actor is gone.
            let _ = done.send(Done::Action(ActionDone {
                attempt,
                room,
                id,
                action: MessageAction::React,
                outcome,
            }));
        });
    }

    /// Edits the message to `text`, or deletes it without one, in a task that reports the outcome.
    fn request_action(
        &self,
        room: RoomId,
        id: MessageId,
        action: MessageAction,
        text: Option<String>,
    ) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = match text {
                Some(text) => api
                    .edit_message(&credentials, &room, &id, &text)
                    .await
                    .map(|message| Applied::Edited(Box::new(message))),
                None => api
                    .delete_message(&credentials, &room, &id)
                    .await
                    .map(|()| Applied::Deleted),
            };
            // Fails only if the actor is gone.
            let _ = done.send(Done::Action(ActionDone {
                attempt,
                room,
                id,
                action,
                outcome,
            }));
        });
    }

    pub(super) fn handle_action_done(&mut self, done: ActionDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale message action dropped");
            return;
        }
        match done.outcome {
            Ok(Applied::Edited(message)) => {
                // The push of the same copy follows, which changes nothing. A page that is on its
                // way may predate the edit, so the copy joins it, as a sent message does.
                self.hold_for_page(&message);
                self.replace_everywhere(&done.room, &message);
                self.answer(done.id, done.action, Ok(()));
            }
            Ok(Applied::Deleted) => {
                self.delete_message(&done.room, &done.id);
                self.answer(done.id, done.action, Ok(()));
            }
            Ok(Applied::Reacted { emoji, react }) => {
                self.apply_reaction(&done.room, &done.id, &emoji, react);
                self.answer(done.id, done.action, Ok(()));
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, message = %done.id, action = ?done.action, "refused");
                self.answer(done.id, done.action, Err(ActionError::from(&error)));
            }
        }
    }

    /// Puts the user's reaction in the held message without waiting for the server's push, which
    /// then changes nothing. The message is taken as it is now, not as it was when the reaction was
    /// asked for. Nothing if it has gone meanwhile.
    fn apply_reaction(&mut self, room: &RoomId, id: &MessageId, emoji: &str, react: bool) {
        let Some(username) = self.own_user().map(|user| user.username.clone()) else {
            return;
        };
        let reacted = self
            .find_message(id)
            .filter(|held| &held.room_id == room)
            .and_then(|held| {
                let mut copy = held.clone();
                copy.set_reaction(emoji, &username, react).then_some(copy)
            });
        let Some(message) = reacted else {
            return;
        };
        // A page that is on its way may predate the reaction, so the copy joins it, as for an edit.
        self.hold_for_page(&message);
        self.replace_everywhere(room, &message);
    }
}
