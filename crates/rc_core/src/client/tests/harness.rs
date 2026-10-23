// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The test client, and the builders and waits the tests share.

use std::cell::{Cell, RefCell};
use std::sync::mpsc::RecvTimeoutError;

use super::*;

pub(super) const TIMEOUT: Duration = Duration::from_secs(5);

/// The wait before a reconnect for the tests that don't reconnect: longer than they run.
pub(super) const NEVER: Duration = Duration::from_secs(3600);

/// Polls `check` until it returns a value.
pub(super) fn wait_until<T>(check: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn user(username: &str) -> User {
    User {
        id: "uid".to_owned().into(),
        username: username.to_owned(),
        display_name: Some("Display".to_owned()),
    }
}

pub(super) fn subscription(
    id: &str,
    name: &str,
    room_type: rc_api::RoomType,
) -> rc_api::Subscription {
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
        favorite: false,
        thread_unread: Vec::new(),
        thread_unread_user: Vec::new(),
        thread_unread_group: Vec::new(),
        created: None,
        last_message: None,
        avatar_etag: None,
        topic: String::new(),
    }
}

pub(super) fn room(id: &str, name: &str, kind: RoomKind) -> Room {
    Room {
        id: id.to_owned().into(),
        kind,
        name: name.to_owned(),
        // A direct message is called after the other user.
        peer: (kind == RoomKind::Direct).then(|| name.to_owned()),
        avatar_etag: None,
        topic: String::new(),
        unread: 0,
        user_mentions: 0,
        group_mentions: 0,
        alert: false,
        favorite: false,
        unread_threads: Vec::new(),
        mentioned_threads: Vec::new(),
        last_activity: jiff::Timestamp::UNIX_EPOCH,
    }
}

pub(super) fn session_event(state: SessionState) -> Event {
    Event::Session(state)
}

pub(super) fn rooms_event(rooms: Vec<Room>) -> Event {
    Event::Rooms(vec![RoomChange::Reset(rooms)])
}

pub(super) fn logged_out(failure: Option<LoginFailure>) -> SessionState {
    SessionState::LoggedOut { failure }
}

pub(super) fn logged_in(server: &str, username: &str) -> SessionState {
    SessionState::LoggedIn(Account {
        server: server.to_owned(),
        user: user(username),
    })
}

/// A signed-in account stored under `id`.
pub(super) fn stored_as(id: &str, server: &str) -> StoredAccount {
    StoredAccount {
        id: id.to_owned().into(),
        server: server.to_owned(),
        username: "alice".to_owned(),
        user_id: "uid".to_owned().into(),
        token: Some(SecretString::from("stored-tok")),
    }
}

pub(super) fn stored(server: &str) -> StoredAccount {
    stored_as(STORED_ID, server)
}

/// The id of the account `stored` makes.
pub(super) const STORED_ID: &str = "stored-id";

/// A store that already holds an account.
pub(super) fn store_with(server: &str) -> MemoryStore {
    let store = MemoryStore::default();
    store.save(&stored(server)).unwrap();
    store
}

pub(super) fn wait_for_empty(store: &MemoryStore) {
    wait_until(|| store.load().unwrap().is_empty().then_some(()));
}

/// Waits until the store holds a single account that has lost its token, and returns it: what an
/// expired session leaves.
pub(super) fn wait_for_expired(store: &MemoryStore) -> StoredAccount {
    wait_until(|| {
        let accounts = store.load().unwrap();
        match accounts.as_slice() {
            [account] if account.token.is_none() => Some(account.clone()),
            _ => None,
        }
    })
}

/// A client whose events are read raw, none skipped, as they come.
pub(super) fn raw_client(
    fake: &Fake,
    store: impl SecretStore,
    cache: FakeCache,
    clock: FakeClock,
    backoff: Backoff,
) -> (Client, Receiver<(AccountId, Event)>) {
    let (tx, events) = mpsc::channel();
    let client = Client::with_tuning(
        fake.clone(),
        store,
        cache,
        clock,
        move |id, event| {
            let _ = tx.send((id, event));
        },
        Tuning {
            backoff,
            ..Tuning::default()
        },
    )
    .unwrap();
    (client, events)
}

/// The next `count` events, whichever account they belong to.
pub(super) fn raw_events(events: &Receiver<(AccountId, Event)>, count: usize) -> Vec<Event> {
    (0..count)
        .map(|_| events.recv_timeout(TIMEOUT).expect("no event").1)
        .collect()
}

pub(super) struct Harness {
    pub(super) client: Client,
    /// The time the accounts see. Starts at [`FakeClock::start`].
    pub(super) clock: FakeClock,
    events: Receiver<(AccountId, Event)>,
    /// The account the harness follows: the first one added.
    account: AccountId,
    /// Events read while looking for another account's, kept for when it is asked for.
    held: RefCell<HashMap<AccountId, VecDeque<Event>>>,
    /// The test uses more than one account: events of the others are held, not a failure.
    several: Cell<bool>,
    /// The first session state of the account, which every account has.
    pub(super) first: SessionState,
}

impl Harness {
    pub(super) fn new(fake: &Fake) -> Self {
        Self::with_store(fake, MemoryStore::default())
    }

    pub(super) fn with_store(fake: &Fake, store: impl SecretStore) -> Self {
        Self::with_backoff(fake, store, Backoff::new(NEVER, NEVER, NEVER))
    }

    pub(super) fn with_backoff(fake: &Fake, store: impl SecretStore, backoff: Backoff) -> Self {
        Self::build(fake, store, FakeCache::default(), backoff)
    }

    pub(super) fn with_cache(fake: &Fake, cache: FakeCache) -> Self {
        Self::with_cache_and_backoff(fake, cache, Backoff::new(NEVER, NEVER, NEVER))
    }

    pub(super) fn with_cache_and_backoff(fake: &Fake, cache: FakeCache, backoff: Backoff) -> Self {
        Self::build(fake, MemoryStore::default(), cache, backoff)
    }

    /// A harness for a store that holds several accounts: the events of the others are held
    /// (see [`allow_other_accounts`](Self::allow_other_accounts)) from the start.
    pub(super) fn with_accounts(fake: &Fake, store: impl SecretStore) -> Self {
        Self::build_for(
            fake,
            store,
            FakeCache::default(),
            Backoff::new(NEVER, NEVER, NEVER),
            true,
        )
    }

    pub(super) fn build(
        fake: &Fake,
        store: impl SecretStore,
        cache: FakeCache,
        backoff: Backoff,
    ) -> Self {
        Self::build_for(fake, store, cache, backoff, false)
    }

    fn build_for(
        fake: &Fake,
        store: impl SecretStore,
        cache: FakeCache,
        backoff: Backoff,
        several: bool,
    ) -> Self {
        let clock = FakeClock::default();
        let (client, events) = raw_client(fake, store, cache, clock.clone(), backoff);
        let (account, added) = events.recv_timeout(TIMEOUT).expect("no event");
        assert_eq!(added, Event::Added);
        let mut harness = Self {
            client,
            clock,
            events,
            account,
            held: RefCell::default(),
            several: Cell::new(several),
            first: SessionState::Connecting,
        };
        harness.first = harness.next();
        harness
    }

    /// The account the harness follows.
    pub(super) fn id(&self) -> &AccountId {
        &self.account
    }

    /// The commands for the account the harness follows.
    pub(super) fn account(&self) -> AccountHandle<'_> {
        self.client.account(&self.account)
    }

    /// Lets the test use other accounts: their events are held until the test asks for them with
    /// [`recv_of`](Self::recv_of) and the like, instead of failing the test.
    pub(super) fn allow_other_accounts(&self) {
        self.several.set(true);
    }

    /// Tells the account's actor to log out, which leaves the account in place, logged out. Not
    /// [`Client::remove_account`], which ends the account too.
    pub(super) fn logout(&self) {
        self.logout_account(&self.account);
    }

    /// [`logout`](Self::logout) for the account `id`.
    pub(super) fn logout_account(&self, id: &AccountId) {
        self.client.senders.send(id, crate::client::Command::Logout);
    }

    pub(super) fn login(&self, server: &str, username: &str) {
        self.account()
            .login(server, username, SecretString::from("secret"));
    }

    /// The next event of the account, within `timeout`. Every event of another account is a
    /// failure, unless the test allowed them (see
    /// [`allow_other_accounts`](Self::allow_other_accounts)).
    pub(super) fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
        self.recv_of(&self.account, timeout)
    }

    /// The next event of the account `id`, within `timeout`.
    pub(super) fn recv_of(
        &self,
        id: &AccountId,
        timeout: Duration,
    ) -> Result<Event, RecvTimeoutError> {
        if let Some(event) = (self.held.borrow_mut().get_mut(id)).and_then(VecDeque::pop_front) {
            return Ok(event);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let (from, event) = self.events.recv_timeout(left)?;
            if from == *id {
                return Ok(event);
            }
            assert!(
                self.several.get() || from == self.account,
                "event of another account {from}: {event:?}"
            );
            self.held
                .borrow_mut()
                .entry(from)
                .or_default()
                .push_back(event);
        }
    }

    /// The next event, connection changes, identities and unread totals aside: the tests that
    /// don't look at them don't wait for them either. See
    /// [`next_connection`](Self::next_connection), [`events_until`](Self::events_until) (identities)
    /// and [`next_unread_of`](Self::next_unread_of).
    pub(super) fn next_event(&self) -> Event {
        self.next_event_of(&self.account)
    }

    /// [`next_event`](Self::next_event) of the account `id`.
    pub(super) fn next_event_of(&self, id: &AccountId) -> Event {
        loop {
            match self.recv_of(id, TIMEOUT).expect("no event") {
                Event::Connection(_) | Event::Identity(_) | Event::Unread(_) => {}
                event => return event,
            }
        }
    }

    /// The next unread totals of the account `id`, skipping the other events.
    pub(super) fn next_unread_of(&self, id: &AccountId) -> Unread {
        loop {
            if let Event::Unread(unread) = self.recv_of(id, TIMEOUT).expect("no event") {
                return unread;
            }
        }
    }

    /// The next unread totals of the account, skipping the other events.
    pub(super) fn next_unread(&self) -> Unread {
        self.next_unread_of(&self.account)
    }

    /// The updates of the next `Avatars` event, skipping the other events.
    pub(super) fn next_avatars(&self) -> Vec<AvatarUpdate> {
        loop {
            if let Event::Avatars(updates) = self.recv_timeout(TIMEOUT).expect("no event") {
                return updates;
            }
        }
    }

    /// Checks that no `Avatars` event comes for a little while, and drops the other events.
    pub(super) fn assert_no_avatars(&self) {
        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(Event::Avatars(updates)) = self.recv_timeout(left) {
                panic!("unexpected avatars: {updates:?}");
            }
        }
    }

    /// The updates of the next `Images` event, skipping the other events.
    pub(super) fn next_images(&self) -> Vec<ImageUpdate> {
        loop {
            if let Event::Images(updates) = self.recv_timeout(TIMEOUT).expect("no event") {
                return updates;
            }
        }
    }

    /// Checks that no `Images` event comes for a little while, and drops the other events.
    pub(super) fn assert_no_images(&self) {
        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(Event::Images(updates)) = self.recv_timeout(left) {
                panic!("unexpected images: {updates:?}");
            }
        }
    }

    /// Skips events until the open room's messages gain the one with this id.
    pub(super) fn wait_for_appended(&self, id: &str) {
        loop {
            if let Event::Timeline(changes) = self.recv_timeout(TIMEOUT).expect("no event")
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
    pub(super) fn next_connection(&self) -> Connection {
        loop {
            if let Event::Connection(connection) = self.recv_timeout(TIMEOUT).expect("no event") {
                return connection;
            }
        }
    }

    /// The next session state. Room events in between are skipped: the session tests don't care
    /// about them, the room tests use [`next_event`](Self::next_event).
    pub(super) fn next(&self) -> SessionState {
        self.next_of(&self.account)
    }

    /// [`next`](Self::next) of the account `id`.
    pub(super) fn next_of(&self, id: &AccountId) -> SessionState {
        loop {
            if let Event::Session(state) = self.next_event_of(id) {
                return state;
            }
        }
    }

    /// Checks the next session states, in order.
    pub(super) fn expect(&self, states: &[SessionState]) {
        self.expect_of(&self.account, states);
    }

    /// [`expect`](Self::expect) for the account `id`.
    pub(super) fn expect_of(&self, id: &AccountId, states: &[SessionState]) {
        for expected in states {
            assert_eq!(&self.next_of(id), expected);
        }
    }

    /// Checks the next events, in order, room events included.
    pub(super) fn expect_events(&self, events: Vec<Event>) {
        self.expect_events_of(&self.account, events);
    }

    /// [`expect_events`](Self::expect_events) for the account `id`.
    pub(super) fn expect_events_of(&self, id: &AccountId, events: Vec<Event>) {
        for expected in events {
            assert_eq!(self.next_event_of(id), expected);
        }
    }

    /// The raw events of the account `id`, none skipped, up to and including the first that
    /// satisfies `last`.
    pub(super) fn events_until_of(
        &self,
        id: &AccountId,
        last: impl Fn(&Event) -> bool,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            let event = self.recv_of(id, TIMEOUT).expect("no event");
            let done = last(&event);
            events.push(event);
            if done {
                return events;
            }
        }
    }

    /// [`events_until_of`](Self::events_until_of) for the account the harness follows.
    pub(super) fn events_until(&self, last: impl Fn(&Event) -> bool) -> Vec<Event> {
        self.events_until_of(&self.account, last)
    }

    /// The account whose `Added` comes next in the stream, which the test can't know yet because
    /// the core made it up. The events read on the way are held.
    pub(super) fn next_added(&self) -> AccountId {
        loop {
            let (id, event) = self.events.recv_timeout(TIMEOUT).expect("no event");
            if event == Event::Added {
                return id;
            }
            self.held
                .borrow_mut()
                .entry(id)
                .or_default()
                .push_back(event);
        }
    }
}

pub(super) fn message(id: &str, secs: i64) -> Message {
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
        quotes: Vec::new(),
        edited: false,
        reactions: Vec::new(),
        thread_id: None,
        thread_shown: false,
        thread_replies: 0,
        thread_last_reply: None,
        thread_followers: Vec::new(),
    }
}

/// The message as the server stores it after `alice` sent it.
pub(super) fn server_copy(room: &str, id: &str, text: &str, secs: i64) -> Message {
    Message {
        room_id: room.to_owned().into(),
        kind: MessageKind::Text(text.into()),
        ..message(id, secs)
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn page(messages: Vec<Message>, reached_start: bool) -> Result<Page, ApiError> {
    Ok(Page {
        messages,
        reached_start,
    })
}

pub(super) fn reset_event(room: Option<&str>, messages: Vec<Message>) -> Event {
    Event::Timeline(vec![TimelineChange::Reset {
        room: room.map(|id| id.to_owned().into()),
        messages,
    }])
}

pub(super) fn status(
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
pub(super) fn closed_events() -> Vec<Event> {
    vec![
        reset_event(None, Vec::new()),
        Event::TimelineStatus(TimelineStatus::default()),
    ]
}

/// A harness logged in with the rooms `1` (a channel) and `2` (a private group), past their events.
pub(super) fn logged_in_harness(fake: &Fake) -> Harness {
    logged_in_harness_with(fake, Backoff::new(NEVER, NEVER, NEVER))
}

pub(super) fn logged_in_harness_with(fake: &Fake, backoff: Backoff) -> Harness {
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
pub(super) fn open_first_room(fake: &Fake, harness: &Harness, reached_start: bool) {
    fake.script_history(
        None,
        page(vec![message("b", 2), message("a", 1)], reached_start),
    );
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", false, false, reached_start, None),
    ]);
}

/// A message of room `room`, pushed by the server.
pub(super) fn pushed_message(room: &str, id: &str, secs: i64) -> Pushed {
    let mut message = message(id, secs);
    message.room_id = room.to_owned().into();
    Pushed::Message(message)
}

pub(super) fn pushed_subscription(id: &str, name: &str, room_type: rc_api::RoomType) -> Pushed {
    Pushed::Subscription(subscription(id, name, room_type))
}

pub(super) fn removed(id: &str) -> Pushed {
    Pushed::SubscriptionRemoved(id.to_owned().into())
}

pub(super) fn rooms_change(change: RoomChange) -> Event {
    Event::Rooms(vec![change])
}

pub(super) fn appended(id: &str, secs: i64) -> Event {
    Event::Timeline(vec![TimelineChange::Appended(vec![message(id, secs)])])
}

/// The waits of the reconnect tests: short, but far apart enough to tell the steps from the noise.
pub(super) const BASE: Duration = Duration::from_millis(100);

pub(super) const CAP: Duration = Duration::from_millis(800);

/// A connection never counts as working for long enough.
pub(super) fn fast() -> Backoff {
    Backoff::new(BASE, CAP, NEVER)
}

/// The subscription of room `1` as the fake lists it, with `unread` unread messages.
pub(super) fn general_with(unread: u32) -> rc_api::Subscription {
    let mut general = subscription("1", "general", rc_api::RoomType::Channel);
    general.unread = unread;
    general
}

pub(super) fn general_room(unread: u32) -> Room {
    let mut general = room("1", "general", RoomKind::Channel);
    general.unread = unread;
    general
}

/// A harness logged in whose room `1` is open with `a` and `b`, and whose connection was lost.
/// The next connection is not scripted yet.
pub(super) fn open_room_then_lose_the_connection(fake: &Fake) -> Harness {
    let harness = logged_in_harness_with(fake, fast());
    open_first_room(fake, &harness, false);
    harness
}

pub(super) fn updated(index: usize, message: Message) -> Event {
    Event::Timeline(vec![TimelineChange::Updated { index, message }])
}

/// The same message in another delivery state.
pub(super) fn with_delivery(message: &Message, delivery: Delivery) -> Message {
    Message {
        delivery,
        ..message.clone()
    }
}

/// The pending message that sending put at the end of the open room, which is the next event.
pub(super) fn expect_pending(harness: &Harness) -> Message {
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

/// Opens room 2, which has nothing, and takes its events.
pub(super) fn open_the_other_room(harness: &Harness) {
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
}

/// Makes sure the actor has taken in everything pushed so far, for events that have no output:
/// pushes are taken in order, so a message of the open room that comes out is the proof.
pub(super) fn flush_pushed(fake: &Fake, harness: &Harness, room: &str) {
    assert!(fake.push(0, pushed_message(room, "flush", 999)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        Message {
            room_id: room.to_owned().into(),
            ..message("flush", 999)
        },
    ])])]);
}

/// A harness logged in with room `1` (`general`) holding unread messages and a mention, and room
/// `2` (`secret`) holding some too.
pub(super) fn logged_in_with_unread(fake: &Fake) -> Harness {
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

pub(super) fn unread_general() -> rc_api::Subscription {
    let mut general = subscription("1", "general", rc_api::RoomType::Channel);
    general.unread = 3;
    general.user_mentions = 1;
    general.alert = true;
    general
}

pub(super) fn unread_secret() -> rc_api::Subscription {
    let mut secret = subscription("2", "secret", rc_api::RoomType::Private);
    secret.unread = 2;
    secret.alert = true;
    secret
}

/// The next event is the room list changing, once, to this.
pub(super) fn expect_rooms_updated(harness: &Harness, index: usize, room: Room) {
    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index,
        item: room,
    })]);
}

pub(super) fn at(secs: i64) -> jiff::Timestamp {
    jiff::Timestamp::from_second(secs).unwrap()
}

/// A room's subscription as the server lists it when the room has an avatar of its own.
pub(super) fn avatar_subscription(
    id: &str,
    name: &str,
    room_type: rc_api::RoomType,
) -> rc_api::Subscription {
    rc_api::Subscription {
        avatar_etag: Some("etag".to_owned()),
        ..subscription(id, name, room_type)
    }
}

pub(super) fn avatar_room(id: &str, name: &str, kind: RoomKind) -> Room {
    Room {
        avatar_etag: Some("etag".to_owned()),
        ..room(id, name, kind)
    }
}

/// A harness with a cache, logged in with the rooms `1` and `2`, past their events.
pub(super) fn logged_in_with_cache(fake: &Fake, cache: &FakeCache) -> Harness {
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

/// Opens room 1 with an empty page, which wants no avatar.
pub(super) fn open_first_room_of(fake: &Fake, harness: &Harness) {
    fake.script_history(None, page(Vec::new(), true));
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
}

pub(super) fn deleted(room: &str, id: &str) -> Pushed {
    Pushed::MessageDeleted {
        room: room.to_owned().into(),
        id: id.to_owned().into(),
    }
}

pub(super) fn removal(index: usize) -> Event {
    Event::Timeline(vec![TimelineChange::Removed { index }])
}

pub(super) fn action_event(
    id: &str,
    action: MessageAction,
    outcome: Result<(), ActionError>,
) -> Event {
    Event::MessageAction {
        id: id.to_owned().into(),
        action,
        outcome,
    }
}

pub(super) fn refused() -> ApiError {
    ApiError::Server {
        status: 400,
        message: "error-action-not-allowed".to_owned(),
    }
}

/// A server whose `Site_Url` is not the address that the tests log in with.
pub(super) const SITE: &str = "http://site:3000";
