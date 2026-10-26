// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching the server's directory, and opening a person's direct message, previewing a channel
//! and joining it, which are what the results are for (see `docs/design/directory.md`).

use std::future::Future;

use tokio::task::JoinHandle;

use crate::client::Event;
use crate::directory::{
    ActionState, Directory, DirectoryAction, DirectoryChannel, DirectoryState, Found, Person,
    RESULTS_PER_KIND,
};
use crate::domain::{AvatarOwner, Credentials, Room, RoomId, room_from, room_from_info};
use crate::ports::{ApiError, Connector, ServerApi};
use crate::rooms::RoomActionError;
use crate::session::SessionState;

use super::{Actor, Done};

/// The search of the directory: what the frontend was last told, and the wait and the request
/// that lead to the next thing to tell.
pub(super) struct DirectorySearch {
    /// What the frontend was last told. Its query is the one being searched for.
    told: Directory,
    /// Counts the searches and the clearings, to tell the answer of the current search from a
    /// stale one.
    attempt: u64,
    /// How long the search waits for the typing to stop.
    debounce: jiff::SignedDuration,
    /// When the request is to be made, while the search waits for the typing to stop.
    due: Option<jiff::Timestamp>,
    /// The request, while it runs.
    task: Option<JoinHandle<()>>,
}

impl DirectorySearch {
    pub(super) fn new(debounce: jiff::SignedDuration) -> Self {
        Self {
            told: Directory::cleared(),
            attempt: 0,
            debounce,
            due: None,
            task: None,
        }
    }

    /// When the request is to be made, if the search is waiting.
    pub(super) fn due(&self) -> Option<jiff::Timestamp> {
        self.due
    }
}

/// A directory request's answer.
pub(super) struct DirectoryDone {
    /// The search the request belongs to: the answer is stale if another has begun since.
    attempt: u64,
    outcome: Result<(Found<Person>, Found<DirectoryChannel>), ApiError>,
}

/// A directory action under way.
pub(super) struct RunningAction {
    /// Identifies the request: the answer is stale if the action was cancelled or the session
    /// ended.
    attempt: u64,
    action: DirectoryAction,
    task: JoinHandle<()>,
}

/// A directory action's answer.
pub(super) struct DirectoryActionDone {
    attempt: u64,
    outcome: Result<Room, ApiError>,
}

impl<C: Connector> Actor<C> {
    /// Starts a search for `text`, or retries one that failed, or clears the search if `text` is
    /// blank. See [`AccountHandle::search_directory`](crate::AccountHandle::search_directory).
    pub(super) fn search_directory(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            self.clear_directory_search();
            return;
        }
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("search_directory ignored: not logged in");
            return;
        }
        if text == self.directory.told.query {
            // The same text: the answer is coming, or is here. Only a failure is tried again, at
            // once, as the user asked for it.
            if matches!(self.directory.told.state, DirectoryState::Failed(_)) {
                self.begin_directory_search(text, true);
            }
            return;
        }
        self.begin_directory_search(text, false);
    }

    /// Replaces the search by one for `text`, which asks the server at once or when the typing has
    /// stopped.
    fn begin_directory_search(&mut self, text: &str, at_once: bool) {
        self.abort_directory_request();
        self.directory.attempt += 1;
        self.set_directory(Directory::searching(text));
        if at_once || self.directory.debounce <= jiff::SignedDuration::ZERO {
            self.request_directory();
        } else {
            self.directory.due = self.clock.now().checked_add(self.directory.debounce).ok();
        }
    }

    /// Asks the server if the wait for the typing to stop is over.
    pub(super) fn start_due_directory_search(&mut self) {
        if self
            .directory
            .due
            .is_some_and(|due| due <= self.clock.now())
        {
            self.request_directory();
        }
    }

    /// Asks the server for the people and the channels, in a task that reports the answer.
    fn request_directory(&mut self) {
        self.directory.due = None;
        let Some(session) = &self.session else {
            return;
        };
        let attempt = self.directory.attempt;
        let query = self.directory.told.query.clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.directory.task = Some(tokio::spawn(async move {
            let (people, channels) = tokio::join!(
                api.directory_users(&credentials, &query, RESULTS_PER_KIND),
                api.directory_channels(&credentials, &query, RESULTS_PER_KIND),
            );
            let outcome = match (people, channels) {
                (Ok(people), Ok(channels)) => Ok((people, channels)),
                // A dead token is what matters, whichever request found out.
                (Err(ApiError::Unauthorized), _) | (_, Err(ApiError::Unauthorized)) => {
                    Err(ApiError::Unauthorized)
                }
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            // Fails only if the actor is gone.
            let _ = done.send(Done::Directory(DirectoryDone { attempt, outcome }));
        }));
    }

    pub(super) fn handle_directory_done(&mut self, done: DirectoryDone) {
        if done.attempt != self.directory.attempt
            || !matches!(self.state, SessionState::LoggedIn(_))
        {
            tracing::debug!("answer of a stale directory search dropped");
            return;
        }
        self.directory.task = None;
        let query = self.directory.told.query.clone();
        match done.outcome {
            Ok((people, channels)) => {
                let owners: Vec<_> = (people.items.iter())
                    .map(|person| AvatarOwner::User(person.username.clone()))
                    .chain(
                        (channels.items.iter())
                            .map(|channel| AvatarOwner::Room(channel.id.clone())),
                    )
                    .collect();
                self.set_directory(Directory {
                    query,
                    state: DirectoryState::Ready,
                    people: people.items,
                    people_total: people.total,
                    channels: channels.items,
                    channels_total: channels.total,
                });
                self.want_avatars(owners);
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, query, "can't search the directory");
                self.set_directory(Directory {
                    state: DirectoryState::Failed((&error).into()),
                    ..Directory::searching(&query)
                });
            }
        }
    }

    /// Tells the frontend what the search holds now, unless it knows.
    fn set_directory(&mut self, directory: Directory) {
        if self.directory.told != directory {
            self.directory.told = directory.clone();
            (self.on_event)(Event::Directory(directory));
        }
    }

    /// Ends the search and the request, and clears what the frontend shows of it.
    fn clear_directory_search(&mut self) {
        self.abort_directory_request();
        self.directory.attempt += 1;
        self.set_directory(Directory::cleared());
    }

    fn abort_directory_request(&mut self) {
        self.directory.due = None;
        if let Some(task) = self.directory.task.take() {
            task.abort();
        }
    }

    /// Ends the search and the action under way, for a session that ends.
    pub(super) fn clear_directory(&mut self) {
        self.clear_directory_search();
        self.abort_directory_action();
    }

    /// Stops the tasks, for the actor to end.
    pub(super) fn abort_directory(&mut self) {
        self.abort_directory_request();
        self.abort_directory_action();
    }

    fn abort_directory_action(&mut self) {
        if let Some(running) = self.directory_action.take() {
            running.task.abort();
        }
    }

    /// Opens the direct message with `username`: the server makes it if there is none, it joins
    /// the room list, and it is opened.
    pub(super) fn open_direct(&mut self, username: String) {
        let action = DirectoryAction::OpenDirect(username.clone());
        self.begin_action(action, move |api, credentials| async move {
            let room = api.create_direct(&credentials, &username).await?;
            subscribed(&api, &credentials, &room).await
        });
    }

    /// Opens the channel as a preview, or as a room if the room list has it.
    pub(super) fn preview_channel(&mut self, room: RoomId) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("preview_channel ignored: not logged in");
            return;
        }
        if self.rooms.get(&room).is_some() || self.preview.as_ref().is_some_and(|p| p.id == room) {
            self.open_room(room);
            return;
        }
        let action = DirectoryAction::Preview(room.clone());
        self.begin_action(action, move |api, credentials| async move {
            let info = api.room_info(&credentials, &room).await?;
            room_from_info(info)
                .ok_or_else(|| ApiError::Unexpected(format!("{room} isn't a public channel")))
        });
    }

    /// Joins the channel, and lists it.
    pub(super) fn join_channel(&mut self, room: RoomId) {
        if self.rooms.get(&room).is_some() {
            tracing::debug!(%room, "join_channel ignored: already in the room list");
            return;
        }
        let action = DirectoryAction::Join(room.clone());
        self.begin_action(action, move |api, credentials| async move {
            api.join_channel(&credentials, &room).await?;
            subscribed(&api, &credentials, &room).await
        });
    }

    /// Starts `action`, unless the session is not logged in or another action is under way:
    /// `request` runs in a task and gives the room the action ends with.
    fn begin_action<F>(
        &mut self,
        action: DirectoryAction,
        request: impl FnOnce(C::Api, Credentials) -> F,
    ) where
        F: Future<Output = Result<Room, ApiError>> + Send + 'static,
    {
        let Some(session) = &self.session else {
            tracing::debug!(?action, "directory action ignored: not logged in");
            return;
        };
        if self.directory_action.is_some() {
            tracing::debug!(?action, "directory action ignored: another is under way");
            return;
        }
        self.directory_actions += 1;
        let attempt = self.directory_actions;
        let request = request(session.api.clone(), session.credentials.clone());
        let done = self.done_tx.clone();
        let task = tokio::spawn(async move {
            let outcome = request.await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::DirectoryAction(DirectoryActionDone {
                attempt,
                outcome,
            }));
        });
        self.directory_action = Some(RunningAction {
            attempt,
            action: action.clone(),
            task,
        });
        self.tell_action(action, ActionState::Working);
    }

    fn tell_action(&self, action: DirectoryAction, state: ActionState) {
        (self.on_event)(Event::DirectoryAction { action, state });
    }

    pub(super) fn handle_directory_action_done(&mut self, done: DirectoryActionDone) {
        let running = self
            .directory_action
            .take_if(|running| running.attempt == done.attempt);
        let (Some(RunningAction { action, .. }), true) =
            (running, matches!(self.state, SessionState::LoggedIn(_)))
        else {
            tracing::debug!("answer of a stale directory action dropped");
            return;
        };
        match done.outcome {
            Ok(room) => {
                match &action {
                    DirectoryAction::OpenDirect(_) => {
                        let id = room.id.clone();
                        self.list_room(room);
                        self.open_room(id);
                    }
                    DirectoryAction::Join(_) => self.list_room(room),
                    DirectoryAction::Preview(_) => self.start_preview(room),
                }
                self.tell_action(action, ActionState::Done);
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, ?action, "directory action refused");
                let state = ActionState::Failed(RoomActionError::from(&error));
                self.tell_action(action, state);
            }
        }
    }

    /// Puts a room the user has joined, or opened a direct message with, in the room list without
    /// waiting for the server's push, which then changes nothing. The user asked for it, so held
    /// news is shown with it.
    fn list_room(&mut self, room: Room) {
        let room = self.seen_if_open(room);
        let change = self.rooms.upsert_now(room, self.clock.now());
        self.emit_rooms(change.into_iter().collect());
    }

    /// Shows the channel as the open room, though the room list lacks it.
    fn start_preview(&mut self, room: Room) {
        let id = room.id.clone();
        self.want_avatars(room.avatar_to_fetch());
        self.preview = Some(room.clone());
        (self.on_event)(Event::Preview(Some(room)));
        self.open_room(id);
    }

    /// Ends the preview, if there is one, and tells the frontend.
    pub(super) fn end_preview(&mut self) {
        if self.preview.take().is_some() {
            (self.on_event)(Event::Preview(None));
        }
    }

    /// The preview is over once the room list has the channel: the user joined it, here or on
    /// another client.
    pub(super) fn end_preview_if_listed(&mut self) {
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| self.rooms.get(&preview.id).is_some())
        {
            self.end_preview();
        }
    }

    /// The user opened a room: a preview still on its way is not wanted any more, and the action
    /// that was after it is over.
    pub(super) fn cancel_preview_request(&mut self) {
        let Some(running) = self
            .directory_action
            .take_if(|running| matches!(running.action, DirectoryAction::Preview(_)))
        else {
            return;
        };
        running.task.abort();
        self.tell_action(running.action, ActionState::Done);
    }
}

/// The subscription of a room the user is in as the room list shows it. A room without one, or
/// one the list doesn't show, is not what was asked for.
async fn subscribed<A: ServerApi>(
    api: &A,
    credentials: &Credentials,
    room: &RoomId,
) -> Result<Room, ApiError> {
    api.subscription(credentials, room)
        .await?
        .and_then(room_from)
        .ok_or_else(|| ApiError::Unexpected(format!("{room} isn't in the user's rooms")))
}
