// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching a room's members for the composer's mention popup (see `docs/design/mentions.md`).
//! It works like the directory search: the same pause, the same dropping of stale answers.

use tokio::task::JoinHandle;

use crate::client::Event;
use crate::directory::Found;
use crate::domain::{AvatarOwner, RoomId, RoomKind};
use crate::members::{
    MEMBERS_PER_SEARCH, Member, MemberSearch, MemberSearchState, direct_members, without_user,
};
use crate::ports::{ApiError, Connector, ServerApi as _};
use crate::session::SessionState;

use super::{Actor, Done};

/// The search of the members: what the frontend was last told, and the wait and the request that
/// lead to the next thing to tell.
pub(super) struct MembersSearch {
    /// What the frontend was last told; none when there is no search. Its room and query are the
    /// ones being searched for.
    told: Option<MemberSearch>,
    /// Counts the searches and the cancellations, to tell the answer of the current search from a
    /// stale one.
    attempt: u64,
    /// How long the search waits for the typing to stop.
    debounce: jiff::SignedDuration,
    /// When the request is to be made, while the search waits for the typing to stop.
    due: Option<jiff::Timestamp>,
    /// The request, while it runs.
    task: Option<JoinHandle<()>>,
}

impl MembersSearch {
    pub(super) fn new(debounce: jiff::SignedDuration) -> Self {
        Self {
            told: None,
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

/// A members request's answer.
pub(super) struct MembersDone {
    /// The search the request belongs to: the answer is stale if another has begun since.
    attempt: u64,
    outcome: Result<Found<Member>, ApiError>,
}

impl<C: Connector> Actor<C> {
    /// Starts a search of `room`'s members for `query`, or retries one that failed. See
    /// [`AccountHandle::search_members`](crate::AccountHandle::search_members).
    pub(super) fn search_members(&mut self, room: RoomId, query: String) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("search_members ignored: not logged in");
            return;
        }
        if let Some(told) = &self.members.told
            && told.room == room
            && told.query == query
        {
            // The same search: the answer is coming, or is here. Only a failure is tried again, at
            // once, as the user asked for it.
            if matches!(told.state, MemberSearchState::Failed(_)) {
                self.begin_members_search(room, query, true);
            }
            return;
        }
        let direct = self
            .rooms
            .get(&room)
            .filter(|known| known.kind == RoomKind::Direct);
        if let Some(direct) = direct {
            // The server doesn't list a direct message's members: the other person is known
            // from the room.
            let peer = direct.peer.as_ref().map(|username| Member {
                id: String::new(),
                username: username.clone(),
                name: Some(direct.name.clone()).filter(|name| name != username),
            });
            let members = direct_members(peer, &query);
            self.abort_members_request();
            self.members.attempt += 1;
            self.tell_members(room, query, found(members));
            return;
        }
        self.begin_members_search(room, query, false);
    }

    /// Replaces the search by one for `query` in `room`, which asks the server at once or when the
    /// typing has stopped.
    fn begin_members_search(&mut self, room: RoomId, query: String, at_once: bool) {
        self.abort_members_request();
        self.members.attempt += 1;
        let at_once = at_once || self.members.debounce <= jiff::SignedDuration::ZERO;
        // The pause counts from before the event goes out: whoever hears it may move the clock on.
        if !at_once {
            self.members.due = self.clock.now().checked_add(self.members.debounce).ok();
        }
        self.tell_members(room, query, MemberSearchState::Searching);
        if at_once {
            self.request_members();
        }
    }

    /// Asks the server if the wait for the typing to stop is over.
    pub(super) fn start_due_members_search(&mut self) {
        if self.members.due.is_some_and(|due| due <= self.clock.now()) {
            self.request_members();
        }
    }

    /// Asks the server for the members, in a task that reports the answer.
    fn request_members(&mut self) {
        self.members.due = None;
        let (Some(session), Some(told)) = (&self.session, &self.members.told) else {
            return;
        };
        let attempt = self.members.attempt;
        let (room, query) = (told.room.clone(), told.query.clone());
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.members.task = Some(tokio::spawn(async move {
            let outcome = api
                .room_members(&credentials, &room, &query, MEMBERS_PER_SEARCH + 1)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Members(MembersDone { attempt, outcome }));
        }));
    }

    pub(super) fn handle_members_done(&mut self, done: MembersDone) {
        let Some(told) = self.members.told.clone() else {
            return;
        };
        if done.attempt != self.members.attempt || !matches!(self.state, SessionState::LoggedIn(_))
        {
            tracing::debug!("answer of a stale member search dropped");
            return;
        }
        self.members.task = None;
        match done.outcome {
            Ok(page) => {
                let page = match &self.state {
                    SessionState::LoggedIn(account) => {
                        without_user(page, &account.user, &told.query)
                    }
                    _ => page,
                };
                self.want_avatars(
                    (page.items.iter()).map(|member| AvatarOwner::User(member.username.clone())),
                );
                let state = MemberSearchState::Found {
                    members: page.items,
                    total: page.total,
                };
                self.tell_members(told.room, told.query, state);
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %told.room, query = told.query, "can't search the members");
                let state = MemberSearchState::Failed((&error).into());
                self.tell_members(told.room, told.query, state);
            }
        }
    }

    /// Tells the frontend what the search holds now, unless it knows.
    fn tell_members(&mut self, room: RoomId, query: String, state: MemberSearchState) {
        let search = MemberSearch { room, query, state };
        if self.members.told.as_ref() != Some(&search) {
            self.members.told = Some(search.clone());
            (self.on_event)(Event::MemberSearch(Some(search)));
        }
    }

    /// Ends the search and the request, and tells the frontend there is none, if there was.
    pub(super) fn cancel_members_search(&mut self) {
        self.abort_members_request();
        self.members.attempt += 1;
        if self.members.told.take().is_some() {
            (self.on_event)(Event::MemberSearch(None));
        }
    }

    fn abort_members_request(&mut self) {
        self.members.due = None;
        if let Some(task) = self.members.task.take() {
            task.abort();
        }
    }

    /// Stops the task, for the actor to end.
    pub(super) fn abort_members(&mut self) {
        self.abort_members_request();
    }
}

fn found(members: Vec<Member>) -> MemberSearchState {
    let total = u32::try_from(members.len()).unwrap_or(u32::MAX);
    MemberSearchState::Found { members, total }
}
