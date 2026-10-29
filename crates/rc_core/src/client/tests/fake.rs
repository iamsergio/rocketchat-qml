// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A fake server API, secret store and file cache, scripted by the tests.

use super::*;

pub(super) struct LoginScript {
    /// Holds the login until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<(), ApiError>,
}

pub(super) struct FetchScript {
    /// Holds the fetch until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<Vec<rc_api::Subscription>, ApiError>,
}

pub(super) struct RealtimeScript {
    /// Holds the connection until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    /// What is already waiting on the connection when it opens.
    pub(super) initial: Vec<Pushed>,
    pub(super) result: Result<(), ApiError>,
}

pub(super) struct HistoryScript {
    /// Holds the fetch until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<Page, ApiError>,
}

pub(super) struct ThreadScript {
    /// Holds the fetch until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<Page, ApiError>,
}

pub(super) struct ThreadsListScript {
    /// Holds the fetch until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<ThreadsPage, ApiError>,
}

pub(super) struct MessageScript {
    /// Holds the fetch until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<Message, ApiError>,
}

pub(super) struct ChangesScript {
    /// Holds the request until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<Changes, ApiError>,
}

pub(super) struct SendScript {
    /// Holds the send until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    /// The server's time of the stored message, in seconds after the epoch, or why it refused.
    pub(super) result: Result<i64, ApiError>,
}

pub(super) struct UploadScript {
    /// Holds the upload until the test notifies it, after the progress was reported.
    pub(super) gate: Option<Arc<Notify>>,
    /// What the upload reports as sent, in order, before it ends.
    pub(super) progress: Vec<u64>,
    /// The server's id of the file, or why it refused.
    pub(super) result: Result<String, ApiError>,
}

pub(super) struct DeleteScript {
    /// Holds the deletion until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<(), ApiError>,
}

pub(super) struct AvatarScript {
    /// Holds the download until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    /// `None`: the owner has no avatar of its own.
    pub(super) result: Result<Option<Image>, ApiError>,
}

pub(super) struct FileScript {
    /// Holds the download until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<Image, ApiError>,
}

/// A call that waits for its gate, if it has one, then answers.
pub(super) struct Scripted<T> {
    /// Holds the call until the test notifies it.
    pub(super) gate: Option<Arc<Notify>>,
    pub(super) result: Result<T, ApiError>,
}

/// What a request of the directory was asked: what it looks for, and how many.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DirectoryCall {
    pub(super) kind: &'static str,
    pub(super) text: String,
    pub(super) count: usize,
}

/// What a request of a room's members was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MemberCall {
    pub(super) room: String,
    pub(super) filter: String,
    pub(super) count: usize,
}

/// What `send_message` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SendCall {
    pub(super) room: String,
    pub(super) id: String,
    pub(super) text: String,
    pub(super) thread: Option<ThreadTarget>,
}

/// The message the server makes of the file `file` confirmed with `text`, stored at `secs`.
pub(super) fn uploaded_message(
    room: &str,
    file: &str,
    text: &str,
    secs: i64,
    thread: Option<&MessageId>,
) -> Message {
    Message {
        thread_id: thread.cloned(),
        attachments: vec![Attachment {
            title: file.to_owned(),
            link: format!("/file-upload/{file}/{file}"),
            description: None,
            size: Some(1),
            image: None,
        }],
        ..server_copy(room, &format!("msg-{file}"), text, secs)
    }
}

/// What `upload_file` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UploadCall {
    pub(super) room: String,
    pub(super) path: PathBuf,
    pub(super) name: String,
}

/// What `confirm_upload` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConfirmCall {
    pub(super) room: String,
    pub(super) file: String,
    pub(super) text: String,
    pub(super) thread: Option<String>,
}

/// What `edit_message` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditCall {
    pub(super) room: String,
    pub(super) id: String,
    pub(super) text: String,
}

/// What `react` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReactCall {
    pub(super) id: String,
    pub(super) emoji: String,
    pub(super) react: bool,
}

/// What the room commands that have a call of their own were asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RoomCall {
    SetFavorite { room: String, favorite: bool },
    MarkUnread { room: String },
    Leave { room: String, kind: RoomKind },
}

/// What `thread_messages` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThreadCall {
    pub(super) thread: String,
    pub(super) offset: usize,
    pub(super) count: usize,
}

/// What `threads_list` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThreadsListCall {
    pub(super) room: String,
    pub(super) filter: ThreadFilter,
    pub(super) offset: usize,
    pub(super) count: usize,
}

/// What `changes_since` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChangesCall {
    pub(super) room: String,
    pub(super) since: jiff::Timestamp,
}

/// What `history` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HistoryCall {
    pub(super) room: String,
    pub(super) kind: RoomKind,
    pub(super) before: Option<jiff::Timestamp>,
    pub(super) count: usize,
}

#[derive(Default)]
pub(super) struct FakeState {
    /// The fakes that answer for other servers than this one's, which stands in for the rest.
    pub(super) servers: HashMap<String, Fake>,
    pub(super) version: Option<Result<ServerVersion, ApiError>>,
    pub(super) connect_error: Option<ApiError>,
    pub(super) logins: VecDeque<LoginScript>,
    /// The id the server gives a user who logs in, by username; `uid` for the others.
    pub(super) user_ids: HashMap<String, String>,
    pub(super) logout_result: Option<Result<(), ApiError>>,
    /// How long `logout` takes.
    pub(super) logout_delay: Duration,
    pub(super) me_result: Option<Result<User, ApiError>>,
    pub(super) me_gate: Option<Arc<Notify>>,
    pub(super) login_calls: Vec<(String, String)>,
    pub(super) logout_calls: Vec<(String, String)>,
    /// Logouts that ran to the end.
    pub(super) logouts_done: usize,
    pub(super) me_calls: Vec<(String, String)>,
    /// One per room fetch; an empty room list once they run out.
    pub(super) fetches: VecDeque<FetchScript>,
    pub(super) subscriptions_calls: Vec<(String, String)>,
    /// What `custom_emoji` answers: no emoji if unset.
    pub(super) emoji: Option<Result<Vec<rc_api::CustomEmoji>, ApiError>>,
    /// One per download of an owner's avatar; no avatar of its own once they run out.
    pub(super) avatars: HashMap<AvatarOwner, VecDeque<AvatarScript>>,
    pub(super) avatar_calls: Vec<AvatarOwner>,
    pub(super) avatar_running: usize,
    /// The most downloads that ran at the same time.
    pub(super) avatar_max_running: usize,
    /// One per download of a file by its path; a 404 once they run out.
    pub(super) files: HashMap<String, VecDeque<FileScript>>,
    pub(super) file_calls: Vec<String>,
    pub(super) file_running: usize,
    /// The most file downloads that ran at the same time.
    pub(super) file_max_running: usize,
    /// One per download of a custom emoji's image by name and extension (`party.gif`); a 404 once
    /// they run out.
    pub(super) emoji_images: HashMap<String, VecDeque<FileScript>>,
    pub(super) emoji_image_calls: Vec<String>,
    pub(super) emoji_image_running: usize,
    /// The most emoji image downloads that ran at the same time.
    pub(super) emoji_image_max_running: usize,
    /// One per history fetch; an empty page at the start once they run out.
    pub(super) histories: VecDeque<HistoryScript>,
    pub(super) history_calls: Vec<HistoryCall>,
    /// One per thread fetch; an empty page at the start once they run out.
    pub(super) threads: VecDeque<ThreadScript>,
    pub(super) thread_calls: Vec<ThreadCall>,
    /// One per fetch of a page of a room's threads; an empty last page once they run out.
    pub(super) thread_lists: VecDeque<ThreadsListScript>,
    pub(super) thread_list_calls: Vec<ThreadsListCall>,
    /// One per fetch of a message by its id; not found once they run out.
    pub(super) messages: VecDeque<MessageScript>,
    /// The ids of the messages fetched.
    pub(super) message_calls: Vec<String>,
    /// One per catch-up of a room; no changes once they run out.
    pub(super) changes: HashMap<String, VecDeque<ChangesScript>>,
    pub(super) changes_calls: Vec<ChangesCall>,
    /// One per send; stored at second 1000 once they run out.
    pub(super) sends: VecDeque<SendScript>,
    pub(super) send_calls: Vec<SendCall>,
    /// Sends that got past their gate.
    pub(super) sends_returned: usize,
    /// One per upload of a file; a file named `file<n>` once they run out.
    pub(super) uploads: VecDeque<UploadScript>,
    pub(super) upload_calls: Vec<UploadCall>,
    /// Uploads that got past their gate.
    pub(super) uploads_returned: usize,
    /// One per confirmation, with the server's time of the stored message as for the sends; stored
    /// at second 1000 once they run out. The message is `msg-<file>`, with an attachment.
    pub(super) confirms: VecDeque<SendScript>,
    pub(super) confirm_calls: Vec<ConfirmCall>,
    /// Confirmations that got past their gate.
    pub(super) confirms_returned: usize,
    /// What `site_url` answers: the server has none if unset.
    pub(super) site_url: Option<Result<Option<String>, ApiError>>,
    /// What `message_settings` answers: the defaults if unset.
    pub(super) settings: Option<Result<rc_api::MessageSettings, ApiError>>,
    /// One per edit, with the server's time of the stored message as for the sends; refused as
    /// not allowed once they run out.
    pub(super) edits: VecDeque<SendScript>,
    pub(super) edit_calls: Vec<EditCall>,
    /// Edits that got past their gate.
    pub(super) edits_returned: usize,
    /// One per deletion; a success once they run out.
    pub(super) deletes: VecDeque<DeleteScript>,
    /// The `(room, id)` of each deletion asked for.
    pub(super) delete_calls: Vec<(String, String)>,
    /// Deletions that got past their gate.
    pub(super) deletes_returned: usize,
    /// One per reaction; a success once they run out.
    pub(super) reacts: VecDeque<DeleteScript>,
    pub(super) react_calls: Vec<ReactCall>,
    /// Reactions that got past their gate.
    pub(super) reacts_returned: usize,
    /// One per room command (`set_favorite`, `mark_unread`, `leave_room`); a success once they
    /// run out.
    pub(super) room_actions: VecDeque<DeleteScript>,
    pub(super) room_calls: Vec<RoomCall>,
    /// Room commands that got past their gate.
    pub(super) room_actions_returned: usize,
    /// One per request for the directory's users, and one per request for its channels; nothing
    /// found once they run out.
    pub(super) directory_people: VecDeque<Scripted<Found<Person>>>,
    pub(super) directory_channels: VecDeque<Scripted<Found<DirectoryChannel>>>,
    pub(super) directory_calls: Vec<DirectoryCall>,
    /// One per request for a room's members; nobody found once they run out.
    pub(super) members: VecDeque<Scripted<Found<Member>>>,
    pub(super) member_calls: Vec<MemberCall>,
    /// One per `create_direct`; refused once they run out.
    pub(super) directs: VecDeque<Scripted<RoomId>>,
    pub(super) direct_calls: Vec<String>,
    /// One per `join_channel`; a success once they run out.
    pub(super) joins: VecDeque<Scripted<()>>,
    pub(super) join_calls: Vec<String>,
    /// One per `room_info`; refused once they run out.
    pub(super) infos: VecDeque<Scripted<rc_api::RoomInfo>>,
    pub(super) info_calls: Vec<String>,
    /// What `subscription` answers for a room: not in it if unset.
    pub(super) own_subscriptions: HashMap<String, Result<Option<rc_api::Subscription>, ApiError>>,
    pub(super) subscription_calls: Vec<String>,
    /// One per mark-as-read call; a success once they run out.
    pub(super) mark_reads: VecDeque<Result<(), ApiError>>,
    /// The rooms `mark_read` was asked for.
    pub(super) mark_read_calls: Vec<String>,
    /// One per realtime connection; an open one with nothing waiting once they run out.
    pub(super) realtimes: VecDeque<RealtimeScript>,
    pub(super) realtime_calls: Vec<(String, String)>,
    /// When each of them came.
    pub(super) realtime_times: Vec<Instant>,
    /// The server's end of every connection opened, in order. `None` once the test ended it.
    pub(super) connections: Vec<Option<UnboundedSender<Pushed>>>,
    /// What each connection was told about the open room, not yet moved to `watch_log`.
    pub(super) watches: Vec<UnboundedReceiver<Option<RoomId>>>,
    /// What each connection was told about the open room, in order.
    pub(super) watch_log: Vec<Vec<Option<RoomId>>>,
    /// What each connection was told about threads read, not yet moved to `read_log`.
    pub(super) reads: Vec<UnboundedReceiver<MessageId>>,
    /// The ids of the threads each connection was told were read.
    pub(super) read_log: Vec<Vec<MessageId>>,
}

/// Scripted results, plus a record of what it was asked.
#[derive(Clone, Default)]
pub(super) struct Fake(pub(super) Arc<Mutex<FakeState>>);

impl Fake {
    pub(super) fn new() -> Self {
        let fake = Self::default();
        fake.state().version = Some(Ok(ServerVersion { major: 8, minor: 0 }));
        fake.state().me_result = Some(Ok(user("alice")));
        fake
    }

    /// Makes `other` answer for `server`, so that two accounts can be scripted apart.
    pub(super) fn serve(&self, server: &str, other: &Self) {
        self.state()
            .servers
            .insert(server.to_owned(), other.clone());
    }

    pub(super) fn state(&self) -> MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }

    pub(super) fn script_login(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state().logins.push_back(LoginScript { gate, result });
    }

    pub(super) fn script_fetch(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<Vec<rc_api::Subscription>, ApiError>,
    ) {
        self.state().fetches.push_back(FetchScript { gate, result });
    }

    pub(super) fn script_avatar(
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
    pub(super) fn wait_for_avatar_calls(&self, count: usize) -> Vec<AvatarOwner> {
        wait_until(|| {
            let calls = self.state().avatar_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_file(
        &self,
        path: &str,
        gate: Option<Arc<Notify>>,
        result: Result<Image, ApiError>,
    ) {
        self.state()
            .files
            .entry(path.to_owned())
            .or_default()
            .push_back(FileScript { gate, result });
    }

    pub(super) fn script_emoji_image(
        &self,
        file: &str,
        gate: Option<Arc<Notify>>,
        result: Result<Image, ApiError>,
    ) {
        self.state()
            .emoji_images
            .entry(file.to_owned())
            .or_default()
            .push_back(FileScript { gate, result });
    }

    /// The download tasks have started and are inside `emoji_image`.
    pub(super) fn wait_for_emoji_image_calls(&self, count: usize) -> Vec<String> {
        wait_until(|| {
            let calls = self.state().emoji_image_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The download tasks have started and are inside `file`.
    pub(super) fn wait_for_file_calls(&self, count: usize) -> Vec<String> {
        wait_until(|| {
            let calls = self.state().file_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_history(&self, gate: Option<Arc<Notify>>, result: Result<Page, ApiError>) {
        self.state()
            .histories
            .push_back(HistoryScript { gate, result });
    }

    pub(super) fn script_thread(&self, gate: Option<Arc<Notify>>, result: Result<Page, ApiError>) {
        self.state()
            .threads
            .push_back(ThreadScript { gate, result });
    }

    pub(super) fn script_threads_list(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<ThreadsPage, ApiError>,
    ) {
        self.state()
            .thread_lists
            .push_back(ThreadsListScript { gate, result });
    }

    /// The fetch tasks of thread lists have started and are inside `threads_list`.
    pub(super) fn wait_for_threads_list_calls(&self, count: usize) -> Vec<ThreadsListCall> {
        wait_until(|| {
            let calls = self.state().thread_list_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_message(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<Message, ApiError>,
    ) {
        self.state()
            .messages
            .push_back(MessageScript { gate, result });
    }

    /// The fetch tasks of messages have started and are inside `message`.
    pub(super) fn wait_for_message_calls(&self, count: usize) -> Vec<String> {
        wait_until(|| {
            let calls = self.state().message_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The thread fetch task has started and is inside `thread_messages`.
    pub(super) fn wait_for_thread_calls(&self, count: usize) -> Vec<ThreadCall> {
        wait_until(|| {
            let calls = self.state().thread_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_changes(
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
    pub(super) fn wait_for_changes_calls(&self, room: &str, count: usize) -> Vec<ChangesCall> {
        wait_until(|| {
            let calls: Vec<_> = (self.state().changes_calls.iter())
                .filter(|call| call.room == room)
                .cloned()
                .collect();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// What connection `index` was told about the open room so far.
    pub(super) fn watched(&self, index: usize) -> Vec<Option<RoomId>> {
        self.wait_for_connections(index + 1);
        let mut state = self.state();
        let state = &mut *state;
        state.watch_log.resize(state.watches.len(), Vec::new());
        while let Ok(room) = state.watches[index].try_recv() {
            state.watch_log[index].push(room);
        }
        state.watch_log[index].clone()
    }

    /// The threads connection `index` was told the user read, so far.
    pub(super) fn read_threads(&self, index: usize) -> Vec<MessageId> {
        self.wait_for_connections(index + 1);
        let mut state = self.state();
        let state = &mut *state;
        state.read_log.resize(state.reads.len(), Vec::new());
        while let Ok(thread) = state.reads[index].try_recv() {
            state.read_log[index].push(thread);
        }
        state.read_log[index].clone()
    }

    /// Waits until connection `index` has been told `count` times that a thread was read.
    pub(super) fn wait_for_read_threads(&self, index: usize, count: usize) -> Vec<MessageId> {
        wait_until(|| {
            let log = self.read_threads(index);
            (log.len() >= count).then_some(log)
        })
    }

    /// Waits until connection `index` has been told `count` times about the open room.
    pub(super) fn wait_for_watched(&self, index: usize, count: usize) -> Vec<Option<RoomId>> {
        wait_until(|| {
            let log = self.watched(index);
            (log.len() >= count).then_some(log)
        })
    }

    pub(super) fn script_send(&self, gate: Option<Arc<Notify>>, result: Result<i64, ApiError>) {
        self.state().sends.push_back(SendScript { gate, result });
    }

    pub(super) fn script_upload(
        &self,
        gate: Option<Arc<Notify>>,
        progress: Vec<u64>,
        result: Result<&str, ApiError>,
    ) {
        self.state().uploads.push_back(UploadScript {
            gate,
            progress,
            result: result.map(str::to_owned),
        });
    }

    pub(super) fn script_confirm(&self, gate: Option<Arc<Notify>>, result: Result<i64, ApiError>) {
        self.state().confirms.push_back(SendScript { gate, result });
    }

    /// The upload tasks have started and are inside `upload_file`.
    pub(super) fn wait_for_upload_calls(&self, count: usize) -> Vec<UploadCall> {
        wait_until(|| {
            let calls = self.state().upload_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn wait_for_confirm_calls(&self, count: usize) -> Vec<ConfirmCall> {
        wait_until(|| {
            let calls = self.state().confirm_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn wait_for_confirms_returned(&self, count: usize) {
        wait_until(|| (self.state().confirms_returned >= count).then_some(()));
        std::thread::sleep(Duration::from_millis(20));
    }

    pub(super) fn script_edit(&self, gate: Option<Arc<Notify>>, result: Result<i64, ApiError>) {
        self.state().edits.push_back(SendScript { gate, result });
    }

    pub(super) fn script_delete(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state()
            .deletes
            .push_back(DeleteScript { gate, result });
    }

    pub(super) fn script_react(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state().reacts.push_back(DeleteScript { gate, result });
    }

    /// The reaction tasks are done with the server and about to report.
    pub(super) fn wait_for_reacts_returned(&self, count: usize) {
        wait_until(|| (self.state().reacts_returned >= count).then_some(()));
        std::thread::sleep(Duration::from_millis(20));
    }

    /// The edit tasks are done with the server and about to report.
    pub(super) fn wait_for_edits_returned(&self, count: usize) {
        wait_until(|| (self.state().edits_returned >= count).then_some(()));
        // As for the sends: what the test does next must not overtake the report.
        std::thread::sleep(Duration::from_millis(20));
    }

    /// The deletion tasks are done with the server and about to report.
    pub(super) fn wait_for_deletes_returned(&self, count: usize) {
        wait_until(|| (self.state().deletes_returned >= count).then_some(()));
        std::thread::sleep(Duration::from_millis(20));
    }

    pub(super) fn script_room_action(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<(), ApiError>,
    ) {
        self.state()
            .room_actions
            .push_back(DeleteScript { gate, result });
    }

    /// The room command tasks are done with the server and about to report.
    pub(super) fn wait_for_room_actions_returned(&self, count: usize) {
        wait_until(|| (self.state().room_actions_returned >= count).then_some(()));
        std::thread::sleep(Duration::from_millis(20));
    }

    /// Records a room command and plays its script.
    pub(super) async fn room_action(&self, call: RoomCall) -> Result<(), ApiError> {
        let script = {
            let mut state = self.state();
            state.room_calls.push(call);
            state.room_actions.pop_front()
        };
        let (gate, result) = script.map_or((None, Ok(())), |s| (s.gate, s.result));
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().room_actions_returned += 1;
        result
    }

    pub(super) fn script_directory(
        &self,
        people: Result<Found<Person>, ApiError>,
        channels: Result<Found<DirectoryChannel>, ApiError>,
    ) {
        self.script_directory_gated(None, people, channels);
    }

    /// Like [`script_directory`](Self::script_directory); the request for the users waits for the gate.
    pub(super) fn script_directory_gated(
        &self,
        gate: Option<Arc<Notify>>,
        people: Result<Found<Person>, ApiError>,
        channels: Result<Found<DirectoryChannel>, ApiError>,
    ) {
        let mut state = self.state();
        state.directory_people.push_back(Scripted {
            gate,
            result: people,
        });
        state.directory_channels.push_back(Scripted {
            gate: None,
            result: channels,
        });
    }

    /// The directory requests have started: `count` of them, users and channels together.
    pub(super) fn wait_for_directory_calls(&self, count: usize) -> Vec<DirectoryCall> {
        wait_until(|| {
            let calls = self.state().directory_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_members(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<Found<Member>, ApiError>,
    ) {
        self.state().members.push_back(Scripted { gate, result });
    }

    /// The member requests have started: `count` of them.
    pub(super) fn wait_for_member_calls(&self, count: usize) -> Vec<MemberCall> {
        wait_until(|| {
            let calls = self.state().member_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_direct(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<RoomId, ApiError>,
    ) {
        self.state().directs.push_back(Scripted { gate, result });
    }

    pub(super) fn script_join(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state().joins.push_back(Scripted { gate, result });
    }

    pub(super) fn script_info(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<rc_api::RoomInfo, ApiError>,
    ) {
        self.state().infos.push_back(Scripted { gate, result });
    }

    /// Makes `subscription` answer for the room from now on.
    pub(super) fn script_own_subscription(
        &self,
        room: &str,
        result: Result<Option<rc_api::Subscription>, ApiError>,
    ) {
        self.state()
            .own_subscriptions
            .insert(room.to_owned(), result);
    }

    /// Records a call and plays the next script of the queue `pick` takes it from.
    async fn scripted<T>(
        &self,
        pick: impl FnOnce(&mut FakeState) -> Option<Scripted<T>>,
        default: Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let script = pick(&mut self.state());
        let Some(script) = script else {
            return default;
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    pub(super) fn script_mark_read(&self, result: Result<(), ApiError>) {
        self.state().mark_reads.push_back(result);
    }

    pub(super) fn wait_for_mark_read_calls(&self, count: usize) -> Vec<String> {
        wait_until(|| {
            let calls = self.state().mark_read_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    pub(super) fn script_realtime(
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
    pub(super) fn wait_for_connections(&self, count: usize) {
        wait_until(|| (self.state().connections.len() >= count).then_some(()));
    }

    /// Pushes an event on connection `index`. False if the client dropped its end.
    pub(super) fn push(&self, index: usize, event: Pushed) -> bool {
        self.wait_for_connections(index + 1);
        let state = self.state();
        state.connections[index]
            .as_ref()
            .is_some_and(|server| server.send(event).is_ok())
    }

    /// The server ends connection `index`.
    pub(super) fn end_connection(&self, index: usize) {
        self.wait_for_connections(index + 1);
        self.state().connections[index] = None;
    }

    /// The client has dropped its end of connection `index`, which is what closes it.
    pub(super) fn wait_for_connection_closed(&self, index: usize) {
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
    pub(super) fn wait_for_history_calls(&self, count: usize) -> Vec<HistoryCall> {
        wait_until(|| {
            let calls = self.state().history_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The send task has started and is inside `send_message`.
    pub(super) fn wait_for_send_calls(&self, count: usize) -> Vec<SendCall> {
        wait_until(|| {
            let calls = self.state().send_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The send tasks are done with the server and about to report.
    pub(super) fn wait_for_sends_returned(&self, count: usize) {
        wait_until(|| (self.state().sends_returned >= count).then_some(()));
        // A task counted may not have queued its report yet, and what the test sends next must
        // not overtake it.
        std::thread::sleep(Duration::from_millis(20));
    }

    /// The fetch task has started and is inside `subscriptions`.
    pub(super) fn wait_for_subscriptions_calls(&self, count: usize) {
        wait_until(|| (self.state().subscriptions_calls.len() >= count).then_some(()));
    }

    pub(super) fn login_calls(&self) -> Vec<(String, String)> {
        self.state().login_calls.clone()
    }

    pub(super) fn wait_for_logout_call(&self) -> (String, String) {
        wait_until(|| self.state().logout_calls.first().cloned())
    }

    pub(super) fn wait_for_me_call(&self) {
        wait_until(|| (!self.state().me_calls.is_empty()).then_some(()));
    }

    /// The login task has started and is inside `login`.
    pub(super) fn wait_for_login_calls(&self, count: usize) {
        wait_until(|| (self.state().login_calls.len() >= count).then_some(()));
    }
}

impl Connector for Fake {
    type Api = Self;

    fn connect(&self, server: &str) -> Result<Self, ApiError> {
        let fake = self.state().servers.get(server).unwrap_or(self).clone();
        let error = fake.state().connect_error.clone();
        error.map_or(Ok(fake), Err)
    }
}

impl ServerApi for Fake {
    fn version(&self) -> impl Future<Output = Result<ServerVersion, ApiError>> + Send {
        std::future::ready(self.state().version.clone().unwrap())
    }

    fn site_url(&self) -> impl Future<Output = Result<Option<String>, ApiError>> + Send {
        std::future::ready(self.state().site_url.clone().unwrap_or(Ok(None)))
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
        let user_id =
            (self.state().user_ids.get(username).cloned()).unwrap_or_else(|| "uid".into());
        Ok((
            Credentials {
                user_id: user_id.clone().into(),
                token: SecretString::from("tok"),
            },
            User {
                id: user_id.into(),
                ..user(username)
            },
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

    async fn emoji_image(
        &self,
        _credentials: &Credentials,
        name: &str,
        extension: &str,
    ) -> Result<Image, ApiError> {
        struct Running(Fake);
        impl Drop for Running {
            fn drop(&mut self) {
                self.0.state().emoji_image_running -= 1;
            }
        }
        let file = format!("{name}.{extension}");
        let script = {
            let mut state = self.state();
            state.emoji_image_calls.push(file.clone());
            state.emoji_image_running += 1;
            state.emoji_image_max_running =
                state.emoji_image_max_running.max(state.emoji_image_running);
            state
                .emoji_images
                .get_mut(&file)
                .and_then(VecDeque::pop_front)
        };
        let _running = Running(self.clone());
        let Some(script) = script else {
            return Err(ApiError::Server {
                status: 404,
                message: "no such emoji".to_owned(),
            });
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

    async fn thread_messages(
        &self,
        _credentials: &Credentials,
        thread: &MessageId,
        offset: usize,
        count: usize,
    ) -> Result<Page, ApiError> {
        let script = {
            let mut state = self.state();
            state.thread_calls.push(ThreadCall {
                thread: thread.to_string(),
                offset,
                count,
            });
            state.threads.pop_front()
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

    async fn threads_list(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        filter: ThreadFilter,
        offset: usize,
        count: usize,
    ) -> Result<ThreadsPage, ApiError> {
        let script = {
            let mut state = self.state();
            state.thread_list_calls.push(ThreadsListCall {
                room: room.to_string(),
                filter,
                offset,
                count,
            });
            state.thread_lists.pop_front()
        };
        let Some(script) = script else {
            return Ok(ThreadsPage {
                threads: Vec::new(),
                reached_end: true,
            });
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    async fn message(
        &self,
        _credentials: &Credentials,
        id: &MessageId,
    ) -> Result<Message, ApiError> {
        let script = {
            let mut state = self.state();
            state.message_calls.push(id.to_string());
            state.messages.pop_front()
        };
        let Some(script) = script else {
            return Err(ApiError::Server {
                status: 404,
                message: "Not Found".to_owned(),
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
        thread: Option<&ThreadTarget>,
    ) -> Result<Message, ApiError> {
        let script = {
            let mut state = self.state();
            state.send_calls.push(SendCall {
                room: room.to_string(),
                id: id.to_string(),
                text: text.to_owned(),
                thread: thread.cloned(),
            });
            state.sends.pop_front()
        };
        let (gate, result) = script.map_or((None, Ok(1_000)), |s| (s.gate, s.result));
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().sends_returned += 1;
        let stored = server_copy(room.as_str(), id.as_str(), text, result?);
        Ok(Message {
            thread_id: thread.map(|target| target.parent.clone()),
            thread_shown: thread.is_some_and(|target| target.also_to_channel),
            ..stored
        })
    }

    async fn upload_file(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        path: &Path,
        name: &str,
        progress: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<String, ApiError> {
        let script = {
            let mut state = self.state();
            state.upload_calls.push(UploadCall {
                room: room.to_string(),
                path: path.to_path_buf(),
                name: name.to_owned(),
            });
            let number = state.upload_calls.len();
            state.uploads.pop_front().unwrap_or(UploadScript {
                gate: None,
                progress: Vec::new(),
                result: Ok(format!("file{number}")),
            })
        };
        for sent in script.progress {
            progress(sent);
        }
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        self.state().uploads_returned += 1;
        script.result
    }

    async fn confirm_upload(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        file: &str,
        text: &str,
        thread: Option<&MessageId>,
    ) -> Result<Message, ApiError> {
        let script = {
            let mut state = self.state();
            state.confirm_calls.push(ConfirmCall {
                room: room.to_string(),
                file: file.to_owned(),
                text: text.to_owned(),
                thread: thread.map(ToString::to_string),
            });
            state.confirms.pop_front()
        };
        let (gate, result) = script.map_or((None, Ok(1_000)), |s| (s.gate, s.result));
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().confirms_returned += 1;
        Ok(uploaded_message(room.as_str(), file, text, result?, thread))
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

    async fn react(
        &self,
        _credentials: &Credentials,
        id: &MessageId,
        emoji: &str,
        react: bool,
    ) -> Result<(), ApiError> {
        let script = {
            let mut state = self.state();
            state.react_calls.push(ReactCall {
                id: id.to_string(),
                emoji: emoji.to_owned(),
                react,
            });
            state.reacts.pop_front()
        };
        let (gate, result) = script.map_or((None, Ok(())), |s| (s.gate, s.result));
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().reacts_returned += 1;
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

    async fn mark_unread(&self, _credentials: &Credentials, room: &RoomId) -> Result<(), ApiError> {
        let room = room.to_string();
        self.room_action(RoomCall::MarkUnread { room }).await
    }

    async fn set_favorite(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        favorite: bool,
    ) -> Result<(), ApiError> {
        let room = room.to_string();
        self.room_action(RoomCall::SetFavorite { room, favorite })
            .await
    }

    async fn leave_room(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        kind: RoomKind,
    ) -> Result<(), ApiError> {
        let room = room.to_string();
        self.room_action(RoomCall::Leave { room, kind }).await
    }

    async fn directory_users(
        &self,
        _credentials: &Credentials,
        text: &str,
        count: usize,
    ) -> Result<Found<Person>, ApiError> {
        let call = DirectoryCall {
            kind: "users",
            text: text.to_owned(),
            count,
        };
        let empty = Found {
            items: Vec::new(),
            total: 0,
        };
        self.scripted(
            |state| {
                state.directory_calls.push(call);
                state.directory_people.pop_front()
            },
            Ok(empty),
        )
        .await
    }

    async fn directory_channels(
        &self,
        _credentials: &Credentials,
        text: &str,
        count: usize,
    ) -> Result<Found<DirectoryChannel>, ApiError> {
        let call = DirectoryCall {
            kind: "channels",
            text: text.to_owned(),
            count,
        };
        let empty = Found {
            items: Vec::new(),
            total: 0,
        };
        self.scripted(
            |state| {
                state.directory_calls.push(call);
                state.directory_channels.pop_front()
            },
            Ok(empty),
        )
        .await
    }

    async fn room_members(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        filter: &str,
        count: usize,
    ) -> Result<Found<Member>, ApiError> {
        let call = MemberCall {
            room: room.to_string(),
            filter: filter.to_owned(),
            count,
        };
        let empty = Found {
            items: Vec::new(),
            total: 0,
        };
        self.scripted(
            |state| {
                state.member_calls.push(call);
                state.members.pop_front()
            },
            Ok(empty),
        )
        .await
    }

    async fn create_direct(
        &self,
        _credentials: &Credentials,
        username: &str,
    ) -> Result<RoomId, ApiError> {
        let username = username.to_owned();
        self.scripted(
            |state| {
                state.direct_calls.push(username);
                state.directs.pop_front()
            },
            Err(refused()),
        )
        .await
    }

    async fn join_channel(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
    ) -> Result<(), ApiError> {
        let room = room.to_string();
        self.scripted(
            |state| {
                state.join_calls.push(room);
                state.joins.pop_front()
            },
            Ok(()),
        )
        .await
    }

    async fn room_info(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
    ) -> Result<rc_api::RoomInfo, ApiError> {
        let room = room.to_string();
        self.scripted(
            |state| {
                state.info_calls.push(room);
                state.infos.pop_front()
            },
            Err(refused()),
        )
        .await
    }

    fn subscription(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
    ) -> impl Future<Output = Result<Option<rc_api::Subscription>, ApiError>> + Send {
        let mut state = self.state();
        state.subscription_calls.push(room.to_string());
        let answer = (state.own_subscriptions.get(room.as_str()))
            .cloned()
            .unwrap_or(Ok(None));
        std::future::ready(answer)
    }

    async fn realtime(&self, credentials: &Credentials) -> Result<Realtime, ApiError> {
        let (server, client) = unbounded_channel();
        let (watch, watched) = unbounded_channel();
        let (read_threads, reads) = unbounded_channel();
        let realtime = Realtime {
            events: client,
            watch,
            read_threads,
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
            state.reads.push(reads);
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

/// A clock that tells the time the test sets, and stands still otherwise. Clones share the time.
/// A sleep ends when the test moves the time to its end.
#[derive(Clone)]
pub(super) struct FakeClock(Arc<tokio::sync::watch::Sender<jiff::Timestamp>>);

impl FakeClock {
    /// The time a harness starts at: five days after the epoch, which is when the rooms and
    /// messages of the tests date from. That is long enough for a message to be too old to change,
    /// and short enough for no direct message to be quiet unless the test says so.
    pub(super) fn start() -> jiff::Timestamp {
        jiff::Timestamp::UNIX_EPOCH + Duration::from_hours(120)
    }

    pub(super) fn set(&self, time: jiff::Timestamp) {
        self.0.send_replace(time);
    }

    pub(super) fn advance(&self, by: Duration) {
        self.0
            .send_modify(|time| *time = time.checked_add(by).expect("in range"));
    }
}

impl Default for FakeClock {
    /// At [`start`](Self::start).
    fn default() -> Self {
        Self(Arc::new(tokio::sync::watch::channel(Self::start()).0))
    }
}

impl Clock for FakeClock {
    fn now(&self) -> jiff::Timestamp {
        *self.0.borrow()
    }

    fn sleep_until(&self, at: jiff::Timestamp) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let mut time = self.0.subscribe();
        Box::pin(async move {
            // Fails only if the clock is gone, and then nothing is waiting any more.
            let _ = time.wait_for(|now| *now >= at).await;
        })
    }
}

/// Files in memory, at made-up paths: `/cache/<server>/<user or room>/<name>.<extension>`.
#[derive(Clone, Default)]
pub(super) struct FakeCache(pub(super) Arc<Mutex<FakeFiles>>);

/// The extension and the bytes of each file.
pub(super) type FakeFiles = HashMap<CacheKey, (&'static str, Vec<u8>)>;

impl FakeCache {
    /// A file held before the session starts. `server` is the key's, `h` for `http://h`.
    pub(super) fn seed(
        &self,
        server: &str,
        owner: &AvatarOwner,
        extension: &'static str,
        bytes: &[u8],
    ) {
        let key = CacheKey::avatar(server, owner);
        self.0
            .lock()
            .unwrap()
            .insert(key, (extension, bytes.to_vec()));
    }

    pub(super) fn bytes(&self, server: &str, owner: &AvatarOwner) -> Option<Vec<u8>> {
        let key = CacheKey::avatar(server, owner);
        self.0
            .lock()
            .unwrap()
            .get(&key)
            .map(|(_, bytes)| bytes.clone())
    }

    /// A file held before the session starts, as an attachment image.
    pub(super) fn seed_attachment(
        &self,
        server: &str,
        path: &str,
        extension: &'static str,
        bytes: &[u8],
    ) {
        let key = CacheKey::attachment(server, path);
        self.0
            .lock()
            .unwrap()
            .insert(key, (extension, bytes.to_vec()));
    }

    pub(super) fn attachment_bytes(&self, server: &str, path: &str) -> Option<Vec<u8>> {
        let key = CacheKey::attachment(server, path);
        self.0
            .lock()
            .unwrap()
            .get(&key)
            .map(|(_, bytes)| bytes.clone())
    }

    /// A file held before the session starts, as the image of a custom emoji.
    pub(super) fn seed_emoji(
        &self,
        server: &str,
        name: &str,
        list_extension: &str,
        extension: &'static str,
        bytes: &[u8],
    ) {
        let key = CacheKey::custom_emoji(server, name, list_extension);
        self.0
            .lock()
            .unwrap()
            .insert(key, (extension, bytes.to_vec()));
    }

    pub(super) fn emoji_bytes(
        &self,
        server: &str,
        name: &str,
        list_extension: &str,
    ) -> Option<Vec<u8>> {
        let key = CacheKey::custom_emoji(server, name, list_extension);
        self.0
            .lock()
            .unwrap()
            .get(&key)
            .map(|(_, bytes)| bytes.clone())
    }

    pub(super) fn path(key: &CacheKey, extension: &str) -> PathBuf {
        let kind = match key.kind {
            crate::ports::CacheKind::UserAvatar => "user",
            crate::ports::CacheKind::RoomAvatar => "room",
            crate::ports::CacheKind::Attachment => "attachment",
            crate::ports::CacheKind::CustomEmoji => "emoji",
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
pub(super) struct FakeStore {
    pub(super) inner: MemoryStore,
    pub(super) fail_load: bool,
    pub(super) fail_save: bool,
}

impl SecretStore for FakeStore {
    fn load(&self) -> Result<Vec<StoredAccount>, StoreError> {
        if self.fail_load {
            return Err(StoreError::Unavailable("locked".into()));
        }
        self.inner.load()
    }

    fn save(&self, account: &StoredAccount) -> Result<(), StoreError> {
        if self.fail_save {
            return Err(StoreError::Unavailable("locked".into()));
        }
        self.inner.save(account)
    }

    fn remove(&self, id: &AccountId) -> Result<(), StoreError> {
        self.inner.remove(id)
    }
}
