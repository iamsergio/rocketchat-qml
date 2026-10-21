// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The session actor driven through a fake server API and in-memory stores. Plain `#[test]`s: the
//! client owns its runtime and can't be dropped inside one. Nothing here touches the OS keyring.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rc_api::ServerVersion;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::*;
use crate::adapters::MemoryStore;
use crate::backoff::Backoff;
use crate::domain::{
    Account, Attachment, Author, Delivery, ImageAttachment, Message, MessageKind, Room, RoomKind,
    User,
};
use crate::policy::{ActionError, MessageAction, MessagePolicy};
use crate::ports::{
    ApiError, CacheError, Changes, Image, Page, Pushed, Realtime, Stored, image_extension,
};
use crate::rooms::RoomChange;
use crate::timeline::{TimelineChange, TimelineError, TimelineStatus};

const TIMEOUT: Duration = Duration::from_secs(5);
/// The wait before a reconnect for the tests that don't reconnect: longer than they run.
const NEVER: Duration = Duration::from_secs(3600);

struct LoginScript {
    /// Holds the login until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<(), ApiError>,
}

struct FetchScript {
    /// Holds the fetch until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<Vec<rc_api::Subscription>, ApiError>,
}

struct RealtimeScript {
    /// Holds the connection until the test notifies it.
    gate: Option<Arc<Notify>>,
    /// What is already waiting on the connection when it opens.
    initial: Vec<Pushed>,
    result: Result<(), ApiError>,
}

struct HistoryScript {
    /// Holds the fetch until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<Page, ApiError>,
}

struct ChangesScript {
    /// Holds the request until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<Changes, ApiError>,
}

struct SendScript {
    /// Holds the send until the test notifies it.
    gate: Option<Arc<Notify>>,
    /// The server's time of the stored message, in seconds after the epoch, or why it refused.
    result: Result<i64, ApiError>,
}

struct DeleteScript {
    /// Holds the deletion until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<(), ApiError>,
}

struct AvatarScript {
    /// Holds the download until the test notifies it.
    gate: Option<Arc<Notify>>,
    /// `None`: the owner has no avatar of its own.
    result: Result<Option<Image>, ApiError>,
}

struct FileScript {
    /// Holds the download until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<Image, ApiError>,
}

/// What `send_message` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SendCall {
    room: String,
    id: String,
    text: String,
}

/// What `edit_message` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
struct EditCall {
    room: String,
    id: String,
    text: String,
}

/// What `changes_since` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ChangesCall {
    room: String,
    since: jiff::Timestamp,
}

/// What `history` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HistoryCall {
    room: String,
    kind: RoomKind,
    before: Option<jiff::Timestamp>,
    count: usize,
}

#[derive(Default)]
struct FakeState {
    version: Option<Result<ServerVersion, ApiError>>,
    connect_error: Option<ApiError>,
    logins: VecDeque<LoginScript>,
    logout_result: Option<Result<(), ApiError>>,
    /// How long `logout` takes.
    logout_delay: Duration,
    me_result: Option<Result<User, ApiError>>,
    me_gate: Option<Arc<Notify>>,
    login_calls: Vec<(String, String)>,
    logout_calls: Vec<(String, String)>,
    /// Logouts that ran to the end.
    logouts_done: usize,
    me_calls: Vec<(String, String)>,
    /// One per room fetch; an empty room list once they run out.
    fetches: VecDeque<FetchScript>,
    subscriptions_calls: Vec<(String, String)>,
    /// What `custom_emoji` answers: no emoji if unset.
    emoji: Option<Result<Vec<rc_api::CustomEmoji>, ApiError>>,
    /// One per download of an owner's avatar; no avatar of its own once they run out.
    avatars: HashMap<AvatarOwner, VecDeque<AvatarScript>>,
    avatar_calls: Vec<AvatarOwner>,
    avatar_running: usize,
    /// The most downloads that ran at the same time.
    avatar_max_running: usize,
    /// One per download of a file by its path; a 404 once they run out.
    files: HashMap<String, VecDeque<FileScript>>,
    file_calls: Vec<String>,
    file_running: usize,
    /// The most file downloads that ran at the same time.
    file_max_running: usize,
    /// One per history fetch; an empty page at the start once they run out.
    histories: VecDeque<HistoryScript>,
    history_calls: Vec<HistoryCall>,
    /// One per catch-up of a room; no changes once they run out.
    changes: HashMap<String, VecDeque<ChangesScript>>,
    changes_calls: Vec<ChangesCall>,
    /// One per send; stored at second 1000 once they run out.
    sends: VecDeque<SendScript>,
    send_calls: Vec<SendCall>,
    /// Sends that got past their gate.
    sends_returned: usize,
    /// What `message_settings` answers: the defaults if unset.
    settings: Option<Result<rc_api::MessageSettings, ApiError>>,
    /// One per edit, with the server's time of the stored message as for the sends; refused as
    /// not allowed once they run out.
    edits: VecDeque<SendScript>,
    edit_calls: Vec<EditCall>,
    /// Edits that got past their gate.
    edits_returned: usize,
    /// One per deletion; a success once they run out.
    deletes: VecDeque<DeleteScript>,
    /// The `(room, id)` of each deletion asked for.
    delete_calls: Vec<(String, String)>,
    /// Deletions that got past their gate.
    deletes_returned: usize,
    /// One per mark-as-read call; a success once they run out.
    mark_reads: VecDeque<Result<(), ApiError>>,
    /// The rooms `mark_read` was asked for.
    mark_read_calls: Vec<String>,
    /// One per realtime connection; an open one with nothing waiting once they run out.
    realtimes: VecDeque<RealtimeScript>,
    realtime_calls: Vec<(String, String)>,
    /// When each of them came.
    realtime_times: Vec<Instant>,
    /// The server's end of every connection opened, in order. `None` once the test ended it.
    connections: Vec<Option<UnboundedSender<Pushed>>>,
    /// What each connection was told about the open room, not yet moved to `watch_log`.
    watches: Vec<UnboundedReceiver<Option<RoomId>>>,
    /// What each connection was told about the open room, in order.
    watch_log: Vec<Vec<Option<RoomId>>>,
}

/// Scripted results, plus a record of what it was asked.
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<FakeState>>);

impl Fake {
    fn new() -> Self {
        let fake = Self::default();
        fake.state().version = Some(Ok(ServerVersion { major: 8, minor: 0 }));
        fake.state().me_result = Some(Ok(user("alice")));
        fake
    }

    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }

    fn script_login(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state().logins.push_back(LoginScript { gate, result });
    }

    fn script_fetch(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<Vec<rc_api::Subscription>, ApiError>,
    ) {
        self.state().fetches.push_back(FetchScript { gate, result });
    }

    fn script_avatar(
        &self,
        owner: AvatarOwner,
        gate: Option<Arc<Notify>>,
        result: Result<Option<Image>, ApiError>,
    ) {
        self.state()
            .avatars
            .entry(owner)
            .or_default()
            .push_back(AvatarScript { gate, result });
    }

    /// The download tasks have started and are inside `avatar`.
    fn wait_for_avatar_calls(&self, count: usize) -> Vec<AvatarOwner> {
        wait_until(|| {
            let calls = self.state().avatar_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    fn script_file(&self, path: &str, gate: Option<Arc<Notify>>, result: Result<Image, ApiError>) {
        self.state()
            .files
            .entry(path.to_owned())
            .or_default()
            .push_back(FileScript { gate, result });
    }

    /// The download tasks have started and are inside `file`.
    fn wait_for_file_calls(&self, count: usize) -> Vec<String> {
        wait_until(|| {
            let calls = self.state().file_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    fn script_history(&self, gate: Option<Arc<Notify>>, result: Result<Page, ApiError>) {
        self.state()
            .histories
            .push_back(HistoryScript { gate, result });
    }

    fn script_changes(
        &self,
        room: &str,
        gate: Option<Arc<Notify>>,
        result: Result<Changes, ApiError>,
    ) {
        self.state()
            .changes
            .entry(room.to_owned())
            .or_default()
            .push_back(ChangesScript { gate, result });
    }

    /// The catch-up tasks of the room have started and are inside `changes_since`.
    fn wait_for_changes_calls(&self, room: &str, count: usize) -> Vec<ChangesCall> {
        wait_until(|| {
            let calls: Vec<_> = (self.state().changes_calls.iter())
                .filter(|call| call.room == room)
                .cloned()
                .collect();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// What connection `index` was told about the open room so far.
    fn watched(&self, index: usize) -> Vec<Option<RoomId>> {
        self.wait_for_connections(index + 1);
        let mut state = self.state();
        let state = &mut *state;
        state.watch_log.resize(state.watches.len(), Vec::new());
        while let Ok(room) = state.watches[index].try_recv() {
            state.watch_log[index].push(room);
        }
        state.watch_log[index].clone()
    }

    /// Waits until connection `index` has been told `count` times about the open room.
    fn wait_for_watched(&self, index: usize, count: usize) -> Vec<Option<RoomId>> {
        wait_until(|| {
            let log = self.watched(index);
            (log.len() >= count).then_some(log)
        })
    }

    fn script_send(&self, gate: Option<Arc<Notify>>, result: Result<i64, ApiError>) {
        self.state().sends.push_back(SendScript { gate, result });
    }

    fn script_edit(&self, gate: Option<Arc<Notify>>, result: Result<i64, ApiError>) {
        self.state().edits.push_back(SendScript { gate, result });
    }

    fn script_delete(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state()
            .deletes
            .push_back(DeleteScript { gate, result });
    }

    /// The edit tasks are done with the server and about to report.
    fn wait_for_edits_returned(&self, count: usize) {
        wait_until(|| (self.state().edits_returned >= count).then_some(()));
        // As for the sends: what the test does next must not overtake the report.
        std::thread::sleep(Duration::from_millis(20));
    }

    /// The deletion tasks are done with the server and about to report.
    fn wait_for_deletes_returned(&self, count: usize) {
        wait_until(|| (self.state().deletes_returned >= count).then_some(()));
        std::thread::sleep(Duration::from_millis(20));
    }

    fn script_mark_read(&self, result: Result<(), ApiError>) {
        self.state().mark_reads.push_back(result);
    }

    fn wait_for_mark_read_calls(&self, count: usize) -> Vec<String> {
        wait_until(|| {
            let calls = self.state().mark_read_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    fn script_realtime(
        &self,
        gate: Option<Arc<Notify>>,
        initial: Vec<Pushed>,
        result: Result<(), ApiError>,
    ) {
        self.state().realtimes.push_back(RealtimeScript {
            gate,
            initial,
            result,
        });
    }

    /// The realtime task has started and is inside `realtime`.
    fn wait_for_connections(&self, count: usize) {
        wait_until(|| (self.state().connections.len() >= count).then_some(()));
    }

    /// Pushes an event on connection `index`. False if the client dropped its end.
    fn push(&self, index: usize, event: Pushed) -> bool {
        self.wait_for_connections(index + 1);
        let state = self.state();
        state.connections[index]
            .as_ref()
            .is_some_and(|server| server.send(event).is_ok())
    }

    /// The server ends connection `index`.
    fn end_connection(&self, index: usize) {
        self.wait_for_connections(index + 1);
        self.state().connections[index] = None;
    }

    /// The client has dropped its end of connection `index`, which is what closes it.
    fn wait_for_connection_closed(&self, index: usize) {
        self.wait_for_connections(index + 1);
        wait_until(|| {
            let state = self.state();
            state.connections[index]
                .as_ref()
                .is_some_and(UnboundedSender::is_closed)
                .then_some(())
        });
    }

    /// The history fetch task has started and is inside `history`.
    fn wait_for_history_calls(&self, count: usize) -> Vec<HistoryCall> {
        wait_until(|| {
            let calls = self.state().history_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The send task has started and is inside `send_message`.
    fn wait_for_send_calls(&self, count: usize) -> Vec<SendCall> {
        wait_until(|| {
            let calls = self.state().send_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The send tasks are done with the server and about to report.
    fn wait_for_sends_returned(&self, count: usize) {
        wait_until(|| (self.state().sends_returned >= count).then_some(()));
        // A task counted may not have queued its report yet, and what the test sends next must
        // not overtake it.
        std::thread::sleep(Duration::from_millis(20));
    }

    /// The fetch task has started and is inside `subscriptions`.
    fn wait_for_subscriptions_calls(&self, count: usize) {
        wait_until(|| (self.state().subscriptions_calls.len() >= count).then_some(()));
    }

    fn login_calls(&self) -> Vec<(String, String)> {
        self.state().login_calls.clone()
    }

    fn wait_for_logout_call(&self) -> (String, String) {
        wait_until(|| self.state().logout_calls.first().cloned())
    }

    fn wait_for_me_call(&self) {
        wait_until(|| (!self.state().me_calls.is_empty()).then_some(()));
    }

    /// The login task has started and is inside `login`.
    fn wait_for_login_calls(&self, count: usize) {
        wait_until(|| (self.state().login_calls.len() >= count).then_some(()));
    }
}

/// Polls `check` until it returns a value.
fn wait_until<T>(check: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

impl Connector for Fake {
    type Api = Self;

    fn connect(&self, _server: &str) -> Result<Self, ApiError> {
        match self.state().connect_error.clone() {
            Some(error) => Err(error),
            None => Ok(self.clone()),
        }
    }
}

impl ServerApi for Fake {
    fn version(&self) -> impl Future<Output = Result<ServerVersion, ApiError>> + Send {
        std::future::ready(self.state().version.clone().unwrap())
    }

    async fn login(
        &self,
        username: &str,
        password: &SecretString,
    ) -> Result<(Credentials, User), ApiError> {
        let script = {
            let mut state = self.state();
            state
                .login_calls
                .push((username.to_owned(), password.expose_secret().to_owned()));
            state.logins.pop_front().unwrap()
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result?;
        Ok((
            Credentials {
                user_id: "uid".to_owned().into(),
                token: SecretString::from("tok"),
            },
            user(username),
        ))
    }

    async fn me(&self, credentials: &Credentials) -> Result<User, ApiError> {
        let gate = {
            let mut state = self.state();
            state.me_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.me_gate.clone()
        };
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().me_result.clone().unwrap()
    }

    async fn subscriptions(
        &self,
        credentials: &Credentials,
    ) -> Result<Vec<rc_api::Subscription>, ApiError> {
        let script = {
            let mut state = self.state();
            state.subscriptions_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.fetches.pop_front()
        };
        let Some(script) = script else {
            return Ok(Vec::new());
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    fn custom_emoji(
        &self,
        _credentials: &Credentials,
    ) -> impl Future<Output = Result<Vec<rc_api::CustomEmoji>, ApiError>> + Send {
        std::future::ready(self.state().emoji.clone().unwrap_or(Ok(Vec::new())))
    }

    fn message_settings(
        &self,
        _credentials: &Credentials,
    ) -> impl Future<Output = Result<rc_api::MessageSettings, ApiError>> + Send {
        let settings = self.state().settings.clone();
        std::future::ready(settings.unwrap_or_else(|| Ok(rc_api::MessageSettings::default())))
    }

    async fn avatar(
        &self,
        _credentials: &Credentials,
        owner: &AvatarOwner,
    ) -> Result<Option<Image>, ApiError> {
        struct Running(Fake);
        impl Drop for Running {
            fn drop(&mut self) {
                self.0.state().avatar_running -= 1;
            }
        }
        let script = {
            let mut state = self.state();
            state.avatar_calls.push(owner.clone());
            state.avatar_running += 1;
            state.avatar_max_running = state.avatar_max_running.max(state.avatar_running);
            state.avatars.get_mut(owner).and_then(VecDeque::pop_front)
        };
        let _running = Running(self.clone());
        let Some(script) = script else {
            return Ok(None);
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    async fn file(&self, _credentials: &Credentials, path: &str) -> Result<Image, ApiError> {
        struct Running(Fake);
        impl Drop for Running {
            fn drop(&mut self) {
                self.0.state().file_running -= 1;
            }
        }
        let script = {
            let mut state = self.state();
            state.file_calls.push(path.to_owned());
            state.file_running += 1;
            state.file_max_running = state.file_max_running.max(state.file_running);
            state.files.get_mut(path).and_then(VecDeque::pop_front)
        };
        let _running = Running(self.clone());
        let Some(script) = script else {
            return Err(ApiError::Server {
                status: 404,
                message: "no such file".to_owned(),
            });
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    async fn history(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        kind: RoomKind,
        before: Option<jiff::Timestamp>,
        count: usize,
    ) -> Result<Page, ApiError> {
        let script = {
            let mut state = self.state();
            state.history_calls.push(HistoryCall {
                room: room.to_string(),
                kind,
                before,
                count,
            });
            state.histories.pop_front()
        };
        let Some(script) = script else {
            return Ok(Page {
                messages: Vec::new(),
                reached_start: true,
            });
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    async fn changes_since(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        since: jiff::Timestamp,
    ) -> Result<Changes, ApiError> {
        let script = {
            let mut state = self.state();
            state.changes_calls.push(ChangesCall {
                room: room.to_string(),
                since,
            });
            state
                .changes
                .get_mut(room.as_str())
                .and_then(VecDeque::pop_front)
        };
        let Some(script) = script else {
            return Ok(Changes::default());
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    async fn send_message(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
    ) -> Result<Message, ApiError> {
        let script = {
            let mut state = self.state();
            state.send_calls.push(SendCall {
                room: room.to_string(),
                id: id.to_string(),
                text: text.to_owned(),
            });
            state.sends.pop_front()
        };
        let (gate, result) = script.map_or((None, Ok(1_000)), |s| (s.gate, s.result));
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().sends_returned += 1;
        Ok(server_copy(room.as_str(), id.as_str(), text, result?))
    }

    async fn edit_message(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
    ) -> Result<Message, ApiError> {
        let script = {
            let mut state = self.state();
            state.edit_calls.push(EditCall {
                room: room.to_string(),
                id: id.to_string(),
                text: text.to_owned(),
            });
            state.edits.pop_front()
        };
        let (gate, result) = script.map_or_else(
            || {
                let refused = ApiError::Server {
                    status: 400,
                    message: "error-action-not-allowed".to_owned(),
                };
                (None, Err(refused))
            },
            |s| (s.gate, s.result),
        );
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().edits_returned += 1;
        let stored = server_copy(room.as_str(), id.as_str(), text, result?);
        Ok(Message {
            edited: true,
            ..stored
        })
    }

    async fn delete_message(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
    ) -> Result<(), ApiError> {
        let script = {
            let mut state = self.state();
            state.delete_calls.push((room.to_string(), id.to_string()));
            state.deletes.pop_front()
        };
        let (gate, result) = script.map_or((None, Ok(())), |s| (s.gate, s.result));
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().deletes_returned += 1;
        result
    }

    fn mark_read(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
    ) -> impl Future<Output = Result<(), ApiError>> + Send {
        let mut state = self.state();
        state.mark_read_calls.push(room.to_string());
        std::future::ready(state.mark_reads.pop_front().unwrap_or(Ok(())))
    }

    async fn realtime(&self, credentials: &Credentials) -> Result<Realtime, ApiError> {
        let (server, client) = unbounded_channel();
        let (watch, watched) = unbounded_channel();
        let realtime = Realtime {
            events: client,
            watch,
        };
        let script = {
            let mut state = self.state();
            state.realtime_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.realtime_times.push(Instant::now());
            state.connections.push(Some(server.clone()));
            state.watches.push(watched);
            state.realtimes.pop_front()
        };
        let Some(script) = script else {
            return Ok(realtime);
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result?;
        for event in script.initial {
            let _ = server.send(event);
        }
        Ok(realtime)
    }

    async fn logout(&self, credentials: &Credentials) -> Result<(), ApiError> {
        let delay = {
            let mut state = self.state();
            state.logout_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.logout_delay
        };
        tokio::time::sleep(delay).await;
        let mut state = self.state();
        state.logouts_done += 1;
        state.logout_result.clone().unwrap_or(Ok(()))
    }
}

/// Files in memory, at made-up paths: `/cache/<server>/<user or room>/<name>.<extension>`.
#[derive(Clone, Default)]
struct FakeCache(Arc<Mutex<FakeFiles>>);

/// The extension and the bytes of each file.
type FakeFiles = HashMap<CacheKey, (&'static str, Vec<u8>)>;

impl FakeCache {
    /// A file held before the session starts. `server` is the key's, `h` for `http://h`.
    fn seed(&self, server: &str, owner: &AvatarOwner, extension: &'static str, bytes: &[u8]) {
        let key = CacheKey::avatar(server, owner);
        self.0
            .lock()
            .unwrap()
            .insert(key, (extension, bytes.to_vec()));
    }

    fn bytes(&self, server: &str, owner: &AvatarOwner) -> Option<Vec<u8>> {
        let key = CacheKey::avatar(server, owner);
        self.0
            .lock()
            .unwrap()
            .get(&key)
            .map(|(_, bytes)| bytes.clone())
    }

    /// A file held before the session starts, as an attachment image.
    fn seed_attachment(&self, server: &str, path: &str, extension: &'static str, bytes: &[u8]) {
        let key = CacheKey::attachment(server, path);
        self.0
            .lock()
            .unwrap()
            .insert(key, (extension, bytes.to_vec()));
    }

    fn attachment_bytes(&self, server: &str, path: &str) -> Option<Vec<u8>> {
        let key = CacheKey::attachment(server, path);
        self.0
            .lock()
            .unwrap()
            .get(&key)
            .map(|(_, bytes)| bytes.clone())
    }

    fn path(key: &CacheKey, extension: &str) -> PathBuf {
        let kind = match key.kind {
            crate::ports::CacheKind::UserAvatar => "user",
            crate::ports::CacheKind::RoomAvatar => "room",
            crate::ports::CacheKind::Attachment => "attachment",
        };
        PathBuf::from(format!(
            "/cache/{}/{kind}/{}.{extension}",
            key.server, key.name
        ))
    }
}

impl FileCache for FakeCache {
    fn get(&self, key: &CacheKey) -> Option<PathBuf> {
        let files = self.0.lock().unwrap();
        files
            .get(key)
            .map(|(extension, _)| Self::path(key, extension))
    }

    fn put(
        &self,
        key: &CacheKey,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<Stored, CacheError> {
        let extension = content_type
            .and_then(image_extension)
            .ok_or_else(|| CacheError::UnsupportedType(content_type.map(str::to_owned)))?;
        let mut files = self.0.lock().unwrap();
        let new = (extension, bytes.to_vec());
        let changed = files.get(key) != Some(&new);
        files.insert(key.clone(), new);
        Ok(Stored {
            path: Self::path(key, extension),
            changed,
        })
    }

    fn remove(&self, key: &CacheKey) -> Result<bool, CacheError> {
        Ok(self.0.lock().unwrap().remove(key).is_some())
    }
}

/// A memory store that can be told to fail.
#[derive(Clone, Default)]
struct FakeStore {
    inner: MemoryStore,
    fail_load: bool,
    fail_save: bool,
}

impl SecretStore for FakeStore {
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        if self.fail_load {
            return Err(StoreError::Unavailable("locked".into()));
        }
        self.inner.load()
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        if self.fail_save {
            return Err(StoreError::Unavailable("locked".into()));
        }
        self.inner.save(session)
    }

    fn clear(&self) -> Result<(), StoreError> {
        self.inner.clear()
    }
}

fn user(username: &str) -> User {
    User {
        id: "uid".to_owned().into(),
        username: username.to_owned(),
        display_name: Some("Display".to_owned()),
    }
}

fn subscription(id: &str, name: &str, room_type: rc_api::RoomType) -> rc_api::Subscription {
    rc_api::Subscription {
        room_id: id.to_owned(),
        room_type,
        name: name.to_owned(),
        display_name: None,
        unread: 0,
        user_mentions: 0,
        group_mentions: 0,
        alert: false,
        open: true,
        created: None,
        last_message: None,
        avatar_etag: None,
    }
}

fn room(id: &str, name: &str, kind: RoomKind) -> Room {
    Room {
        id: id.to_owned().into(),
        kind,
        name: name.to_owned(),
        // A direct message is called after the other user.
        peer: (kind == RoomKind::Direct).then(|| name.to_owned()),
        avatar_etag: None,
        unread: 0,
        user_mentions: 0,
        group_mentions: 0,
        alert: false,
        last_activity: jiff::Timestamp::UNIX_EPOCH,
    }
}

fn session_event(state: SessionState) -> Event {
    Event::Session(state)
}

fn rooms_event(rooms: Vec<Room>) -> Event {
    Event::Rooms(vec![RoomChange::Reset(rooms)])
}

fn logged_out(failure: Option<LoginFailure>) -> SessionState {
    SessionState::LoggedOut { failure }
}

fn logged_in(server: &str, username: &str) -> SessionState {
    SessionState::LoggedIn(Account {
        server: server.to_owned(),
        user: user(username),
    })
}

fn stored(server: &str) -> StoredSession {
    StoredSession {
        server: server.to_owned(),
        credentials: Credentials {
            user_id: "uid".to_owned().into(),
            token: SecretString::from("stored-tok"),
        },
    }
}

/// A store that already holds a session.
fn store_with(server: &str) -> MemoryStore {
    let store = MemoryStore::default();
    store.save(&stored(server)).unwrap();
    store
}

fn wait_for_empty(store: &MemoryStore) {
    wait_until(|| store.load().unwrap().is_none().then_some(()));
}

struct Harness {
    client: Client,
    events: Receiver<Event>,
    /// The first event, which every client sends.
    first: SessionState,
}

impl Harness {
    fn new(fake: &Fake) -> Self {
        Self::with_store(fake, MemoryStore::default())
    }

    fn with_store(fake: &Fake, store: impl SecretStore) -> Self {
        Self::with_backoff(fake, store, Backoff::new(NEVER, NEVER, NEVER))
    }

    fn with_backoff(fake: &Fake, store: impl SecretStore, backoff: Backoff) -> Self {
        Self::build(fake, store, FakeCache::default(), backoff)
    }

    fn with_cache(fake: &Fake, cache: FakeCache) -> Self {
        Self::with_cache_and_backoff(fake, cache, Backoff::new(NEVER, NEVER, NEVER))
    }

    fn with_cache_and_backoff(fake: &Fake, cache: FakeCache, backoff: Backoff) -> Self {
        Self::build(fake, MemoryStore::default(), cache, backoff)
    }

    fn build(fake: &Fake, store: impl SecretStore, cache: FakeCache, backoff: Backoff) -> Self {
        let (tx, events) = mpsc::channel();
        let client = Client::with_backoff(
            fake.clone(),
            store,
            cache,
            move |event| {
                let _ = tx.send(event);
            },
            backoff,
        )
        .unwrap();
        let mut harness = Self {
            client,
            events,
            first: SessionState::Connecting,
        };
        harness.first = harness.next();
        harness
    }

    fn login(&self, server: &str, username: &str) {
        self.client
            .login(server, username, SecretString::from("secret"));
    }

    /// The next event, connection changes aside: the tests that don't look at them don't wait for
    /// them either. See [`next_connection`](Self::next_connection).
    fn next_event(&self) -> Event {
        loop {
            match self.events.recv_timeout(TIMEOUT).expect("no event") {
                Event::Connection(_) => {}
                event => return event,
            }
        }
    }

    /// The updates of the next `Avatars` event, skipping the other events.
    fn next_avatars(&self) -> Vec<AvatarUpdate> {
        loop {
            if let Event::Avatars(updates) = self.events.recv_timeout(TIMEOUT).expect("no event") {
                return updates;
            }
        }
    }

    /// Checks that no `Avatars` event comes for a little while, and drops the other events.
    fn assert_no_avatars(&self) {
        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(Event::Avatars(updates)) = self.events.recv_timeout(left) {
                panic!("unexpected avatars: {updates:?}");
            }
        }
    }

    /// The updates of the next `Images` event, skipping the other events.
    fn next_images(&self) -> Vec<ImageUpdate> {
        loop {
            if let Event::Images(updates) = self.events.recv_timeout(TIMEOUT).expect("no event") {
                return updates;
            }
        }
    }

    /// Checks that no `Images` event comes for a little while, and drops the other events.
    fn assert_no_images(&self) {
        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(Event::Images(updates)) = self.events.recv_timeout(left) {
                panic!("unexpected images: {updates:?}");
            }
        }
    }

    /// Skips events until the open room's messages gain the one with this id.
    fn wait_for_appended(&self, id: &str) {
        loop {
            if let Event::Timeline(changes) = self.events.recv_timeout(TIMEOUT).expect("no event")
                && changes.iter().any(|change| {
                    matches!(change, TimelineChange::Appended(messages)
                        if messages.iter().any(|m| m.id.as_str() == id))
                })
            {
                return;
            }
        }
    }

    /// The next connection change, skipping the other events.
    fn next_connection(&self) -> Connection {
        loop {
            if let Event::Connection(connection) =
                self.events.recv_timeout(TIMEOUT).expect("no event")
            {
                return connection;
            }
        }
    }

    /// The next session state. Room events in between are skipped: the session tests don't care
    /// about them, the room tests use [`next_event`](Self::next_event).
    fn next(&self) -> SessionState {
        loop {
            if let Event::Session(state) = self.next_event() {
                return state;
            }
        }
    }

    /// Checks the next session states, in order.
    fn expect(&self, states: &[SessionState]) {
        for expected in states {
            assert_eq!(&self.next(), expected);
        }
    }

    /// Checks the next events, in order, room events included.
    fn expect_events(&self, events: Vec<Event>) {
        for expected in events {
            assert_eq!(self.next_event(), expected);
        }
    }
}

fn failing_login(failure: ApiError, expected: LoginFailure) {
    let fake = Fake::new();
    fake.script_login(None, Err(failure));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[SessionState::Connecting, logged_out(Some(expected))]);
}

// AUTH-001
#[test]
fn login_succeeds() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login(" chat.example.com ", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    assert_eq!(
        fake.login_calls(),
        [("alice".to_owned(), "secret".to_owned())]
    );
}

// AUTH-002
#[test]
fn invalid_credentials_are_reported() {
    failing_login(
        ApiError::InvalidCredentials,
        LoginFailure::InvalidCredentials,
    );
}

// AUTH-003
#[test]
fn invalid_url_is_reported() {
    let fake = Fake::new();
    fake.state().connect_error = Some(ApiError::InvalidUrl("x".into()));
    let harness = Harness::new(&fake);
    harness.login("x", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidUrl)),
    ]);
}

// AUTH-003
#[test]
fn unreachable_server_is_reported() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::Unreachable)),
    ]);
}

// AUTH-003
#[test]
fn non_rocketchat_server_is_reported() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::NotRocketChat));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::NotRocketChat)),
    ]);
}

// AUTH-003
#[test]
fn old_server_is_refused_without_logging_in() {
    let fake = Fake::new();
    fake.state().version = Some(Ok(ServerVersion { major: 7, minor: 9 }));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    let failure = LoginFailure::UnsupportedVersion {
        found: "7.9".into(),
        minimum: "8.0".into(),
    };
    harness.expect(&[SessionState::Connecting, logged_out(Some(failure))]);
    assert!(fake.login_calls().is_empty());
}

// AUTH-003
#[test]
fn server_errors_are_reported_with_their_message() {
    let error = ApiError::Server {
        status: 500,
        message: "boom".into(),
    };
    let expected = LoginFailure::Server(error.to_string());
    assert!(expected.to_string().contains("boom"));
    failing_login(error, expected);
}

// AUTH-005
#[test]
fn logout_revokes_the_token() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "tok".to_owned())
    );
}

// AUTH-005
#[test]
fn logout_ends_logged_out_even_if_the_server_call_fails() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().logout_result = Some(Err(ApiError::Unauthorized));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    fake.wait_for_logout_call();
}

// AUTH-005
#[test]
fn logout_while_connecting_cancels_the_login() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(Some(Arc::clone(&gate)), Ok(()));
    fake.script_login(None, Err(ApiError::InvalidCredentials));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting]);
    // Cancel only once the attempt is really waiting at the gate.
    fake.wait_for_login_calls(1);

    harness.client.logout();
    harness.expect(&[logged_out(None)]);

    // The cancelled attempt must not surface, whatever it does now.
    gate.notify_one();
    harness.login("http://h", "bob");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidCredentials)),
    ]);
}

// AUTH-001
#[test]
fn login_while_connecting_is_ignored() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(Some(Arc::clone(&gate)), Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.login("http://h", "bob");
    harness.expect(&[SessionState::Connecting]);

    gate.notify_one();
    harness.expect(&[logged_in("http://h", "alice")]);
    // Nothing extra came in between: the next event is the logout's.
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    assert_eq!(fake.login_calls().len(), 1);
}

// AUTH-002
#[test]
fn a_new_login_clears_the_old_failure() {
    let fake = Fake::new();
    fake.script_login(None, Err(ApiError::InvalidCredentials));
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidCredentials)),
    ]);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn an_empty_store_starts_logged_out() {
    let harness = Harness::new(&Fake::new());
    assert_eq!(harness.first, logged_out(None));
}

// AUTH-004
#[test]
fn a_stored_session_is_resumed_without_logging_in() {
    let fake = Fake::new();
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect(&[logged_in("http://h", "alice")]);
    assert_eq!(
        fake.state().me_calls,
        [("uid".to_owned(), "stored-tok".to_owned())]
    );
    assert!(fake.login_calls().is_empty());
}

// AUTH-004
#[test]
fn a_rejected_stored_session_expires_and_is_forgotten() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_empty(&store);
}

// AUTH-004
#[test]
fn a_login_after_a_failed_resume_is_connecting_not_resuming() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    fake.script_login(None, Ok(()));
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn an_unreachable_server_keeps_the_stored_session() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::Unreachable))]);
    // The state is final before any store write would have been queued; flush the queue by
    // dropping the client.
    drop(harness);
    assert!(store.load().unwrap().is_some());
}

// AUTH-004
#[test]
fn a_server_that_is_now_too_old_keeps_the_stored_session() {
    let fake = Fake::new();
    fake.state().version = Some(Ok(ServerVersion { major: 7, minor: 9 }));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    let failure = LoginFailure::UnsupportedVersion {
        found: "7.9".into(),
        minimum: "8.0".into(),
    };
    harness.expect(&[logged_out(Some(failure))]);
    drop(harness);
    assert!(store.load().unwrap().is_some());
    assert!(fake.state().me_calls.is_empty());
}

// AUTH-004
#[test]
fn a_store_that_cant_be_read_starts_logged_out_and_logins_still_work() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = FakeStore {
        fail_load: true,
        ..FakeStore::default()
    };
    let harness = Harness::with_store(&fake, store);
    assert_eq!(harness.first, logged_out(None));
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn a_session_survives_a_restart() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();

    let first = Harness::with_store(&fake, store.clone());
    first.login("http://h", "alice");
    first.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    // Dropping flushes the queued save.
    drop(first);

    let second = Harness::with_store(&fake, store);
    assert_eq!(second.first, SessionState::Resuming);
    second.expect(&[logged_in("http://h", "alice")]);
    assert_eq!(fake.login_calls().len(), 1);
}

// AUTH-001
#[test]
fn login_saves_the_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("chat.example.com", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    let saved = wait_until(|| store.load().unwrap());
    assert_eq!(saved.server, "https://chat.example.com");
    assert_eq!(saved.credentials.user_id.as_str(), "uid");
    assert_eq!(saved.credentials.token.expose_secret(), "tok");
}

// AUTH-001
#[test]
fn a_store_that_cant_be_written_does_not_stop_the_login() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = FakeStore {
        fail_save: true,
        ..FakeStore::default()
    };
    let harness = Harness::with_store(&fake, store);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-005
#[test]
fn logout_forgets_the_stored_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    // The save and the clear are applied in order, so the store ends up empty.
    wait_for_empty(&store);
    fake.wait_for_logout_call();
}

// AUTH-005
#[test]
fn dropping_the_client_after_logout_finishes_the_logout() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().logout_delay = Duration::from_millis(200);
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);

    drop(harness);

    assert_eq!(fake.state().logouts_done, 1);
    assert!(store.load().unwrap().is_none());
}

// AUTH-005
#[test]
fn logout_while_resuming_cancels_and_forgets() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.state().me_gate = Some(Arc::clone(&gate));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    assert_eq!(harness.first, SessionState::Resuming);
    fake.wait_for_me_call();

    harness.client.logout();

    harness.expect(&[logged_out(None)]);
    wait_for_empty(&store);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "stored-tok".to_owned())
    );
}

// ROOM-001
#[test]
fn login_lists_the_rooms_sorted() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("2", "random", rc_api::RoomType::Channel),
            subscription("3", "bob", rc_api::RoomType::Direct),
            subscription("1", "General", rc_api::RoomType::Channel),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            room("3", "bob", RoomKind::Direct),
            room("1", "General", RoomKind::Channel),
            room("2", "random", RoomKind::Channel),
        ]),
    ]);
    assert_eq!(
        fake.state().subscriptions_calls,
        [("uid".to_owned(), "tok".to_owned())]
    );
}

// ROOM-001
#[test]
fn hidden_and_unlisted_rooms_are_not_in_the_list() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let mut hidden = subscription("h", "hidden", rc_api::RoomType::Channel);
    hidden.open = false;
    fake.script_fetch(
        None,
        Ok(vec![
            hidden,
            subscription("l", "livechat", rc_api::RoomType::Other("l".into())),
            subscription("p", "private", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("p", "private", RoomKind::Private)]),
    ]);
}

fn party() -> rc_api::CustomEmoji {
    rc_api::CustomEmoji {
        name: "party".into(),
        aliases: vec!["celebrate".into()],
        extension: "gif".into(),
    }
}

fn party_emoji(server: &str) -> CustomEmojis {
    CustomEmojis::new(server, &[party()])
}

// RICH-003
#[test]
fn login_fetches_the_custom_emoji_after_the_rooms() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().emoji = Some(Ok(vec![party()]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
        Event::CustomEmoji(party_emoji("http://h")),
    ]);
}

// RICH-003
#[test]
fn a_server_without_custom_emoji_sends_no_event_for_them() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    assert!(
        harness
            .events
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
}

// RICH-003
#[test]
fn logout_forgets_the_custom_emoji() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().emoji = Some(Ok(vec![party()]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
        Event::CustomEmoji(party_emoji("http://h")),
    ]);
    harness.client.logout();
    harness.expect_events(vec![
        session_event(logged_out(None)),
        Event::CustomEmoji(CustomEmojis::default()),
    ]);
}

// RICH-003
#[test]
fn a_failed_custom_emoji_fetch_leaves_the_session_alone() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().emoji = Some(Err(ApiError::Unreachable("down".into())));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
}

// ROOM-001
#[test]
fn a_resumed_session_lists_the_rooms() {
    let fake = Fake::new();
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect_events(vec![
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    assert_eq!(
        fake.state().subscriptions_calls,
        [("uid".to_owned(), "stored-tok".to_owned())]
    );
}

// ROOM-001
#[test]
fn logout_empties_the_room_list() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
}

// AUTH-004
#[test]
fn a_token_that_dies_before_the_fetch_expires_the_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Err(ApiError::Unauthorized));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
    ]);
    wait_for_empty(&store);
    // Dropping flushes the actor's work: a revoke would show by now.
    drop(harness);
    assert!(fake.state().logout_calls.is_empty());
}

// ROOM-001
#[test]
fn a_failed_fetch_leaves_the_session_logged_in_without_rooms() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Err(ApiError::Unreachable("refused".into())));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    fake.wait_for_subscriptions_calls(1);
    // No rooms event so far, and none for the empty list on logout: the next event is the
    // logout's own, and the one after it is the next login's.
    harness.client.logout();
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(logged_out(None)),
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
}

// ROOM-001
#[test]
fn a_fetch_that_finishes_after_logout_is_dropped() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(None, Ok(()));
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        Some(Arc::clone(&gate)),
        Ok(vec![subscription("old", "old", rc_api::RoomType::Channel)]),
    );
    fake.script_fetch(
        None,
        Ok(vec![subscription("new", "new", rc_api::RoomType::Channel)]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    fake.wait_for_subscriptions_calls(1);

    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    gate.notify_one();

    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("new", "new", RoomKind::Channel)]),
    ]);
}

fn message(id: &str, secs: i64) -> Message {
    Message {
        id: id.to_owned().into(),
        room_id: "1".to_owned().into(),
        author: Author {
            id: "uid".to_owned().into(),
            username: "alice".to_owned(),
            display_name: None,
        },
        ts: jiff::Timestamp::from_second(secs).unwrap(),
        kind: MessageKind::Text(id.into()),
        delivery: Delivery::Sent,
        attachments: Vec::new(),
        edited: false,
        reactions: Vec::new(),
    }
}

/// The message as the server stores it after `alice` sent it.
fn server_copy(room: &str, id: &str, text: &str, secs: i64) -> Message {
    Message {
        room_id: room.to_owned().into(),
        kind: MessageKind::Text(text.into()),
        ..message(id, secs)
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn page(messages: Vec<Message>, reached_start: bool) -> Result<Page, ApiError> {
    Ok(Page {
        messages,
        reached_start,
    })
}

fn reset_event(room: Option<&str>, messages: Vec<Message>) -> Event {
    Event::Timeline(vec![TimelineChange::Reset {
        room: room.map(|id| id.to_owned().into()),
        messages,
    }])
}

fn status(
    room: &str,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error: Option<TimelineError>,
) -> Event {
    Event::TimelineStatus(TimelineStatus {
        room: Some(room.to_owned().into()),
        loading,
        loading_older,
        reached_start,
        error,
    })
}

/// The event that closes the open room: no messages, no room.
fn closed_events() -> Vec<Event> {
    vec![
        reset_event(None, Vec::new()),
        Event::TimelineStatus(TimelineStatus::default()),
    ]
}

/// A harness logged in with the rooms `1` (a channel) and `2` (a private group), past their events.
fn logged_in_harness(fake: &Fake) -> Harness {
    logged_in_harness_with(fake, Backoff::new(NEVER, NEVER, NEVER))
}

fn logged_in_harness_with(fake: &Fake, backoff: Backoff) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::with_backoff(fake, MemoryStore::default(), backoff);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            room("1", "general", RoomKind::Channel),
            room("2", "secret", RoomKind::Private),
        ]),
    ]);
    harness
}

/// Opens room 1 and takes the events of its first page, which has `a` and `b`.
fn open_first_room(fake: &Fake, harness: &Harness, reached_start: bool) {
    fake.script_history(
        None,
        page(vec![message("b", 2), message("a", 1)], reached_start),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", false, false, reached_start, None),
    ]);
}

// MSG-001
#[test]
fn opening_a_room_shows_the_newest_messages_oldest_first() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert_eq!(
        fake.state().history_calls,
        [HistoryCall {
            room: "1".into(),
            kind: RoomKind::Channel,
            before: None,
            count: crate::timeline::PAGE_SIZE,
        }]
    );
}

// MSG-001
#[test]
fn the_kind_of_the_room_is_passed_to_the_server() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert_eq!(fake.state().history_calls[0].kind, RoomKind::Private);
}

// MSG-001
#[test]
fn opening_an_unknown_room_or_when_logged_out_is_ignored() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    // Logged out.
    harness.client.open_room("1".to_owned().into());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    // Logged in, but not in the room list.
    harness.client.open_room("nope".to_owned().into());
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    drop(harness);
    assert!(fake.state().history_calls.is_empty());
}

// MSG-002
#[test]
fn loading_older_messages_prepends_them() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    // The page starts at the oldest message held, which comes back as the duplicate.
    fake.script_history(None, page(vec![message("a", 1), message("z", 0)], true));
    harness.client.load_older();
    harness.expect_events(vec![
        status("1", false, true, false, None),
        Event::Timeline(vec![TimelineChange::Prepended(vec![message("z", 0)])]),
        status("1", false, false, true, None),
    ]);
    let calls = fake.state().history_calls.clone();
    assert_eq!(calls[1].before, Some(message("a", 1).ts));
}

// MSG-002
#[test]
fn loading_older_is_ignored_while_a_fetch_is_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(Arc::clone(&gate)), page(vec![message("z", 0)], false));
    harness.client.load_older();
    harness.expect_events(vec![status("1", false, true, false, None)]);
    fake.wait_for_history_calls(2);

    harness.client.load_older();
    harness.client.load_older();
    gate.notify_one();
    harness.expect_events(vec![
        Event::Timeline(vec![TimelineChange::Prepended(vec![message("z", 0)])]),
        status("1", false, false, false, None),
    ]);
    // Dropping flushes the actor's work: a second fetch would have been made by now.
    drop(harness);
    assert_eq!(fake.state().history_calls.len(), 2);
}

// MSG-002
#[test]
fn loading_older_is_ignored_once_the_start_is_reached() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    harness.client.load_older();
    drop(harness);
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-002
#[test]
fn loading_older_without_an_open_room_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.load_older();
    drop(harness);
    assert!(fake.state().history_calls.is_empty());
}

// MSG-001
#[test]
fn opening_the_open_room_again_does_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.open_room("1".to_owned().into());
    drop(harness);
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-001
#[test]
fn switching_rooms_replaces_the_messages_and_drops_the_stale_answer() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(Arc::clone(&gate)), page(vec![message("a", 1)], true));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);

    let mut other = message("x", 9);
    other.room_id = "2".to_owned().into();
    fake.script_history(None, page(vec![other.clone()], true));
    harness.client.open_room("2".to_owned().into());
    gate.notify_one();
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        reset_event(Some("2"), vec![other]),
        status("2", false, false, true, None),
    ]);
    // Nothing of room 1 follows: closing the room is the next event.
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
}

// MSG-001
#[test]
fn reopening_a_room_shows_what_was_loaded_first() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);

    // A message arrived meanwhile.
    fake.script_history(
        None,
        page(
            vec![message("c", 3), message("b", 2), message("a", 1)],
            false,
        ),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        Event::Timeline(vec![TimelineChange::Appended(vec![message("c", 3)])]),
        status("1", false, false, false, None),
    ]);
}

// MSG-001
#[test]
fn logout_closes_the_room_and_forgets_the_messages() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());

    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    // Nothing is held any more: the room opens empty.
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
}

// MSG-001
#[test]
fn an_expired_token_ends_the_session_and_closes_the_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_history(None, Err(ApiError::Unauthorized));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
    // It's dead already, so there's nothing to revoke.
    drop(harness);
    assert!(fake.state().logout_calls.is_empty());
}

// MSG-001
#[test]
fn a_failed_fetch_shows_the_error_and_opening_again_retries() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_history(None, Err(ApiError::Unreachable("refused".into())));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        status("1", false, false, false, Some(TimelineError::Unreachable)),
    ]);
    // Older messages can't be loaded when there are none.
    harness.client.load_older();

    fake.script_history(None, page(vec![message("a", 1)], true));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("a", 1)]),
        status("1", false, false, true, None),
    ]);
    assert_eq!(fake.state().history_calls.len(), 2);
}

// MSG-002
#[test]
fn a_failed_older_fetch_keeps_the_messages() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_history(
        None,
        Err(ApiError::Server {
            status: 500,
            message: "boom".into(),
        }),
    );
    harness.client.load_older();
    harness.expect_events(vec![
        status("1", false, true, false, None),
        status(
            "1",
            false,
            false,
            false,
            Some(TimelineError::Server(
                "the server answered 500: boom".into(),
            )),
        ),
    ]);
    // Trying again clears the error.
    fake.script_history(None, page(vec![message("z", 0)], true));
    harness.client.load_older();
    harness.expect_events(vec![
        status("1", false, true, false, None),
        Event::Timeline(vec![TimelineChange::Prepended(vec![message("z", 0)])]),
        status("1", false, false, true, None),
    ]);
}

/// A message of room `room`, pushed by the server.
fn pushed_message(room: &str, id: &str, secs: i64) -> Pushed {
    let mut message = message(id, secs);
    message.room_id = room.to_owned().into();
    Pushed::Message(message)
}

fn pushed_subscription(id: &str, name: &str, room_type: rc_api::RoomType) -> Pushed {
    Pushed::Subscription(subscription(id, name, room_type))
}

fn removed(id: &str) -> Pushed {
    Pushed::SubscriptionRemoved(id.to_owned().into())
}

fn rooms_change(change: RoomChange) -> Event {
    Event::Rooms(vec![change])
}

fn appended(id: &str, secs: i64) -> Event {
    Event::Timeline(vec![TimelineChange::Appended(vec![message(id, secs)])])
}

// RT-002
#[test]
fn what_is_pushed_before_the_rooms_are_listed_is_replayed_on_the_list() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        Some(Arc::clone(&gate)),
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let mut unread = subscription("2", "secret", rc_api::RoomType::Private);
    unread.unread = 3;
    fake.script_realtime(
        None,
        vec![
            Pushed::Subscription(unread),
            pushed_subscription("3", "alpha", rc_api::RoomType::Channel),
        ],
        Ok(()),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    // The connection was opened before the rooms were asked for.
    fake.wait_for_subscriptions_calls(1);
    assert_eq!(fake.state().realtime_calls.len(), 1);
    gate.notify_one();

    let mut secret = room("2", "secret", RoomKind::Private);
    secret.unread = 3;
    harness.expect_events(vec![
        rooms_event(vec![
            room("1", "general", RoomKind::Channel),
            room("2", "secret", RoomKind::Private),
        ]),
        rooms_change(RoomChange::Updated {
            index: 1,
            room: secret,
        }),
        rooms_change(RoomChange::Inserted {
            index: 0,
            room: room("3", "alpha", RoomKind::Channel),
        }),
    ]);
    assert_eq!(
        fake.state().realtime_calls,
        [("uid".to_owned(), "tok".to_owned())]
    );
}

// RT-002
#[test]
fn a_subscription_change_inserts_updates_or_moves_the_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // New.
    assert!(fake.push(
        0,
        pushed_subscription("3", "zulu", rc_api::RoomType::Direct)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 2,
        room: room("3", "zulu", RoomKind::Direct),
    })]);
    // Same place, new counts.
    let mut mentioned = subscription("1", "general", rc_api::RoomType::Channel);
    mentioned.unread = 2;
    mentioned.user_mentions = 1;
    mentioned.alert = true;
    assert!(fake.push(0, Pushed::Subscription(mentioned)));
    let mut general = room("1", "general", RoomKind::Channel);
    general.unread = 2;
    general.user_mentions = 1;
    general.alert = true;
    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        room: general,
    })]);
    // Renamed.
    assert!(fake.push(
        0,
        pushed_subscription("2", "alpha", rc_api::RoomType::Private)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Moved {
        from: 1,
        to: 0,
        room: room("2", "alpha", RoomKind::Private),
    })]);
}

// RT-002
#[test]
fn a_removed_or_hidden_subscription_leaves_the_list() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 1 })]);
    // The next push shows nothing was emitted for an unknown room, or one that's gone.
    assert!(fake.push(0, removed("2")));
    assert!(fake.push(0, removed("nope")));
    let mut hidden = subscription("1", "general", rc_api::RoomType::Channel);
    hidden.open = false;
    assert!(fake.push(0, Pushed::Subscription(hidden)));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// RT-002
#[test]
fn removing_the_open_room_closes_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, removed("1")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
    harness.expect_events(closed_events());
    // The room is gone: it can't be opened.
    harness.client.open_room("1".to_owned().into());
    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// RT-001
#[test]
fn a_live_message_is_appended_to_the_open_room_and_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);

    // Kept: it's there when the room opens again.
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), message("c", 3)],
        ),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
}

// RT-001
#[test]
fn a_live_message_already_held_or_of_another_room_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, pushed_message("1", "b", 2)));
    assert!(fake.push(0, pushed_message("2", "x", 9)));
    assert!(fake.push(0, pushed_message("nowhere", "y", 9)));
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    // Nothing came out for the first three.
    harness.expect_events(vec![appended("c", 3)]);
}

// RT-001
#[test]
fn a_message_pushed_while_the_room_loads_joins_the_page() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(vec![message("b", 2), message("a", 1)], false),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    // Pushes come out in order: once this one has, the message before it was taken in.
    assert!(fake.push(
        0,
        pushed_subscription("3", "alpha", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        room: room("3", "alpha", RoomKind::Channel),
    })]);
    gate.notify_one();
    harness.expect_events(vec![
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), message("c", 3)],
        ),
        status("1", false, false, false, None),
    ]);
}

// AUTH-004
#[test]
fn an_unauthorized_realtime_connection_expires_the_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_realtime(None, Vec::new(), Err(ApiError::Unauthorized));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
    ]);
    wait_for_empty(&store);
    // Dropping flushes the actor's work: a fetch or a revoke would show by now.
    drop(harness);
    assert!(fake.state().subscriptions_calls.is_empty());
    assert!(fake.state().logout_calls.is_empty());
}

// RT-003
#[test]
fn another_realtime_failure_still_lists_the_rooms() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_realtime(
        None,
        Vec::new(),
        Err(ApiError::Unreachable("refused".into())),
    );
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
}

// RT-003
#[test]
fn a_connection_that_ends_leaves_the_session_working() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.end_connection(0);
    open_first_room(&fake, &harness, false);
}

/// The waits of the reconnect tests: short, but far apart enough to tell the steps from the noise.
const BASE: Duration = Duration::from_millis(100);
const CAP: Duration = Duration::from_millis(800);

/// How long a connection must stay up in the tests that need it to count as working.
const STABLE: Duration = Duration::from_millis(300);

/// A connection never counts as working for long enough.
fn fast() -> Backoff {
    Backoff::new(BASE, CAP, NEVER)
}

/// The subscription of room `1` as the fake lists it, with `unread` unread messages.
fn general_with(unread: u32) -> rc_api::Subscription {
    let mut general = subscription("1", "general", rc_api::RoomType::Channel);
    general.unread = unread;
    general
}

fn general_room(unread: u32) -> Room {
    let mut general = room("1", "general", RoomKind::Channel);
    general.unread = unread;
    general
}

// RT-003
#[test]
fn a_lost_connection_is_reopened_and_the_rooms_synced_without_a_reset() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    // While away: "secret" was left, "alpha" joined, "general" got unread messages.
    fake.script_fetch(
        None,
        Ok(vec![
            general_with(2),
            subscription("3", "alpha", rc_api::RoomType::Channel),
        ]),
    );
    // Pushed as soon as the new connection opens: replayed on top of the synced list.
    fake.script_realtime(None, vec![Pushed::Subscription(general_with(5))], Ok(()));
    fake.end_connection(0);

    harness.expect_events(vec![
        Event::Rooms(vec![
            RoomChange::Removed { index: 1 },
            RoomChange::Inserted {
                index: 0,
                room: room("3", "alpha", RoomKind::Channel),
            },
            RoomChange::Updated {
                index: 1,
                room: general_room(2),
            },
        ]),
        rooms_change(RoomChange::Updated {
            index: 1,
            room: general_room(5),
        }),
    ]);
    assert_eq!(fake.state().realtime_calls.len(), 2);
    // The new connection is the live one.
    assert!(fake.push(
        1,
        pushed_subscription("4", "zulu", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 2,
        room: room("4", "zulu", RoomKind::Channel),
    })]);
}

/// The time between `from` and the realtime call `index`.
fn wait_before_call(fake: &Fake, index: usize, from: Instant) -> Duration {
    wait_until(|| fake.state().realtime_times.get(index).copied()).duration_since(from)
}

/// A fake and a harness whose first three connections fail to open and whose fourth works; the
/// rooms are listed on the first and the fourth. Returns once the rooms are listed again.
fn after_three_failed_opens(backoff: Backoff) -> (Fake, Harness) {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    for _ in 0..3 {
        fake.script_realtime(
            None,
            Vec::new(),
            Err(ApiError::Unreachable("refused".into())),
        );
    }
    fake.script_fetch(None, Ok(vec![general_with(0)]));
    fake.script_fetch(None, Ok(vec![general_with(0)]));
    let harness = Harness::with_backoff(&fake, MemoryStore::default(), backoff);
    harness.login("http://h", "alice");
    // The rooms are listed once, although the connection isn't there.
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![general_room(0)]),
    ]);
    // The fourth connection opened, and the rooms were fetched once more: only that.
    fake.wait_for_connections(4);
    wait_until(|| (fake.state().subscriptions_calls.len() == 2).then_some(()));
    (fake, harness)
}

// RT-003
#[test]
fn a_failed_open_is_retried_with_growing_waits() {
    let (fake, harness) = after_three_failed_opens(fast());
    // Each wait is at least half of its step: 100, 200, 400 ms.
    let times = fake.state().realtime_times.clone();
    assert_eq!(times.len(), 4);
    for (index, step) in [BASE, BASE * 2, BASE * 4].into_iter().enumerate() {
        assert!(
            times[index + 1].duration_since(times[index]) >= step / 2,
            "wait {index} was shorter than {:?}",
            step / 2
        );
    }
    drop(harness);
}

// RT-003
#[test]
fn a_connection_that_drops_soon_after_it_worked_keeps_the_growing_wait() {
    let (fake, harness) = after_three_failed_opens(fast());
    // The fifth step would be 800 ms, of which 400 at least; the first step's is 100 at most.
    let lost = Instant::now();
    fake.end_connection(3);
    assert!(wait_before_call(&fake, 4, lost) >= CAP / 2);
    // And the one after it grows again, to the cap.
    let lost = Instant::now();
    fake.end_connection(4);
    assert!(wait_before_call(&fake, 5, lost) >= CAP / 2);
    drop(harness);
}

// RT-003
#[test]
fn a_connection_that_stayed_up_starts_the_waits_over() {
    let (fake, harness) = after_three_failed_opens(Backoff::new(BASE, CAP, STABLE));
    std::thread::sleep(STABLE * 2);
    // It worked for long enough, so the next loss waits for the first step (100 ms at most), not
    // the fifth (400 at least).
    let lost = Instant::now();
    fake.end_connection(3);
    assert!(wait_before_call(&fake, 4, lost) < CAP / 2);
    drop(harness);
}

/// A harness logged in whose room `1` is open with `a` and `b`, and whose connection was lost.
/// The next connection is not scripted yet.
fn open_room_then_lose_the_connection(fake: &Fake) -> Harness {
    let harness = logged_in_harness_with(fake, fast());
    open_first_room(fake, &harness, false);
    harness
}

// RT-003
#[test]
fn the_open_room_gets_the_messages_it_missed() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // Missed: c. Pushed right after reconnecting, while the page loads: d.
    fake.script_history(None, page(vec![message("c", 3), message("b", 2)], false));
    fake.script_realtime(None, vec![pushed_message("1", "d", 4)], Ok(()));
    fake.end_connection(0);

    // No room changes, no reset: the page touches what is held, so it is appended.
    harness.expect_events(vec![
        status("1", true, false, false, None),
        Event::Timeline(vec![TimelineChange::Appended(vec![
            message("c", 3),
            message("d", 4),
        ])]),
        status("1", false, false, false, None),
    ]);
    let calls = fake.state().history_calls.clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].before, None);
    assert_eq!(calls[1].room, "1");
}

// RT-003
#[test]
fn a_gap_longer_than_a_page_drops_the_held_messages() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // A page that doesn't reach a, b: keeping them would leave a hole nothing fills.
    fake.script_history(None, page(vec![message("z", 26), message("y", 25)], false));
    fake.end_connection(0);

    harness.expect_events(vec![
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("y", 25), message("z", 26)]),
        status("1", false, false, false, None),
    ]);
}

// RT-003
#[test]
fn an_open_room_that_is_gone_after_the_reconnect_is_closed() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![subscription("2", "secret", rc_api::RoomType::Private)]),
    );
    fake.end_connection(0);

    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
    harness.expect_events(closed_events());
    assert_eq!(fake.state().history_calls.len(), 1);
}

// RT-003
#[test]
fn logout_cancels_a_pending_reconnect() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, Backoff::new(BASE * 2, BASE * 2, NEVER));
    fake.end_connection(0);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    // Past the wait, even with the jitter at its least.
    std::thread::sleep(BASE * 4);
    assert_eq!(fake.state().realtime_calls.len(), 1);
}

// RT-003
#[test]
fn an_expired_token_on_a_reconnect_expires_the_session() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    fake.script_login(None, Ok(()));
    let harness = Harness::with_backoff(&fake, store.clone(), fast());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    fake.wait_for_connections(1);
    fake.script_realtime(None, Vec::new(), Err(ApiError::Unauthorized));
    fake.end_connection(0);
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_empty(&store);
    // No more tries, and nothing revoked.
    std::thread::sleep(BASE * 2);
    drop(harness);
    assert_eq!(fake.state().realtime_calls.len(), 2);
    assert!(fake.state().logout_calls.is_empty());
}

// RT-003
#[test]
fn a_failed_room_fetch_ends_the_cycle_and_is_retried_after_a_wait() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    fake.script_fetch(None, Err(ApiError::Unreachable("reset".into())));
    fake.script_fetch(
        None,
        Ok(vec![
            general_with(1),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let lost = Instant::now();
    fake.end_connection(0);

    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        room: general_room(1),
    })]);
    // Connection 1 was dropped when its fetch failed, and connection 2 came after a wait.
    assert!(
        fake.state().connections[1]
            .as_ref()
            .is_some_and(UnboundedSender::is_closed)
    );
    assert_eq!(fake.state().realtime_calls.len(), 3);
    // Two waits: the first step's half at least, and the second's.
    assert!(lost.elapsed() >= BASE / 2 + BASE);
}

// RT-001
#[test]
fn logout_ends_the_connection_and_the_next_session_gets_its_own() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    // The client dropped its end: the connection is closed.
    fake.wait_for_connection_closed(0);
    assert!(!fake.push(0, removed("1")));

    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    assert!(fake.push(
        1,
        pushed_subscription("5", "new", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        room: room("5", "new", RoomKind::Channel),
    })]);
}

// RT-001
#[test]
fn a_connection_that_opens_after_logout_is_dropped() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(None, Ok(()));
    fake.script_login(None, Ok(()));
    fake.script_realtime(
        Some(Arc::clone(&gate)),
        vec![pushed_subscription("old", "old", rc_api::RoomType::Channel)],
        Ok(()),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    fake.wait_for_connections(1);

    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    fake.wait_for_connection_closed(0);
    gate.notify_one();

    fake.script_fetch(
        None,
        Ok(vec![subscription("new", "new", rc_api::RoomType::Channel)]),
    );
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("new", "new", RoomKind::Channel)]),
    ]);
    // Only the new connection's pushes arrive.
    assert!(fake.push(
        1,
        pushed_subscription("5", "other", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 1,
        room: room("5", "other", RoomKind::Channel),
    })]);
}

fn updated(index: usize, message: Message) -> Event {
    Event::Timeline(vec![TimelineChange::Updated { index, message }])
}

/// The same message in another delivery state.
fn with_delivery(message: &Message, delivery: Delivery) -> Message {
    Message {
        delivery,
        ..message.clone()
    }
}

/// The pending message that sending put at the end of the open room, which is the next event.
fn expect_pending(harness: &Harness) -> Message {
    let Event::Timeline(changes) = harness.next_event() else {
        panic!("not a timeline event");
    };
    let [TimelineChange::Appended(messages)] = changes.as_slice() else {
        panic!("not a single append: {changes:?}");
    };
    let [pending] = messages.as_slice() else {
        panic!("not a single message: {messages:?}");
    };
    assert_eq!(pending.delivery, Delivery::Pending);
    pending.clone()
}

/// The messages of the next event, which resets the message list.
fn reset_messages(harness: &Harness) -> Vec<Message> {
    let Event::Timeline(changes) = harness.next_event() else {
        panic!("not a timeline event");
    };
    let [TimelineChange::Reset { messages, .. }] = changes.as_slice() else {
        panic!("not a reset: {changes:?}");
    };
    messages.clone()
}

/// Opens room 2, which has nothing, and takes its events.
fn open_the_other_room(harness: &Harness) {
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
}

/// Makes sure the actor has taken in everything pushed so far, for events that have no output:
/// pushes are taken in order, so a message of the open room that comes out is the proof.
fn flush_pushed(fake: &Fake, harness: &Harness, room: &str) {
    assert!(fake.push(0, pushed_message(room, "flush", 999)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        Message {
            room_id: room.to_owned().into(),
            ..message("flush", 999)
        },
    ])])]);
}

// SEND-001
#[test]
fn a_sent_message_shows_as_pending_and_becomes_sent_when_the_server_answers() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));

    harness
        .client
        .send_message("1".to_owned().into(), "  hello\n");

    let pending = expect_pending(&harness);
    assert_eq!(pending.kind, MessageKind::Text("hello".into()));
    assert_eq!(pending.room_id.as_str(), "1");
    assert_eq!(pending.author.id.as_str(), "uid");
    assert_eq!(pending.author.username, "alice");
    // Sent under the id that was shown.
    assert_eq!(
        fake.wait_for_send_calls(1),
        [SendCall {
            room: "1".into(),
            id: pending.id.to_string(),
            text: "hello".into()
        }]
    );
    gate.notify_one();
    harness.expect_events(vec![updated(
        2,
        server_copy("1", pending.id.as_str(), "hello", 7),
    )]);
}

// SEND-001
#[test]
fn blank_text_unknown_rooms_and_logged_out_clients_send_nothing() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    // Logged out.
    harness.client.send_message("1".to_owned().into(), "hi");
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    // Not in the room list.
    harness.client.send_message("1".to_owned().into(), "hi");
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    drop(harness);

    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.send_message("1".to_owned().into(), " \n\t ");
    harness.client.send_message("1".to_owned().into(), "");
    drop(harness);
    assert!(fake.state().send_calls.is_empty());
}

// SEND-001
#[test]
fn the_echo_that_comes_first_settles_the_message_and_the_answer_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);

    let copy = server_copy("1", pending.id.as_str(), "hello", 7);
    assert!(fake.push(0, Pushed::Message(copy.clone())));
    harness.expect_events(vec![updated(2, copy)]);

    gate.notify_one();
    fake.wait_for_sends_returned(1);
    // Nothing came out for the answer, and the message is there once.
    assert!(fake.push(0, pushed_message("1", "c", 8)));
    harness.expect_events(vec![appended("c", 8)]);
}

// SEND-002
#[test]
fn a_refused_message_fails_and_retrying_sends_it_under_the_same_id() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(
        None,
        Err(ApiError::Server {
            status: 400,
            message: "no".into(),
        }),
    );
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);

    fake.script_send(None, Ok(9));
    harness.client.retry_message(pending.id.clone());
    harness.expect_events(vec![
        updated(2, pending.clone()),
        updated(2, server_copy("1", pending.id.as_str(), "hello", 9)),
    ]);
    let calls = fake.wait_for_send_calls(2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(calls[1].id, pending.id.as_str());
}

// SEND-002
#[test]
fn an_unreachable_server_fails_the_message_too() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unreachable("refused".into())));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);
}

// SEND-002
#[test]
fn only_a_failed_message_of_the_open_room_is_retried() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);

    // Sent already, pending, and unknown.
    harness.client.retry_message("a".to_owned().into());
    harness.client.retry_message(pending.id.clone());
    harness.client.retry_message("nope".to_owned().into());
    gate.notify_one();
    harness.expect_events(vec![updated(
        2,
        server_copy("1", pending.id.as_str(), "hello", 7),
    )]);

    assert_eq!(fake.state().send_calls.len(), 1);
}

// SEND-001
#[test]
fn a_message_confirmed_after_the_room_was_switched_is_sent_when_the_room_opens_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);
    open_the_other_room(&harness);

    gate.notify_one();
    fake.wait_for_sends_returned(1);
    flush_after_answer(&fake, &harness);

    harness.client.open_room("1".to_owned().into());
    assert_eq!(
        reset_messages(&harness),
        [
            message("a", 1),
            message("b", 2),
            server_copy("1", pending.id.as_str(), "hello", 7)
        ]
    );
}

/// Makes sure the actor has taken in the answer of a send that returned, while room 2 is open.
fn flush_after_answer(fake: &Fake, harness: &Harness) {
    flush_pushed(fake, harness, "2");
}

// SEND-002
#[test]
fn a_message_that_fails_after_the_room_was_switched_shows_as_failed_when_it_opens_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(
        Some(Arc::clone(&gate)),
        Err(ApiError::Unreachable("down".into())),
    );
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);
    open_the_other_room(&harness);

    gate.notify_one();
    fake.wait_for_sends_returned(1);
    flush_after_answer(&fake, &harness);

    harness.client.open_room("1".to_owned().into());
    assert_eq!(
        reset_messages(&harness),
        [
            message("a", 1),
            message("b", 2),
            with_delivery(&pending, Delivery::Failed)
        ]
    );
}

// SEND-002
#[test]
fn a_failed_message_is_retried_from_the_room_it_was_shown_in_after_switching_back() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unreachable("down".into())));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);
    open_the_other_room(&harness);
    harness.client.open_room("1".to_owned().into());
    assert_eq!(reset_messages(&harness).len(), 3);
    harness.expect_events(vec![
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);

    // Retrying, then switching away before the answer comes.
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.client.retry_message(pending.id.clone());
    harness.expect_events(vec![updated(2, pending.clone())]);
    fake.wait_for_send_calls(2);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, true, None),
        status("2", false, false, true, None),
    ]);
    gate.notify_one();
    fake.wait_for_sends_returned(1);
    flush_after_answer(&fake, &harness);

    harness.client.open_room("1".to_owned().into());
    let shown = reset_messages(&harness);
    assert_eq!(
        shown.last(),
        Some(&server_copy("1", pending.id.as_str(), "hello", 7))
    );
    assert_eq!(shown.len(), 3);
}

// SEND-001
#[test]
fn the_echo_of_a_message_sent_from_a_room_that_is_not_open_settles_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);
    open_the_other_room(&harness);

    let copy = server_copy("1", pending.id.as_str(), "hello", 7);
    assert!(fake.push(0, Pushed::Message(copy.clone())));
    flush_after_answer(&fake, &harness);

    harness.client.open_room("1".to_owned().into());
    assert_eq!(
        reset_messages(&harness),
        [message("a", 1), message("b", 2), copy]
    );
}

// SEND-001
#[test]
fn messages_of_others_in_rooms_that_are_not_open_are_not_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    open_the_other_room(&harness);
    // Neither ours nor anyone's: only echoes of our own sends touch a closed room.
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    flush_after_answer(&fake, &harness);
    harness.client.open_room("1".to_owned().into());
    assert_eq!(reset_messages(&harness), [message("a", 1), message("b", 2)]);
}

// SEND-001
#[test]
fn a_confirmed_message_moves_to_where_the_servers_time_puts_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(50));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);

    // Someone else's message comes in meanwhile, and goes in front of the pending one.
    assert!(fake.push(0, pushed_message("1", "c", 100)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Inserted {
        index: 2,
        messages: vec![message("c", 100)],
    }])]);

    // The server's clock says ours came before it.
    gate.notify_one();
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Moved {
        from: 3,
        to: 2,
        message: server_copy("1", pending.id.as_str(), "hello", 50),
    }])]);
}

// SEND-002
#[test]
fn an_unsent_message_survives_the_catch_up_after_a_reconnect() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_send(None, Err(ApiError::Unreachable("down".into())));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    let failed = with_delivery(&pending, Delivery::Failed);
    harness.expect_events(vec![updated(2, failed.clone())]);

    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_history(None, page(vec![message("c", 3), message("b", 2)], false));
    fake.end_connection(0);

    // The page goes in front of the failed message.
    harness.expect_events(vec![
        status("1", true, false, false, None),
        Event::Timeline(vec![TimelineChange::Inserted {
            index: 2,
            messages: vec![message("c", 3)],
        }]),
        status("1", false, false, false, None),
    ]);
}

// SEND-002
#[test]
fn an_unsent_message_survives_a_gap_longer_than_a_page() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_send(None, Err(ApiError::Unreachable("down".into())));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    let failed = with_delivery(&pending, Delivery::Failed);
    harness.expect_events(vec![updated(2, failed.clone())]);

    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_history(None, page(vec![message("z", 26), message("y", 25)], false));
    fake.end_connection(0);

    harness.expect_events(vec![
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("y", 25), message("z", 26), failed]),
        status("1", false, false, false, None),
    ]);
}

// SEND-001
#[test]
fn the_catch_up_settles_a_message_whose_echo_and_answer_were_lost() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.client.send_message("1".to_owned().into(), "hello");
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);

    let copy = server_copy("1", pending.id.as_str(), "hello", 7);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_history(None, page(vec![copy.clone(), message("b", 2)], false));
    fake.end_connection(0);

    harness.expect_events(vec![
        status("1", true, false, false, None),
        updated(2, copy),
        status("1", false, false, false, None),
    ]);
}

// SEND-002
#[test]
fn a_refused_token_on_a_send_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unauthorized));
    harness.client.send_message("1".to_owned().into(), "hello");
    expect_pending(&harness);
    harness.expect_events(vec![
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}

// SEND-001
#[test]
fn a_message_confirmed_while_the_room_loads_is_not_lost_to_the_page() {
    for by_echo in [true, false] {
        let fake = Fake::new();
        let harness = logged_in_harness(&fake);
        let history_gate = Arc::new(Notify::new());
        fake.script_history(
            Some(Arc::clone(&history_gate)),
            page(vec![message("b", 2), message("a", 1)], false),
        );
        let send_gate = Arc::new(Notify::new());
        fake.script_send(Some(Arc::clone(&send_gate)), Ok(7));
        harness.client.open_room("1".to_owned().into());
        harness.expect_events(vec![
            reset_event(Some("1"), Vec::new()),
            status("1", true, false, false, None),
        ]);
        harness.client.send_message("1".to_owned().into(), "hello");
        let pending = expect_pending(&harness);
        fake.wait_for_send_calls(1);
        let copy = server_copy("1", pending.id.as_str(), "hello", 7);

        // The server confirms before the page comes, and the page was made before that.
        if by_echo {
            assert!(fake.push(0, Pushed::Message(copy.clone())));
        } else {
            send_gate.notify_one();
        }
        harness.expect_events(vec![updated(0, copy.clone())]);
        history_gate.notify_one();

        harness.expect_events(vec![
            Event::Timeline(vec![TimelineChange::Prepended(vec![
                message("a", 1),
                message("b", 2),
            ])]),
            status("1", false, false, false, None),
        ]);
    }
}

// SEND-001
#[test]
fn a_message_sent_while_the_room_loads_waits_for_the_page_behind_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(vec![message("b", 2), message("a", 1)], true),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    let send_gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&send_gate)), Ok(7));
    harness.client.send_message("1".to_owned().into(), "hello");
    expect_pending(&harness);

    gate.notify_one();

    harness.expect_events(vec![
        Event::Timeline(vec![TimelineChange::Inserted {
            index: 0,
            messages: vec![message("a", 1), message("b", 2)],
        }]),
        status("1", false, false, true, None),
    ]);
}
// RT-004
#[test]
fn a_lost_connection_is_reported_with_the_wait_and_then_the_attempt_and_the_recovery() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    fake.end_connection(0);
    let Connection::Waiting { retry_in } = harness.next_connection() else {
        panic!("not waiting");
    };
    assert!((BASE / 2..=BASE).contains(&retry_in), "{retry_in:?}");
    assert_eq!(harness.next_connection(), Connection::Reconnecting);
    assert_eq!(harness.next_connection(), Connection::Connected);
}

// RT-004
#[test]
fn a_failed_attempt_is_reported_as_waiting_again_with_a_longer_wait() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    fake.script_realtime(
        None,
        Vec::new(),
        Err(ApiError::Unreachable("refused".into())),
    );
    fake.end_connection(0);
    let Connection::Waiting { retry_in: first } = harness.next_connection() else {
        panic!("not waiting");
    };
    assert_eq!(harness.next_connection(), Connection::Reconnecting);
    let Connection::Waiting { retry_in: second } = harness.next_connection() else {
        panic!("not waiting");
    };
    assert!(second >= BASE, "{first:?} then {second:?}");
    assert_eq!(harness.next_connection(), Connection::Reconnecting);
    assert_eq!(harness.next_connection(), Connection::Connected);
}

// RT-004
#[test]
fn a_connection_that_is_never_lost_reports_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.wait_for_connections(1);
    harness.client.logout();
    loop {
        match harness.events.recv_timeout(TIMEOUT).expect("no event") {
            Event::Connection(connection) => panic!("{connection:?}"),
            Event::Session(SessionState::LoggedOut { .. }) => break,
            _ => {}
        }
    }
}

/// A harness logged in with room `1` (`general`) holding unread messages and a mention, and room
/// `2` (`secret`) holding some too.
fn logged_in_with_unread(fake: &Fake) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(vec![unread_general(), unread_secret()]));
    let harness = Harness::new(fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            room_from(unread_general()).unwrap(),
            room_from(unread_secret()).unwrap(),
        ]),
    ]);
    harness
}

fn unread_general() -> rc_api::Subscription {
    let mut general = subscription("1", "general", rc_api::RoomType::Channel);
    general.unread = 3;
    general.user_mentions = 1;
    general.alert = true;
    general
}

fn unread_secret() -> rc_api::Subscription {
    let mut secret = subscription("2", "secret", rc_api::RoomType::Private);
    secret.unread = 2;
    secret.alert = true;
    secret
}

/// The next event is the room list changing, once, to this.
fn expect_rooms_updated(harness: &Harness, index: usize, room: Room) {
    harness.expect_events(vec![rooms_change(RoomChange::Updated { index, room })]);
}

// ROOM-004
#[test]
fn opening_a_room_marks_it_read_on_the_server_and_in_the_list() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);

    harness.client.open_room("1".to_owned().into());

    harness.expect_events(vec![reset_event(Some("1"), Vec::new())]);
    expect_rooms_updated(&harness, 0, room("1", "general", RoomKind::Channel));
    assert_eq!(fake.wait_for_mark_read_calls(1), ["1"]);
}

// ROOM-004
#[test]
fn opening_a_room_that_shows_read_still_tells_the_server() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    assert_eq!(fake.wait_for_mark_read_calls(1), ["1"]);
}

// ROOM-004
#[test]
fn only_the_opened_room_is_marked_read() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);
    harness.client.open_room("1".to_owned().into());
    fake.wait_for_mark_read_calls(1);
    harness.client.open_room("2".to_owned().into());

    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "2"]);
    let mut changes_seen = 0;
    while let Ok(event) = harness.events.recv_timeout(Duration::from_millis(300)) {
        if let Event::Rooms(changes) = event {
            changes_seen += changes.len();
        }
    }
    // Room 1 cleared when it opened, then room 2.
    assert_eq!(changes_seen, 2);
}

// ROOM-004
#[test]
fn a_message_in_the_open_room_does_not_raise_its_unread_count() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);

    // The server counts it as unread, as it doesn't know the room is on screen.
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    assert!(fake.push(0, Pushed::Subscription(general_with(1))));
    // Another room's count does rise.
    let mut secret = subscription("2", "secret", rc_api::RoomType::Private);
    secret.unread = 4;
    assert!(fake.push(0, Pushed::Subscription(secret)));

    let mut secret_room = room("2", "secret", RoomKind::Private);
    secret_room.unread = 4;
    harness.expect_events(vec![
        appended("c", 3),
        rooms_change(RoomChange::Updated {
            index: 1,
            room: secret_room,
        }),
    ]);
    // And the server is told again, once.
    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "1"]);
}

// ROOM-004
#[test]
fn a_subscription_change_of_the_open_room_that_shows_read_is_not_marked_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);

    let mut renamed = subscription("1", "main", rc_api::RoomType::Channel);
    renamed.display_name = Some("main".to_owned());
    assert!(fake.push(0, Pushed::Subscription(renamed)));
    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        room: room("1", "main", RoomKind::Channel),
    })]);
    assert_eq!(fake.state().mark_read_calls, ["1"]);
}

// ROOM-004
#[test]
fn a_closed_room_counts_new_messages_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    // Leaving the room closes it.
    assert!(fake.push(0, removed("1")));
    harness.expect_events(vec![
        rooms_change(RoomChange::Removed { index: 0 }),
        Event::Timeline(vec![TimelineChange::Reset {
            room: None,
            messages: Vec::new(),
        }]),
        Event::TimelineStatus(TimelineStatus::default()),
    ]);
    assert!(fake.push(0, Pushed::Subscription(general_with(2))));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        room: general_room(2),
    })]);
    assert_eq!(fake.state().mark_read_calls, ["1"]);
}

// ROOM-004
#[test]
fn a_snapshot_after_reconnecting_does_not_show_the_open_room_unread() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);
    // A message came in while away: the snapshot counts it.
    fake.script_fetch(
        None,
        Ok(vec![
            general_with(1),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.end_connection(0);

    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "1"]);
    // The list needs no change: room 1 shows read already.
    while let Ok(event) = harness.events.recv_timeout(Duration::from_millis(300)) {
        assert!(!matches!(event, Event::Rooms(_)), "{event:?}");
    }
}

// ROOM-004
#[test]
fn a_failed_mark_read_is_not_fatal() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_mark_read(Err(ApiError::Server {
        status: 400,
        message: "error-invalid-subscription".to_owned(),
    }));
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);

    // Still logged in, and the room still gets live messages.
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);
}

// ROOM-004
#[test]
fn an_expired_token_in_mark_read_ends_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(Arc::clone(&gate)), page(Vec::new(), true));
    fake.script_mark_read(Err(ApiError::Unauthorized));

    harness.client.open_room("1".to_owned().into());

    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}

fn at(secs: i64) -> jiff::Timestamp {
    jiff::Timestamp::from_second(secs).unwrap()
}

/// The subscription of room `id`, as the server lists it with a last message at `secs`.
fn active_subscription(id: &str, name: &str, secs: i64) -> rc_api::Subscription {
    let mut sub = subscription(id, name, rc_api::RoomType::Channel);
    sub.last_message = Some(at(secs));
    sub
}

fn active_room(id: &str, name: &str, secs: i64) -> Room {
    Room {
        last_activity: at(secs),
        ..room(id, name, RoomKind::Channel)
    }
}

/// A harness logged in with rooms `1` (`alpha`, busy at 10 s), `2` (`beta`, 30 s) and `3`
/// (`gamma`, 20 s), listed as `2`, `3`, `1`.
fn logged_in_by_activity(fake: &Fake) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            active_subscription("1", "alpha", 10),
            active_subscription("2", "beta", 30),
            active_subscription("3", "gamma", 20),
        ]),
    );
    let harness = Harness::new(fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            active_room("2", "beta", 30),
            active_room("3", "gamma", 20),
            active_room("1", "alpha", 10),
        ]),
    ]);
    harness
}

// ROOM-005
#[test]
fn login_lists_the_rooms_by_last_activity_falling_back_to_the_join_time() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let mut joined = subscription("4", "joined", rc_api::RoomType::Channel);
    joined.created = Some(at(25));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("5", "never", rc_api::RoomType::Channel),
            active_subscription("1", "alpha", 10),
            joined,
            active_subscription("2", "beta", 30),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    let mut joined = room("4", "joined", RoomKind::Channel);
    joined.last_activity = at(25);
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            active_room("2", "beta", 30),
            joined,
            active_room("1", "alpha", 10),
            room("5", "never", RoomKind::Channel),
        ]),
    ]);
}

// ROOM-005
#[test]
fn activity_moves_the_room_to_the_front() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);

    assert!(fake.push(
        0,
        Pushed::Activity {
            room: "1".to_owned().into(),
            at: at(40)
        }
    ));

    harness.expect_events(vec![rooms_change(RoomChange::Moved {
        from: 2,
        to: 0,
        room: active_room("1", "alpha", 40),
    })]);
}

// ROOM-005
#[test]
fn a_message_raises_the_unread_count_and_then_moves_the_room() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    let mut unread = active_subscription("3", "gamma", 20);
    unread.unread = 1;
    unread.last_message = None;

    assert!(fake.push(0, Pushed::Subscription(unread)));
    assert!(fake.push(
        0,
        Pushed::Activity {
            room: "3".to_owned().into(),
            at: at(50)
        }
    ));

    let mut updated = active_room("3", "gamma", 20);
    updated.unread = 1;
    let mut moved = active_room("3", "gamma", 50);
    moved.unread = 1;
    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 1,
            room: updated,
        }),
        rooms_change(RoomChange::Moved {
            from: 1,
            to: 0,
            room: moved,
        }),
    ]);
}

// ROOM-005
#[test]
fn a_subscription_change_keeps_the_known_activity() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    // What the server pushes has no last message and a join time long before it.
    let mut renamed = subscription("1", "omega", rc_api::RoomType::Channel);
    renamed.created = Some(at(1));

    assert!(fake.push(0, Pushed::Subscription(renamed)));

    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 2,
        room: active_room("1", "omega", 10),
    })]);
}

// ROOM-005
#[test]
fn old_or_unknown_activity_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    for (room, secs) in [("2", 30), ("2", 5), ("zzz", 99)] {
        assert!(fake.push(
            0,
            Pushed::Activity {
                room: room.to_owned().into(),
                at: at(secs)
            }
        ));
    }
    // Something that does show, as the end of the line.
    assert!(fake.push(0, removed("2")));

    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// ROOM-005
#[test]
fn a_new_room_comes_in_at_its_activity() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    let mut joined = subscription("9", "fresh", rc_api::RoomType::Channel);
    joined.created = Some(at(25));

    assert!(fake.push(0, Pushed::Subscription(joined)));

    let mut fresh = room("9", "fresh", RoomKind::Channel);
    fresh.last_activity = at(25);
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 1,
        room: fresh,
    })]);
}

// ROOM-005
#[test]
fn a_snapshot_after_reconnecting_moves_the_rooms_that_were_busy() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            active_subscription("1", "alpha", 10),
            active_subscription("2", "beta", 30),
        ]),
    );
    let harness = Harness::with_backoff(&fake, MemoryStore::default(), fast());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            active_room("2", "beta", 30),
            active_room("1", "alpha", 10),
        ]),
    ]);
    // While away, "alpha" had a message.
    fake.script_fetch(
        None,
        Ok(vec![
            active_subscription("1", "alpha", 50),
            active_subscription("2", "beta", 30),
        ]),
    );
    fake.end_connection(0);

    harness.expect_events(vec![Event::Rooms(vec![RoomChange::Moved {
        from: 1,
        to: 0,
        room: active_room("1", "alpha", 50),
    }])]);
}

// Avatars

/// A room's subscription as the server lists it when the room has an avatar of its own.
fn avatar_subscription(id: &str, name: &str, room_type: rc_api::RoomType) -> rc_api::Subscription {
    rc_api::Subscription {
        avatar_etag: Some("etag".to_owned()),
        ..subscription(id, name, room_type)
    }
}

fn avatar_room(id: &str, name: &str, kind: RoomKind) -> Room {
    Room {
        avatar_etag: Some("etag".to_owned()),
        ..room(id, name, kind)
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn png(bytes: &[u8]) -> Result<Option<Image>, ApiError> {
    Ok(Some(Image {
        bytes: bytes.to_vec(),
        content_type: Some("image/png".to_owned()),
    }))
}

fn user_owner(name: &str) -> AvatarOwner {
    AvatarOwner::User(name.to_owned())
}

fn room_owner(id: &str) -> AvatarOwner {
    AvatarOwner::Room(id.to_owned().into())
}

/// A message of room `1` written by `author`.
fn message_by(id: &str, secs: i64, author: &str) -> Message {
    let mut message = message(id, secs);
    message.author.username = author.to_owned();
    message
}

fn update(owner: AvatarOwner, path: &str) -> AvatarUpdate {
    AvatarUpdate {
        owner,
        path: Some(PathBuf::from(path)),
    }
}

/// A harness with a cache, logged in with the rooms `1` and `2`, past their events.
fn logged_in_with_cache(fake: &Fake, cache: &FakeCache) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            avatar_subscription("1", "general", rc_api::RoomType::Channel),
            avatar_subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::with_cache(fake, cache.clone());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            avatar_room("1", "general", RoomKind::Channel),
            avatar_room("2", "secret", RoomKind::Private),
        ]),
    ]);
    harness
}

// RICH-004
#[test]
fn cached_avatars_of_the_rooms_are_sent_at_once() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"cached");
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    // Nothing for the room that has none. The downloads run at once, in any order.
    let calls = fake.wait_for_avatar_calls(2);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.contains(&room_owner("1")), "{calls:?}");
    assert!(calls.contains(&room_owner("2")), "{calls:?}");
}

/// Like [`logged_in_with_cache`], for a cache that makes `Avatars` events come in between.
fn logged_in_with_cache_events(fake: &Fake, cache: &FakeCache) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            avatar_subscription("1", "general", rc_api::RoomType::Channel),
            avatar_subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::with_cache(fake, cache.clone());
    harness.login("http://h", "alice");
    harness
}

// RICH-004
#[test]
fn a_downloaded_avatar_is_sent_once_it_is_stored() {
    let fake = Fake::new();
    fake.script_avatar(room_owner("1"), None, png(b"one"));
    let cache = FakeCache::default();
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"one".to_vec()));
}

// RICH-004
#[test]
fn a_direct_message_shows_the_avatar_of_the_other_user() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            avatar_subscription("d1", "bob", rc_api::RoomType::Direct),
            avatar_subscription("d2", "bob,carol", rc_api::RoomType::Direct),
        ]),
    );
    let harness = Harness::with_cache(&fake, FakeCache::default());
    harness.login("http://h", "alice");
    let calls = fake.wait_for_avatar_calls(2);
    assert!(calls.contains(&user_owner("bob")), "{calls:?}");
    assert!(!calls.contains(&room_owner("d1")), "{calls:?}");
    // A group conversation has no other user to show.
    assert!(calls.contains(&room_owner("d2")), "{calls:?}");
    drop(harness);
}

// RICH-004
#[test]
fn the_authors_of_the_open_room_get_their_avatars() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &user_owner("bob"), "png", b"cached");
    fake.script_avatar(user_owner("bob"), None, png(b"cached"));
    fake.script_avatar(user_owner("carol"), None, png(b"carol"));
    let harness = logged_in_with_cache(&fake, &cache);
    fake.script_history(
        None,
        page(
            vec![message_by("b", 2, "carol"), message_by("a", 1, "bob")],
            true,
        ),
    );
    harness.client.open_room("1".to_owned().into());

    let mut sent = harness.next_avatars();
    sent.extend(harness.next_avatars());
    assert!(sent.contains(&update(user_owner("bob"), "/cache/h/user/bob.png")));
    assert!(sent.contains(&update(user_owner("carol"), "/cache/h/user/carol.png")));
}

// RICH-004
#[test]
fn the_author_of_a_live_message_gets_an_avatar_and_system_messages_none() {
    let fake = Fake::new();
    fake.script_avatar(user_owner("dave"), None, png(b"dave"));
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);
    let mut joined = message_by("j", 9, "erin");
    joined.kind = MessageKind::System(crate::domain::SystemEvent::Joined);
    fake.push(0, Pushed::Message(joined));
    fake.push(0, Pushed::Message(message_by("c", 3, "dave")));

    assert_eq!(
        harness.next_avatars(),
        [update(user_owner("dave"), "/cache/h/user/dave.png")]
    );
    let calls = fake.state().avatar_calls.clone();
    assert!(!calls.contains(&user_owner("erin")), "{calls:?}");
}

/// Opens room 1 with an empty page, which wants no avatar.
fn open_first_room_of(fake: &Fake, harness: &Harness) {
    fake.script_history(None, page(Vec::new(), true));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
}

// RICH-004
#[test]
fn an_avatar_is_downloaded_once_per_session_however_often_it_is_wanted() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_avatar(user_owner("bob"), Some(gate.clone()), png(b"bob"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    fake.script_history(
        None,
        page(
            vec![message_by("b", 2, "bob"), message_by("a", 1, "bob")],
            true,
        ),
    );
    harness.client.open_room("1".to_owned().into());
    fake.wait_for_avatar_calls(3);
    // Held by the gate: a live message from the same author finds the download in flight.
    fake.push(0, Pushed::Message(message_by("c", 3, "bob")));
    gate.notify_one();
    assert_eq!(
        harness.next_avatars(),
        [update(user_owner("bob"), "/cache/h/user/bob.png")]
    );
    fake.push(0, Pushed::Message(message_by("d", 4, "bob")));
    harness.wait_for_appended("d");
    let calls = fake.state().avatar_calls.clone();
    assert_eq!(
        calls
            .iter()
            .filter(|owner| **owner == user_owner("bob"))
            .count(),
        1,
        "{calls:?}"
    );
}

// RICH-004
#[test]
fn no_more_than_four_avatars_download_at_once() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    let names = ["1", "2", "3", "4", "5", "6"];
    for name in names {
        fake.script_avatar(room_owner(name), Some(gate.clone()), png(name.as_bytes()));
    }
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(names
            .iter()
            .map(|name| avatar_subscription(name, name, rc_api::RoomType::Channel))
            .collect()),
    );
    let harness = Harness::with_cache(&fake, FakeCache::default());
    harness.login("http://h", "alice");

    fake.wait_for_avatar_calls(4);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.state().avatar_calls.len(), 4);
    assert_eq!(fake.state().avatar_max_running, 4);

    // One finishing lets the next start.
    gate.notify_one();
    fake.wait_for_avatar_calls(5);
    gate.notify_waiters();
    fake.wait_for_avatar_calls(6);
    assert!(fake.state().avatar_max_running <= 4);
    drop(harness);
}

// RICH-004
#[test]
fn an_avatar_that_is_the_same_as_the_cached_one_sends_nothing_more() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"same");
    fake.script_avatar(room_owner("1"), None, png(b"same"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    fake.wait_for_avatar_calls(2);
    harness.assert_no_avatars();
}

// RICH-004
#[test]
fn an_avatar_that_differs_from_the_cached_one_is_sent_again() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"old");
    fake.script_avatar(room_owner("1"), None, png(b"new"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    let path = "/cache/h/room/1.png";
    assert_eq!(harness.next_avatars(), [update(room_owner("1"), path)]);
    assert_eq!(harness.next_avatars(), [update(room_owner("1"), path)]);
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"new".to_vec()));
}

// RICH-004
#[test]
fn an_avatar_of_another_type_replaces_the_cached_one() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "svg", b"<svg/>");
    fake.script_avatar(room_owner("1"), None, png(b"png"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.svg")]
    );
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
}

// RICH-004
#[test]
fn a_missing_or_refused_avatar_is_no_avatar_and_nothing_else_happens() {
    for error in [
        ApiError::Server {
            status: 404,
            message: String::new(),
        },
        ApiError::Unauthorized,
        ApiError::Unexpected("odd".into()),
    ] {
        let fake = Fake::new();
        fake.script_avatar(room_owner("1"), None, Err(error));
        let harness = logged_in_with_cache(&fake, &FakeCache::default());
        fake.wait_for_avatar_calls(2);
        harness.assert_no_avatars();
        // The session carries on: opening a room still works.
        open_first_room_of(&fake, &harness);
    }
}

// RICH-004
#[test]
fn an_avatar_of_a_type_the_cache_doesnt_keep_is_no_avatar() {
    let fake = Fake::new();
    fake.script_avatar(
        room_owner("1"),
        None,
        Ok(Some(Image {
            bytes: b"<html>".to_vec(),
            content_type: Some("text/html".to_owned()),
        })),
    );
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    fake.wait_for_avatar_calls(2);
    harness.assert_no_avatars();
    assert_eq!(cache.bytes("h", &room_owner("1")), None);
}

// RICH-004
#[test]
fn an_avatar_that_couldnt_be_downloaded_is_tried_again_after_reconnecting() {
    let fake = Fake::new();
    fake.script_avatar(
        room_owner("1"),
        None,
        Err(ApiError::Unreachable("down".into())),
    );
    fake.script_avatar(room_owner("1"), None, png(b"one"));
    fake.script_avatar(room_owner("2"), None, png(b"two"));
    fake.script_login(None, Ok(()));
    let rooms = || {
        Ok(vec![
            avatar_subscription("1", "general", rc_api::RoomType::Channel),
            avatar_subscription("2", "secret", rc_api::RoomType::Private),
        ])
    };
    fake.script_fetch(None, rooms());
    fake.script_fetch(None, rooms());
    let harness = Harness::with_cache_and_backoff(&fake, FakeCache::default(), fast());
    harness.login("http://h", "alice");
    // Room 2's came; room 1's didn't.
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("2"), "/cache/h/room/2.png")]
    );

    fake.end_connection(0);

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    let calls = fake.state().avatar_calls.clone();
    // Room 2 had its refresh already.
    assert_eq!(
        calls
            .iter()
            .filter(|owner| **owner == room_owner("2"))
            .count(),
        1,
        "{calls:?}"
    );
}

// RICH-004
#[test]
fn logging_out_forgets_the_avatars_and_drops_what_was_still_downloading() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"cached");
    let gate = Arc::new(Notify::new());
    fake.script_avatar(room_owner("1"), None, png(b"cached"));
    fake.script_avatar(room_owner("2"), Some(gate.clone()), png(b"two"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    fake.wait_for_avatar_calls(2);

    harness.client.logout();
    gate.notify_waiters();

    assert_eq!(
        harness.next_avatars(),
        [AvatarUpdate {
            owner: room_owner("1"),
            path: None
        }]
    );
    harness.assert_no_avatars();
    // The cache keeps the file for the next session.
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"cached".to_vec()));
}

// RICH-004
#[test]
fn the_cache_is_per_server() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("other:3000", &room_owner("1"), "png", b"another server's");
    let harness = logged_in_with_cache_events(&fake, &cache);
    fake.wait_for_avatar_calls(2);
    harness.assert_no_avatars();
}

// RICH-004
#[test]
fn a_pushed_avatar_change_fetches_it_again_and_shows_the_new_file() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"old");
    fake.script_avatar(room_owner("1"), None, png(b"old"));
    fake.script_avatar(room_owner("1"), None, png(b"new"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    fake.wait_for_avatar_calls(2);

    assert!(fake.push(0, Pushed::AvatarChanged(room_owner("1"))));

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"new".to_vec()));
}

// RICH-004
#[test]
fn a_pushed_change_of_an_avatar_nobody_looked_at_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    fake.wait_for_avatar_calls(2);

    assert!(fake.push(0, Pushed::AvatarChanged(user_owner("stranger"))));

    harness.assert_no_avatars();
    assert_eq!(fake.state().avatar_calls.len(), 2);
}

// RICH-004
#[test]
fn a_room_without_an_avatar_of_its_own_is_not_asked_for_one_but_a_direct_message_is() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
            subscription("d1", "bob", rc_api::RoomType::Direct),
        ]),
    );
    let harness = Harness::with_cache(&fake, FakeCache::default());
    harness.login("http://h", "alice");

    // The other user may have one: the server says when asked.
    assert_eq!(fake.wait_for_avatar_calls(1), [user_owner("bob")]);
    harness.assert_no_avatars();
    assert_eq!(fake.state().avatar_calls.len(), 1);
}

// RICH-004
#[test]
fn an_avatar_the_server_doesnt_have_goes_from_the_screen_and_the_cache() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    // Cached when the user had one, or when the server drew one.
    cache.seed("h", &room_owner("1"), "png", b"stale");
    fake.script_avatar(room_owner("1"), None, Ok(None));
    let harness = logged_in_with_cache_events(&fake, &cache);

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    assert_eq!(
        harness.next_avatars(),
        [AvatarUpdate {
            owner: room_owner("1"),
            path: None
        }]
    );
    assert_eq!(cache.bytes("h", &room_owner("1")), None);
}

// RICH-004
#[test]
fn a_pushed_avatar_change_of_a_listed_room_looks_at_it_even_without_an_etag() {
    let fake = Fake::new();
    fake.script_avatar(room_owner("1"), None, png(b"new"));
    let harness = logged_in_harness(&fake);
    assert!(fake.state().avatar_calls.is_empty());

    assert!(fake.push(0, Pushed::AvatarChanged(room_owner("1"))));

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
}

/// The path an image of the tests is uploaded at.
fn image_path(name: &str) -> String {
    format!("/file-upload/id/{name}.png")
}

/// Where the fake cache keeps the image at `image_path(name)`.
fn image_file(name: &str) -> PathBuf {
    PathBuf::from(format!("/cache/h/attachment/{}.png", image_path(name)))
}

fn image_update(name: &str) -> ImageUpdate {
    ImageUpdate {
        path: image_path(name),
        file: Some(image_file(name)),
    }
}

/// A message of room `1` with an image attachment for each of `names`.
fn message_with_images(id: &str, secs: i64, names: &[&str]) -> Message {
    let mut message = message(id, secs);
    message.attachments = names
        .iter()
        .map(|name| Attachment {
            title: format!("{name}.png"),
            link: image_path(name),
            description: None,
            size: None,
            image: Some(ImageAttachment {
                path: image_path(name),
                width: Some(2),
                height: Some(1),
            }),
        })
        .collect();
    message
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn png_file(bytes: &[u8]) -> Result<Image, ApiError> {
    Ok(Image {
        bytes: bytes.to_vec(),
        content_type: Some("image/png".to_owned()),
    })
}

// RICH-005
#[test]
fn the_image_of_an_attachment_is_downloaded_and_sent_once_it_is_stored() {
    let fake = Fake::new();
    fake.script_file(&image_path("a"), None, png_file(b"pixels"));
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    fake.script_history(None, page(vec![message_with_images("m", 1, &["a"])], true));
    harness.client.open_room("1".to_owned().into());

    assert_eq!(harness.next_images(), [image_update("a")]);
    assert_eq!(
        cache.attachment_bytes("h", &image_path("a")),
        Some(b"pixels".to_vec())
    );
}

// RICH-005
#[test]
fn a_cached_image_is_sent_at_once_and_never_downloaded() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_attachment("h", &image_path("a"), "png", b"cached");
    let harness = logged_in_with_cache(&fake, &cache);
    fake.script_history(None, page(vec![message_with_images("m", 1, &["a"])], true));
    harness.client.open_room("1".to_owned().into());

    assert_eq!(harness.next_images(), [image_update("a")]);
    harness.assert_no_images();
    assert!(fake.state().file_calls.is_empty());
}

// RICH-005
#[test]
fn the_image_of_a_live_message_is_wanted() {
    let fake = Fake::new();
    fake.script_file(&image_path("a"), None, png_file(b"pixels"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a"])));

    assert_eq!(harness.next_images(), [image_update("a")]);
}

// RICH-005
#[test]
fn an_image_is_downloaded_once_however_often_it_is_wanted() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_file(&image_path("a"), Some(gate.clone()), png_file(b"pixels"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);
    fake.push(0, Pushed::Message(message_with_images("m1", 1, &["a"])));
    fake.wait_for_file_calls(1);
    // Held by the gate: other messages with the same image find the download in flight.
    fake.push(0, Pushed::Message(message_with_images("m2", 2, &["a"])));
    harness.wait_for_appended("m2");
    gate.notify_one();
    assert_eq!(harness.next_images(), [image_update("a")]);
    fake.push(0, Pushed::Message(message_with_images("m3", 3, &["a"])));
    harness.wait_for_appended("m3");

    assert_eq!(fake.state().file_calls, [image_path("a")]);
}

// RICH-005
#[test]
fn messages_without_an_image_want_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);
    let mut file = message_with_images("f", 1, &["a"]);
    file.attachments[0].image = None;

    fake.push(0, Pushed::Message(file));
    fake.push(0, Pushed::Message(message("t", 2)));
    harness.wait_for_appended("t");

    harness.assert_no_images();
    assert!(fake.state().file_calls.is_empty());
}

// RICH-005
#[test]
fn an_image_of_a_type_the_cache_doesnt_keep_is_not_shown() {
    let fake = Fake::new();
    fake.script_file(
        &image_path("a"),
        None,
        Ok(Image {
            bytes: b"<html>".to_vec(),
            content_type: Some("text/html".to_owned()),
        }),
    );
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a"])));
    fake.wait_for_file_calls(1);

    harness.assert_no_images();
    assert_eq!(cache.attachment_bytes("h", &image_path("a")), None);
}

// RICH-005
#[test]
fn an_image_the_server_refuses_is_not_asked_for_again() {
    let fake = Fake::new();
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m1", 1, &["a"])));
    fake.wait_for_file_calls(1);
    fake.push(0, Pushed::Message(message_with_images("m2", 2, &["a"])));
    harness.wait_for_appended("m2");

    harness.assert_no_images();
    assert_eq!(fake.state().file_calls.len(), 1);
}

// RICH-005
#[test]
fn an_image_that_couldnt_be_downloaded_is_tried_again_when_wanted_again() {
    let fake = Fake::new();
    fake.script_file(
        &image_path("a"),
        None,
        Err(ApiError::Unreachable("down".into())),
    );
    fake.script_file(&image_path("a"), None, png_file(b"pixels"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);
    fake.push(0, Pushed::Message(message_with_images("m1", 1, &["a"])));
    fake.wait_for_file_calls(1);
    harness.assert_no_images();

    fake.push(0, Pushed::Message(message_with_images("m2", 2, &["a"])));

    assert_eq!(harness.next_images(), [image_update("a")]);
    assert_eq!(fake.state().file_calls.len(), 2);
}

// RICH-005
#[test]
fn no_more_than_four_images_download_at_once() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    let names = ["1", "2", "3", "4", "5", "6"];
    for name in names {
        fake.script_file(
            &image_path(name),
            Some(gate.clone()),
            png_file(name.as_bytes()),
        );
    }
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &names)));
    fake.wait_for_file_calls(4);
    // Give a fifth the time to start, wrongly.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.state().file_calls.len(), 4);
    assert_eq!(fake.state().file_max_running, 4);

    gate.notify_waiters();
    fake.wait_for_file_calls(5);
    gate.notify_waiters();
    fake.wait_for_file_calls(6);
    gate.notify_waiters();
    assert!(fake.state().file_max_running <= 4);
    let mut shown = 0;
    while shown < 6 {
        shown += harness.next_images().len();
    }
}

// RICH-005
#[test]
fn logging_out_forgets_the_images_and_drops_what_was_still_downloading() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_attachment("h", &image_path("a"), "png", b"cached");
    let gate = Arc::new(Notify::new());
    fake.script_file(&image_path("b"), Some(gate.clone()), png_file(b"two"));
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);
    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a", "b"])));
    assert_eq!(harness.next_images(), [image_update("a")]);
    fake.wait_for_file_calls(1);

    harness.client.logout();
    gate.notify_waiters();

    assert_eq!(
        harness.next_images(),
        [ImageUpdate {
            path: image_path("a"),
            file: None
        }]
    );
    harness.assert_no_images();
    // The cache keeps the file for the next session.
    assert_eq!(
        cache.attachment_bytes("h", &image_path("a")),
        Some(b"cached".to_vec())
    );
}

// RICH-005
#[test]
fn the_image_cache_is_per_server() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_attachment("other:3000", &image_path("a"), "png", b"another server's");
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a"])));
    fake.wait_for_file_calls(1);

    harness.assert_no_images();
}

fn edited_message(id: &str, secs: i64) -> Message {
    Message {
        kind: MessageKind::Text(format!("{id} edited").as_str().into()),
        edited: true,
        ..message(id, secs)
    }
}

fn reacted_message(id: &str, secs: i64) -> Message {
    Message {
        reactions: vec![crate::domain::Reaction {
            emoji: ":thumbsup:".to_owned(),
            usernames: vec!["bob".to_owned()],
        }],
        ..message(id, secs)
    }
}

fn deleted(room: &str, id: &str) -> Pushed {
    Pushed::MessageDeleted {
        room: room.to_owned().into(),
        id: id.to_owned().into(),
    }
}

fn removal(index: usize) -> Event {
    Event::Timeline(vec![TimelineChange::Removed { index }])
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn changes(updated: Vec<Message>, deleted: &[&str]) -> Result<Changes, ApiError> {
    Ok(Changes {
        updated,
        deleted: deleted.iter().map(|&id| id.to_owned().into()).collect(),
    })
}

/// Opens room 2 and then room 1 again, past the events of both. Room 2 is empty. The newest page of
/// room 1 is `newest`, newest first, after `page_gate`.
fn leave_and_come_back(
    fake: &Fake,
    harness: &Harness,
    page_gate: Option<Arc<Notify>>,
    newest: Vec<Message>,
) {
    let visited = fake
        .state()
        .history_calls
        .iter()
        .any(|call| call.room == "2");
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, visited, None),
        status("2", false, false, true, None),
    ]);
    fake.script_history(page_gate, page(newest, false));
    harness.client.open_room("1".to_owned().into());
}

// RICH-006
#[test]
fn the_connection_follows_the_open_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // Nothing is open yet: nothing to tell.
    assert_eq!(fake.watched(0), []);
    open_first_room(&fake, &harness, false);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 1 })]);
    harness.expect_events(closed_events());

    let id = |room: &str| Some(RoomId::from(room.to_owned()));
    assert_eq!(fake.wait_for_watched(0, 3), [id("1"), id("2"), None]);
}

// RICH-006
#[test]
fn a_new_connection_is_told_which_room_is_open() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.end_connection(0);
    harness.expect_events(vec![
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);

    assert_eq!(
        fake.wait_for_watched(1, 1),
        [Some(RoomId::from("1".to_owned()))]
    );
}

// RICH-006
#[test]
fn a_deletion_removes_the_message_from_the_open_room_and_from_what_is_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, deleted("1", "a")));
    harness.expect_events(vec![removal(0)]);

    leave_and_come_back(&fake, &harness, None, vec![message("b", 2)]);
    // What was kept has no `a`.
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// RICH-006
#[test]
fn a_deletion_of_a_message_not_held_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, deleted("1", "nope")));
    assert!(fake.push(0, deleted("2", "a")));
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);
}

// RICH-006
#[test]
fn a_deletion_in_a_room_that_is_not_open_is_applied_to_what_is_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert!(fake.push(0, deleted("1", "b")));
    // In order after the push: once this has come, the deletion was taken in.
    assert!(fake.push(0, pushed_message("2", "x", 9)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        Message {
            room_id: "2".to_owned().into(),
            ..message("x", 9)
        },
    ])])]);

    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1)]),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
}

// RICH-006
#[test]
fn a_message_deleted_while_the_page_loads_does_not_come_back_with_the_page() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(
            vec![message("c", 3), message("b", 2), message("a", 1)],
            false,
        ),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);
    // One that is waiting for the page, and one that is only in the page.
    assert!(fake.push(0, pushed_message("1", "d", 4)));
    assert!(fake.push(0, deleted("1", "d")));
    assert!(fake.push(0, deleted("1", "b")));
    assert!(fake.push(
        0,
        pushed_subscription("3", "alpha", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        room: room("3", "alpha", RoomKind::Channel),
    })]);
    gate.notify_one();

    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("c", 3)]),
        status("1", false, false, false, None),
    ]);
}

// RICH-006
#[test]
fn a_live_edit_updates_the_message_in_place_and_is_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(edited_message("a", 1))));
    harness.expect_events(vec![updated(0, edited_message("a", 1))]);

    let newest = vec![message("b", 2), edited_message("a", 1)];
    leave_and_come_back(&fake, &harness, None, newest);
    harness.expect_events(vec![
        reset_event(Some("1"), vec![edited_message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// RICH-007
#[test]
fn a_live_reaction_updates_the_message_and_taking_it_back_updates_it_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(reacted_message("b", 2))));
    harness.expect_events(vec![updated(1, reacted_message("b", 2))]);
    assert!(fake.push(0, Pushed::Message(message("b", 2))));
    harness.expect_events(vec![updated(1, message("b", 2))]);
}

// RICH-006
#[test]
fn a_live_edit_of_a_message_further_back_than_what_is_held_is_not_shown() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(edited_message("old", 0))));
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);
}

// RICH-006
#[test]
fn a_room_that_was_never_up_to_date_asks_for_no_changes() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert_eq!(fake.state().changes_calls, []);
}

// RICH-006
#[test]
fn reopening_a_room_catches_up_with_what_changed_while_it_was_closed() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let before = jiff::Timestamp::now();
    open_first_room(&fake, &harness, false);
    let after = jiff::Timestamp::now();
    fake.script_changes("1", None, changes(vec![edited_message("a", 1)], &["b"]));

    let gate = Arc::new(Notify::new());
    leave_and_come_back(
        &fake,
        &harness,
        Some(Arc::clone(&gate)),
        vec![edited_message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        Event::Timeline(vec![
            TimelineChange::Updated {
                index: 0,
                message: edited_message("a", 1),
            },
            TimelineChange::Removed { index: 1 },
        ]),
    ]);
    // A minute earlier than the room was last up to date, for the difference of the clocks.
    let calls = fake.wait_for_changes_calls("1", 1);
    assert_eq!(calls[0].room, "1");
    let margin = Duration::from_secs(60);
    assert!(calls[0].since >= before - margin && calls[0].since <= after - margin);
    gate.notify_one();
    // The page, made after the deletion, has the new copy only.
    harness.expect_events(vec![status("1", false, false, false, None)]);
}

// RICH-006
#[test]
fn reconnecting_catches_up_the_open_room_with_edits_deletions_and_reactions() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // Held: a, b. Meanwhile b was edited and reacted to, a deleted, and c written (which comes
    // with the page, not with the changes).
    fake.script_changes(
        "1",
        None,
        changes(vec![edited_message("b", 2), message("c", 3)], &["a"]),
    );
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(vec![message("c", 3), edited_message("b", 2)], false),
    );
    fake.end_connection(0);

    // Not a reset: the rows of the list stay.
    harness.expect_events(vec![
        status("1", true, false, false, None),
        Event::Timeline(vec![
            TimelineChange::Updated {
                index: 1,
                message: edited_message("b", 2),
            },
            TimelineChange::Removed { index: 0 },
        ]),
    ]);
    gate.notify_one();
    harness.expect_events(vec![
        Event::Timeline(vec![TimelineChange::Appended(vec![message("c", 3)])]),
        status("1", false, false, false, None),
    ]);
    assert_eq!(fake.state().changes_calls.len(), 1);
}

// RICH-006
#[test]
fn a_catch_up_changes_only_what_is_held() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes(
        "1",
        None,
        changes(
            vec![message("new", 9), edited_message("old", 0)],
            &["unheard-of"],
        ),
    );
    let gate = Arc::new(Notify::new());
    leave_and_come_back(
        &fake,
        &harness,
        Some(Arc::clone(&gate)),
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_changes_calls("1", 1);
    // Pushes are taken in order, so once this one is out the changes have been applied too.
    assert!(fake.push(
        0,
        pushed_subscription("3", "alpha", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        room: room("3", "alpha", RoomKind::Channel),
    })]);
    gate.notify_one();
    harness.expect_events(vec![status("1", false, false, false, None)]);
}

// RICH-006
#[test]
fn a_catch_up_answer_for_a_room_that_was_closed_or_a_previous_request_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let stale = Arc::new(Notify::new());
    fake.script_changes("1", Some(Arc::clone(&stale)), changes(vec![], &["a"]));
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    fake.wait_for_changes_calls("1", 1);

    // Away again and back: a second request, answered at once.
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    fake.wait_for_changes_calls("1", 2);

    // The first one is answered now: it belongs to a visit that is over.
    stale.notify_one();
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);

    // And one for a room that is not open any more.
    let closed = Arc::new(Notify::new());
    fake.script_changes("1", Some(Arc::clone(&closed)), changes(vec![], &["b"]));
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, true, None),
        status("2", false, false, true, None),
    ]);
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), message("c", 3)],
        ),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
    fake.wait_for_changes_calls("1", 3);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, true, None),
        status("2", false, false, true, None),
    ]);
    closed.notify_one();
    assert!(fake.push(0, pushed_message("2", "x", 9)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        Message {
            room_id: "2".to_owned().into(),
            ..message("x", 9)
        },
    ])])]);
}

// RICH-006
#[test]
fn a_failed_catch_up_is_tried_again_from_the_same_time_and_a_successful_one_moves_it_on() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes(
        "1",
        None,
        Err(ApiError::Unreachable("no connection".to_owned())),
    );
    // A failed answer moves nothing, whether it comes before the room is left or after.
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    // A successful one must be taken in before the room is left, or it is stale: it brings an
    // edit, which shows when it has come, and the page waits for it.
    fake.script_changes("1", None, changes(vec![edited_message("b", 2)], &[]));
    let gate = Arc::new(Notify::new());
    leave_and_come_back(
        &fake,
        &harness,
        Some(Arc::clone(&gate)),
        vec![edited_message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        updated(1, edited_message("b", 2)),
    ]);
    gate.notify_one();
    harness.expect_events(vec![status("1", false, false, false, None)]);
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![edited_message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), edited_message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    let calls = fake.wait_for_changes_calls("1", 3);
    assert_eq!(calls[0].since, calls[1].since);
    assert!(calls[2].since > calls[1].since);
}

// RICH-006
#[test]
fn a_catch_up_with_a_dead_token_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes("1", None, Err(ApiError::Unauthorized));
    let gate = Arc::new(Notify::new());
    leave_and_come_back(&fake, &harness, Some(gate), vec![message("a", 1)]);
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}

/// A message of room `1` written by somebody else.
fn message_of_bob(id: &str, secs: i64) -> Message {
    let mut message = message(id, secs);
    message.author.id = "bob-id".to_owned().into();
    message.author.username = "bob".to_owned();
    message
}

fn action_event(id: &str, action: MessageAction, outcome: Result<(), ActionError>) -> Event {
    Event::MessageAction {
        id: id.to_owned().into(),
        action,
        outcome,
    }
}

/// The message as an edit to `text` stores it, sent at `secs`.
fn edited_copy(id: &str, text: &str, secs: i64) -> Message {
    Message {
        edited: true,
        ..server_copy("1", id, text, secs)
    }
}

fn refused() -> ApiError {
    ApiError::Server {
        status: 400,
        message: "error-action-not-allowed".to_owned(),
    }
}

// ACT-001
#[test]
fn an_edit_replaces_the_message_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Ok(2));

    harness
        .client
        .edit_message("b".to_owned().into(), "  bee\n");

    harness.expect_events(vec![
        updated(1, edited_copy("b", "bee", 2)),
        action_event("b", MessageAction::Edit, Ok(())),
    ]);
    assert_eq!(
        fake.state().edit_calls,
        [EditCall {
            room: "1".into(),
            id: "b".into(),
            text: "bee".into()
        }]
    );
    // The push of the same copy changes nothing.
    assert!(fake.push(0, Pushed::Message(edited_copy("b", "bee", 2))));
    flush_pushed(&fake, &harness, "1");
}

// ACT-001
#[test]
fn a_refused_edit_is_reported_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Err(refused()));

    harness.client.edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::Edit,
        Err(ActionError::Server(refused().to_string())),
    )]);
    flush_pushed(&fake, &harness, "1");
}

// ACT-001
#[test]
fn an_edit_that_cannot_reach_the_server_is_reported_as_such() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Err(ApiError::Unreachable("refused".into())));

    harness.client.edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::Edit,
        Err(ActionError::Unreachable),
    )]);
}

// ACT-001
#[test]
fn a_blank_edit_is_refused_and_the_same_text_succeeds_without_a_request() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness.client.edit_message("b".to_owned().into(), " \n ");
    harness.client.edit_message("b".to_owned().into(), " b ");

    harness.expect_events(vec![
        action_event("b", MessageAction::Edit, Err(ActionError::Empty)),
        action_event("b", MessageAction::Edit, Ok(())),
    ]);
    assert!(fake.state().edit_calls.is_empty());
}

// ACT-001
#[test]
fn only_sent_text_messages_of_the_user_in_the_open_room_are_edited() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(message_of_bob("c", 3))));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        message_of_bob("c", 3),
    ])])]);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(gate), Ok(7));
    harness.client.send_message("1".to_owned().into(), "unsent");
    let pending = expect_pending(&harness);

    for id in ["c", pending.id.as_str(), "nope"] {
        harness.client.edit_message(id.to_owned().into(), "x");
        harness.expect_events(vec![action_event(
            id,
            MessageAction::Edit,
            Err(ActionError::NotAllowed(MessageAction::Edit)),
        )]);
    }
    assert!(fake.state().edit_calls.is_empty());
}

// ACT-001
#[test]
fn a_message_of_a_room_that_is_not_open_is_not_edited() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    open_the_other_room(&harness);

    harness.client.edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::Edit,
        Err(ActionError::NotAllowed(MessageAction::Edit)),
    )]);
    assert!(fake.state().edit_calls.is_empty());
}

// ACT-001, ACT-002
#[test]
fn the_servers_settings_decide_what_can_be_changed() {
    let fake = Fake::new();
    fake.state().settings = Some(Ok(rc_api::MessageSettings {
        editing_allowed: false,
        edit_minutes: 0,
        deleting_allowed: true,
        delete_minutes: 5,
    }));
    let harness = logged_in_harness(&fake);
    let policy = MessagePolicy::from(fake.state().settings.clone().unwrap().unwrap());
    harness.expect_events(vec![Event::MessagePolicy(policy)]);
    open_first_room(&fake, &harness, false);

    // Editing is off, and the messages of 1970 are past the five minutes for deleting.
    harness.client.edit_message("b".to_owned().into(), "bee");
    harness.client.delete_message("b".to_owned().into());

    harness.expect_events(vec![
        action_event(
            "b",
            MessageAction::Edit,
            Err(ActionError::NotAllowed(MessageAction::Edit)),
        ),
        action_event(
            "b",
            MessageAction::Delete,
            Err(ActionError::NotAllowed(MessageAction::Delete)),
        ),
    ]);
    assert!(fake.state().edit_calls.is_empty());
    assert!(fake.state().delete_calls.is_empty());
}

// ACT-001, ACT-002
#[test]
fn the_policy_is_sent_when_it_changes_and_forgotten_at_logout() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_login(None, Ok(()));
    let limited = rc_api::MessageSettings {
        edit_minutes: 15,
        ..rc_api::MessageSettings::default()
    };
    fake.state().settings = Some(Ok(limited));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
        Event::MessagePolicy(MessagePolicy::from(limited)),
    ]);

    harness.client.logout();
    harness.expect_events(vec![
        session_event(logged_out(None)),
        Event::MessagePolicy(MessagePolicy::default()),
    ]);
}

// ACT-001, ACT-002
#[test]
fn a_failed_settings_fetch_keeps_the_policy_and_the_session() {
    let fake = Fake::new();
    fake.state().settings = Some(Err(ApiError::Unreachable("refused".into())));
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Ok(2));

    harness.client.edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![
        updated(1, edited_copy("b", "bee", 2)),
        action_event("b", MessageAction::Edit, Ok(())),
    ]);
}

// ACT-002
#[test]
fn a_deletion_removes_the_message_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness.client.delete_message("a".to_owned().into());

    harness.expect_events(vec![
        removal(0),
        action_event("a", MessageAction::Delete, Ok(())),
    ]);
    assert_eq!(
        fake.state().delete_calls,
        [("1".to_owned(), "a".to_owned())]
    );
    // The push of the deletion changes nothing.
    assert!(fake.push(0, deleted("1", "a")));
    flush_pushed(&fake, &harness, "1");
}

// ACT-002
#[test]
fn a_refused_deletion_is_reported_and_the_message_stays() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_delete(None, Err(refused()));

    harness.client.delete_message("a".to_owned().into());

    harness.expect_events(vec![action_event(
        "a",
        MessageAction::Delete,
        Err(ActionError::Server(refused().to_string())),
    )]);
    flush_pushed(&fake, &harness, "1");
}

// ACT-002
#[test]
fn only_sent_messages_of_the_user_in_the_open_room_are_deleted() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(message_of_bob("c", 3))));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        message_of_bob("c", 3),
    ])])]);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(gate), Ok(7));
    harness.client.send_message("1".to_owned().into(), "unsent");
    let pending = expect_pending(&harness);

    for id in ["c", pending.id.as_str(), "nope"] {
        harness.client.delete_message(id.to_owned().into());
        harness.expect_events(vec![action_event(
            id,
            MessageAction::Delete,
            Err(ActionError::NotAllowed(MessageAction::Delete)),
        )]);
    }
    assert!(fake.state().delete_calls.is_empty());
}

// ACT-001, ACT-002
#[test]
fn nothing_happens_when_logged_out() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    harness.client.edit_message("b".to_owned().into(), "bee");
    harness.client.delete_message("b".to_owned().into());

    // The first event after these is the login's.
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    assert!(fake.state().edit_calls.is_empty());
    assert!(fake.state().delete_calls.is_empty());
}

// ACT-001
#[test]
fn an_edit_answered_after_logout_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_edit(Some(Arc::clone(&gate)), Ok(2));
    harness.client.edit_message("b".to_owned().into(), "bee");
    wait_until(|| (!fake.state().edit_calls.is_empty()).then_some(()));

    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
    gate.notify_one();
    fake.wait_for_edits_returned(1);

    // Nothing came out of the answer: the next events are the new session's.
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
}

// ACT-002
#[test]
fn a_deletion_answered_after_logout_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_delete(Some(Arc::clone(&gate)), Ok(()));
    harness.client.delete_message("a".to_owned().into());
    wait_until(|| (!fake.state().delete_calls.is_empty()).then_some(()));

    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
    gate.notify_one();
    fake.wait_for_deletes_returned(1);

    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
}

// ACT-001, ACT-002
#[test]
fn a_dead_token_on_an_edit_or_a_deletion_expires_the_session() {
    for delete in [false, true] {
        let fake = Fake::new();
        let harness = logged_in_harness(&fake);
        open_first_room(&fake, &harness, false);
        if delete {
            fake.script_delete(None, Err(ApiError::Unauthorized));
            harness.client.delete_message("a".to_owned().into());
        } else {
            fake.script_edit(None, Err(ApiError::Unauthorized));
            harness.client.edit_message("b".to_owned().into(), "bee");
        }
        harness.expect_events(vec![
            session_event(logged_out(Some(LoginFailure::SessionExpired))),
            rooms_event(vec![]),
        ]);
        harness.expect_events(closed_events());
    }
}
