// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The REST and realtime clients against a real Rocket.Chat server, named by
//! `RC_TEST_SERVER_URL`. Run with `--ignored` (see the README).
//!
//! Every test makes its own user and rooms with unique names and only looks at those, so the tests
//! hold on a dirty server, in any order and in parallel. Server settings are never changed.

// Test helpers outside #[test] fns may unwrap: a failure should just fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use rc_api::{
    Credentials, DdpClient, DdpEvent, DdpEvents, Error, Message, RestClient, RoomType,
    ServerVersion, Subscription, User,
};
use rc_test_server::{Admin, PIXEL_PNG, TestUser, server_url, status_without_login, unique};
use secrecy::{ExposeSecret, SecretString};

const POLL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// How long to wait for an event the server should push right away.
const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

/// A fresh user, plus what's needed to create rooms and messages for it as admin.
struct Fixture {
    client: RestClient,
    admin: Admin,
    user: TestUser,
}

impl Fixture {
    fn new() -> Self {
        let admin = Admin::login();
        let user = admin.create_user();
        Self {
            client: RestClient::new(&server_url()).unwrap(),
            admin,
            user,
        }
    }

    /// What the server should report for the user.
    fn expected_user(&self) -> User {
        User {
            id: self.user.id.clone(),
            username: self.user.username.clone(),
            name: Some(self.user.name.clone()),
        }
    }

    /// The new user logs in, as the app would.
    async fn login(&self) -> rc_api::LoginResult {
        self.client
            .login(
                &self.user.username,
                &SecretString::from(self.user.password.clone()),
            )
            .await
            .unwrap()
    }

    fn create_channel(&self) -> (String, String) {
        self.admin.create_channel(&[&self.user.username])
    }

    fn create_group(&self) -> (String, String) {
        self.admin.create_group(&[&self.user.username])
    }

    /// A direct message between admin and the user.
    fn create_dm(&self) -> String {
        self.admin.create_dm(&self.user.username)
    }
}

/// Polls the user's subscriptions until `ready` accepts them, for up to [`POLL_TIMEOUT`]: unread
/// counts can lag behind a message that was just posted.
async fn subscriptions_when(
    session: &rc_api::RestSession,
    ready: impl Fn(&[Subscription]) -> bool,
) -> Vec<Subscription> {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let rooms = session.subscriptions().await.unwrap();
        if ready(&rooms) || Instant::now() >= deadline {
            return rooms;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// A realtime connection that has logged in with `credentials`, as the app does after a REST login.
async fn connect_and_login(credentials: &Credentials) -> (DdpClient, DdpEvents) {
    let (client, events) = DdpClient::connect(&server_url()).await.unwrap();
    client.login(credentials).await.unwrap();
    (client, events)
}

/// The first event `matches` accepts, for up to [`EVENT_TIMEOUT`]. Events about anything else are
/// skipped: the stream also carries the system messages of the room being set up.
async fn event_where(
    events: &mut DdpEvents,
    mut matches: impl FnMut(&DdpEvent) -> bool,
) -> DdpEvent {
    let wait = async {
        while let Some(event) = events.recv().await {
            if matches(&event) {
                return event;
            }
        }
        panic!("the events ended");
    };
    tokio::time::timeout(EVENT_TIMEOUT, wait)
        .await
        .expect("the event never came")
}

/// Posts `texts` to the room as admin, in order, and returns their ids.
fn post_all(fixture: &Fixture, room_id: &str, texts: &[&str]) -> Vec<String> {
    texts
        .iter()
        .map(|text| fixture.admin.post_message(room_id, text).id)
        .collect()
}

/// The ordinary messages of `page`: rooms also hold system messages, such as the creator joining.
fn ordinary(messages: &[Message]) -> Vec<&Message> {
    messages.iter().filter(|m| m.kind.is_none()).collect()
}

fn find<'a>(rooms: &'a [Subscription], rid: &str) -> &'a Subscription {
    rooms
        .iter()
        .find(|r| r.room_id == rid)
        .unwrap_or_else(|| panic!("room {rid} not in the subscriptions"))
}

// AUTH-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn info_reports_a_supported_version() {
    let info = RestClient::new(&server_url())
        .unwrap()
        .info()
        .await
        .unwrap();
    assert!(
        info.version >= ServerVersion { major: 8, minor: 0 },
        "{}",
        info.version
    );
}

// AUTH-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn login_returns_credentials_and_user() {
    let fixture = Fixture::new();
    let result = fixture.login().await;
    assert_eq!(result.credentials.user_id, fixture.user.id);
    assert!(!result.credentials.token.expose_secret().is_empty());
    assert_eq!(result.user, fixture.expected_user());
}

// AUTH-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn wrong_password_is_invalid_credentials() {
    let fixture = Fixture::new();
    let result = fixture
        .client
        .login(
            &fixture.user.username,
            &SecretString::from("not-the-password"),
        )
        .await;
    assert!(
        matches!(result, Err(Error::InvalidCredentials)),
        "{result:?}"
    );
}

// AUTH-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn unknown_user_is_invalid_credentials() {
    let client = RestClient::new(&server_url()).unwrap();
    let result = client
        .login(
            &format!("ct-nobody-{}", unique()),
            &SecretString::from("whatever"),
        )
        .await;
    assert!(
        matches!(result, Err(Error::InvalidCredentials)),
        "{result:?}"
    );
}

// AUTH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn me_returns_the_user() {
    let fixture = Fixture::new();
    let login = fixture.login().await;
    let me = fixture
        .client
        .session(login.credentials)
        .me()
        .await
        .unwrap();
    assert_eq!(me, fixture.expected_user());
}

// AUTH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn stored_credentials_resume_the_session() {
    let fixture = Fixture::new();
    let login = fixture.login().await;
    // What the app does at startup: a new client and only the stored credentials.
    let fresh = RestClient::new(&server_url()).unwrap();
    let me = fresh.session(login.credentials.clone()).me().await.unwrap();
    assert_eq!(me.id, login.credentials.user_id);
}

// ROOM-001, ROOM-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn subscriptions_have_the_rooms_with_names_and_types() {
    let fixture = Fixture::new();
    let (channel_id, channel_name) = fixture.create_channel();
    let (group_id, group_name) = fixture.create_group();
    let dm_id = fixture.create_dm();
    let session = fixture.client.session(fixture.login().await.credentials);

    let rooms = session.subscriptions().await.unwrap();

    let channel = find(&rooms, &channel_id);
    assert_eq!(channel.room_type, RoomType::Channel);
    assert_eq!(channel.name, channel_name);
    let group = find(&rooms, &group_id);
    assert_eq!(group.room_type, RoomType::Private);
    assert_eq!(group.name, group_name);
    let dm = find(&rooms, &dm_id);
    assert_eq!(dm.room_type, RoomType::Direct);
    assert_eq!(dm.name, fixture.admin.username);
    assert_eq!(dm.display_name, fixture.admin.name);
}

// ROOM-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn subscriptions_have_unread_and_mention_counts() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let dm_id = fixture.create_dm();
    fixture
        .admin
        .post_message(&channel_id, &format!("hello @{}", fixture.user.username));
    fixture.admin.post_message(&dm_id, "hi there");
    let session = fixture.client.session(fixture.login().await.credentials);

    let rooms = subscriptions_when(&session, |rooms| {
        let counted = |rid: &str| rooms.iter().any(|r| r.room_id == rid && r.unread >= 1);
        counted(&channel_id) && counted(&dm_id)
    })
    .await;

    let channel = find(&rooms, &channel_id);
    assert!(channel.unread >= 1, "{channel:?}");
    assert_eq!(channel.user_mentions, 1, "{channel:?}");
    assert!(channel.alert, "{channel:?}");
    let dm = find(&rooms, &dm_id);
    assert!(dm.unread >= 1, "{dm:?}");
}

// ROOM-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn subscriptions_have_the_time_of_the_last_message() {
    let fixture = Fixture::new();
    let (quiet_id, _) = fixture.create_channel();
    let (busy_id, _) = fixture.create_channel();
    let posted = fixture.admin.post_message(&busy_id, "latest");
    let session = fixture.client.session(fixture.login().await.credentials);

    let rooms = session.subscriptions().await.unwrap();

    let busy = find(&rooms, &busy_id);
    assert_eq!(busy.last_message, Some(posted.ts.parse().unwrap()));
    assert!(busy.created.is_some(), "{busy:?}");
    assert!(find(&rooms, &quiet_id).created.is_some());
}

// ROOM-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_message_arrives_as_room_activity_with_its_time() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client
        .subscribe_room_activity(&fixture.user.id)
        .await
        .unwrap();

    let posted = fixture.admin.post_message(&room_id, "something new");

    let event = event_where(
        &mut events,
        |e| matches!(e, DdpEvent::RoomActivity { room_id: r, last_message } if *r == room_id && *last_message == posted.ts.parse::<jiff::Timestamp>().unwrap()),
    )
    .await;
    assert!(matches!(event, DdpEvent::RoomActivity { .. }));
}

// ROOM-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn mark_read_zeroes_the_counts_of_the_room() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let dm_id = fixture.create_dm();
    fixture
        .admin
        .post_message(&channel_id, &format!("hello @{}", fixture.user.username));
    fixture.admin.post_message(&dm_id, "hi there");
    let session = fixture.client.session(fixture.login().await.credentials);
    subscriptions_when(&session, |rooms| {
        let counted = |rid: &str| rooms.iter().any(|r| r.room_id == rid && r.unread >= 1);
        counted(&channel_id) && counted(&dm_id)
    })
    .await;

    session.mark_read(&channel_id).await.unwrap();

    let rooms = subscriptions_when(&session, |rooms| find(rooms, &channel_id).unread == 0).await;
    let channel = find(&rooms, &channel_id);
    assert_eq!(channel.unread, 0, "{channel:?}");
    assert_eq!(channel.user_mentions, 0, "{channel:?}");
    assert!(!channel.alert, "{channel:?}");
    assert!(find(&rooms, &dm_id).unread >= 1, "other rooms stay unread");
}

// ROOM-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn mark_read_of_a_room_without_subscription_is_a_server_error() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.admin.create_channel(&[]);
    let session = fixture.client.session(fixture.login().await.credentials);

    let result = session.mark_read(&channel_id).await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );
}

// ROOM-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn mark_read_with_a_revoked_token_is_unauthorized() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    session.logout().await.unwrap();

    let result = session.mark_read(&channel_id).await;

    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ROOM-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_favorite_shows_in_the_subscriptions_until_it_is_taken_back() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let (other_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let rooms = session.subscriptions().await.unwrap();
    assert!(!find(&rooms, &channel_id).favorite, "never favorited");

    session.set_favorite(&channel_id, true).await.unwrap();
    session.set_favorite(&channel_id, true).await.unwrap();
    let rooms = session.subscriptions().await.unwrap();
    assert!(find(&rooms, &channel_id).favorite);
    assert!(!find(&rooms, &other_id).favorite, "other rooms are not");

    session.set_favorite(&channel_id, false).await.unwrap();
    let rooms = session.subscriptions().await.unwrap();
    assert!(!find(&rooms, &channel_id).favorite);
}

// ROOM-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_room_without_subscription_cannot_be_a_favorite() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.admin.create_channel(&[]);
    let session = fixture.client.session(fixture.login().await.credentials);

    let result = session.set_favorite(&channel_id, true).await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message })
            if message.starts_with("You must be part of a room to favorite it")),
        "{result:?}"
    );
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn mark_unread_makes_the_subscription_unread_with_an_alert() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    fixture.admin.post_message(&channel_id, "one");
    fixture.admin.post_message(&channel_id, "two");
    let session = fixture.client.session(fixture.login().await.credentials);
    session.mark_read(&channel_id).await.unwrap();
    let rooms = subscriptions_when(&session, |rooms| !find(rooms, &channel_id).alert).await;
    assert_eq!(find(&rooms, &channel_id).unread, 0);

    session.mark_unread(&channel_id).await.unwrap();

    // Whatever the number of messages, the server counts one unread message, raises the alert and
    // leaves the mentions alone.
    let rooms = subscriptions_when(&session, |rooms| find(rooms, &channel_id).alert).await;
    let channel = find(&rooms, &channel_id);
    assert_eq!(channel.unread, 1, "{channel:?}");
    assert!(channel.alert, "{channel:?}");
    assert_eq!(channel.user_mentions, 0, "{channel:?}");
    assert_eq!(channel.group_mentions, 0, "{channel:?}");

    session.mark_unread(&channel_id).await.unwrap();
    let rooms = session.subscriptions().await.unwrap();
    assert_eq!(find(&rooms, &channel_id).unread, 1, "again changes nothing");
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn mark_unread_of_a_room_without_messages_is_refused_with_a_text() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);

    let result = session.mark_unread(&channel_id).await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message })
            if message == "There are no messages to mark unread [error-no-message-for-unread]"),
        "{result:?}"
    );
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn mark_unread_with_a_revoked_token_is_unauthorized() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    session.logout().await.unwrap();

    let result = session.mark_unread(&channel_id).await;

    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn leaving_a_channel_or_a_group_removes_the_subscription() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let (group_id, _) = fixture.create_group();
    let (kept_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);

    session
        .leave(&channel_id, &RoomType::Channel)
        .await
        .unwrap();
    session.leave(&group_id, &RoomType::Private).await.unwrap();

    let rooms = session.subscriptions().await.unwrap();
    assert!(rooms.iter().all(|r| r.room_id != channel_id));
    assert!(rooms.iter().all(|r| r.room_id != group_id));
    find(&rooms, &kept_id);
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn leaving_a_room_that_was_left_already_is_refused() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    session
        .leave(&channel_id, &RoomType::Channel)
        .await
        .unwrap();

    let result = session.leave(&channel_id, &RoomType::Channel).await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, ref message })
            if message.starts_with("You are not in this room")),
        "{result:?}"
    );
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn the_last_owner_of_a_group_or_a_channel_cannot_leave_it() {
    let fixture = Fixture::new();
    let owner = Admin::login_as(&fixture.user.username, &fixture.user.password);
    let (group_id, _) = owner.create_group(&[]);
    let (channel_id, _) = owner.create_channel(&[]);
    let session = fixture.client.session(fixture.login().await.credentials);

    // The text is shown to the user as the server words it.
    let refusal = "You are the last owner. Please set new owner before leaving the room. \
                   [error-you-are-last-owner]";
    for (room_id, kind) in [
        (&group_id, RoomType::Private),
        (&channel_id, RoomType::Channel),
    ] {
        let result = session.leave(room_id, &kind).await;
        assert!(
            matches!(result, Err(Error::Server { status: 400, ref message }) if message == refusal),
            "{result:?}"
        );
    }
    let rooms = session.subscriptions().await.unwrap();
    find(&rooms, &group_id);
    find(&rooms, &channel_id);
}

// ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn leaving_a_room_of_the_other_type_is_refused() {
    let fixture = Fixture::new();
    let (channel_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);

    let result = session.leave(&channel_id, &RoomType::Private).await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );
    let rooms = session.subscriptions().await.unwrap();
    find(&rooms, &channel_id);
}

// AUTH-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn logout_revokes_the_token() {
    let fixture = Fixture::new();
    let session = fixture.client.session(fixture.login().await.credentials);
    session.me().await.unwrap();

    session.logout().await.unwrap();

    let me = session.me().await;
    assert!(matches!(me, Err(Error::Unauthorized)), "{me:?}");
    let again = session.logout().await;
    assert!(matches!(again, Err(Error::Unauthorized)), "{again:?}");
}

/// Checks the history of one room: what was posted comes back newest first, with its author and
/// time.
async fn check_history(fixture: &Fixture, room_id: &str, room_type: RoomType) {
    let texts = ["first", "second", "third"];
    let ids = post_all(fixture, room_id, &texts);
    let session = fixture.client.session(fixture.login().await.credentials);

    let page = session
        .history(room_id, &room_type, None, 50)
        .await
        .unwrap();

    assert!(page.reached_start, "fewer than 50 messages in a new room");
    let messages = ordinary(&page.messages);
    let got: Vec<_> = messages
        .iter()
        .map(|m| (m.id.as_str(), m.text.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (ids[2].as_str(), "third"),
            (ids[1].as_str(), "second"),
            (ids[0].as_str(), "first")
        ]
    );
    let newest = messages[0];
    assert_eq!(newest.room_id, room_id);
    assert_eq!(newest.author.id, fixture.admin.user_id);
    assert_eq!(newest.author.username, fixture.admin.username);
    assert_eq!(newest.author.name, fixture.admin.name);
    assert!(messages[0].ts >= messages[1].ts && messages[1].ts >= messages[2].ts);
    assert_eq!(newest.thread_id, None);
}

// MSG-001, MSG-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn channel_history_has_the_messages_with_author_and_time() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    check_history(&fixture, &room_id, RoomType::Channel).await;
}

// MSG-001, MSG-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn group_history_has_the_messages_with_author_and_time() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_group();
    check_history(&fixture, &room_id, RoomType::Private).await;
}

// MSG-001, MSG-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn direct_history_has_the_messages_with_author_and_time() {
    let fixture = Fixture::new();
    let room_id = fixture.create_dm();
    check_history(&fixture, &room_id, RoomType::Direct).await;
}

// MSG-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn history_pages_back_with_latest_until_the_start() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let ids = post_all(&fixture, &room_id, &["m1", "m2", "m3", "m4", "m5"]);
    let session = fixture.client.session(fixture.login().await.credentials);
    let channel = RoomType::Channel;

    let first = session.history(&room_id, &channel, None, 2).await.unwrap();
    assert!(!first.reached_start);
    let got: Vec<_> = first.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(got, [ids[4].as_str(), ids[3].as_str()]);

    // The next page starts at the oldest message held, which is inclusive: it comes back.
    let latest = first.messages[1].ts;
    let second = session
        .history(&room_id, &channel, Some(latest), 2)
        .await
        .unwrap();
    assert!(!second.reached_start);
    let got: Vec<_> = second.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(got, [ids[3].as_str(), ids[2].as_str()]);

    // Everything older, including the system message of the creator joining.
    let latest = second.messages[1].ts;
    let rest = session
        .history(&room_id, &channel, Some(latest), 50)
        .await
        .unwrap();
    assert!(rest.reached_start);
    let texts: Vec<_> = ordinary(&rest.messages)
        .iter()
        .map(|m| m.text.as_str())
        .collect();
    assert_eq!(texts, ["m3", "m2", "m1"]);
}

// MSG-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn history_has_system_messages_with_their_kind() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let other = fixture.admin.create_user();
    fixture.admin.set_topic(&room_id, "A new topic");
    fixture.admin.invite(&room_id, &other.id);
    fixture.admin.kick(&room_id, &other.id);
    let session = fixture.client.session(fixture.login().await.credentials);

    let page = session
        .history(&room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap();

    // Newest first.
    let events: Vec<_> = page
        .messages
        .iter()
        .filter_map(|m| Some((m.kind.as_deref()?, m.text.as_str())))
        .collect();
    assert_eq!(
        &events[..3],
        [
            ("ru", other.username.as_str()),
            ("au", other.username.as_str()),
            ("room_changed_topic", "A new topic"),
        ]
    );
    let topic = page
        .messages
        .iter()
        .find(|m| m.kind.as_deref() == Some("room_changed_topic"))
        .unwrap();
    assert_eq!(topic.author.username, fixture.admin.username);
}

// RICH-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn custom_emoji_lists_a_new_emoji_and_its_image_needs_no_login() {
    let fixture = Fixture::new();
    let name = format!("ct_emoji_{}", unique().replace('-', "_"));
    fixture.admin.create_emoji(&name);
    let session = fixture.client.session(fixture.login().await.credentials);

    let emoji = session.custom_emoji().await.unwrap();

    let found = emoji.iter().find(|emoji| emoji.name == name).unwrap();
    assert_eq!(found.extension, "png");
    let url = format!(
        "{}/emoji-custom/{name}.png",
        server_url().trim_end_matches('/')
    );
    assert_eq!(status_without_login(&url), 200);
}

// RICH-001, RICH-002, RICH-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn history_has_the_parsed_markdown_of_a_message() {
    use rc_api::md::{Block, Inline};
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    fixture.admin.post_message(
        &room_id,
        "**bold** `code` @all #general :smile: [x](https://example.com)",
    );
    fixture
        .admin
        .post_message(&room_id, "```rust\nfn main() {}\n```");
    let session = fixture.client.session(fixture.login().await.credentials);

    let page = session
        .history(&room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap();

    let messages = ordinary(&page.messages);
    assert_eq!(
        messages[0].md,
        Some(vec![Block::Code {
            language: Some("rust".into()),
            lines: vec!["fn main() {}".into()]
        }])
    );
    let Some([Block::Paragraph(inlines)]) = messages[1].md.as_deref() else {
        panic!("{:?}", messages[1].md);
    };
    assert_eq!(inlines[0], Inline::Bold(vec![Inline::Text("bold".into())]));
    assert_eq!(inlines[2], Inline::Code("code".into()));
    assert_eq!(inlines[4], Inline::MentionUser("all".into()));
    assert_eq!(inlines[6], Inline::MentionChannel("general".into()));
    assert_eq!(
        inlines[8],
        Inline::Emoji {
            short_code: Some("smile".into()),
            unicode: None
        }
    );
    assert_eq!(
        inlines[10],
        Inline::Link {
            url: "https://example.com".into(),
            label: vec![Inline::Text("x".into())]
        }
    );
}

// MSG-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn history_of_a_room_the_user_cannot_see_is_a_server_error() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.admin.create_group(&[]);
    let session = fixture.client.session(fixture.login().await.credentials);
    let result = session
        .history(&room_id, &RoomType::Private, None, 50)
        .await;
    assert!(matches!(result, Err(Error::Server { .. })), "{result:?}");
}

// AUTH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn realtime_login_resumes_with_the_rest_token() {
    let fixture = Fixture::new();
    let login = fixture.login().await;
    let (client, _events) = DdpClient::connect(&server_url()).await.unwrap();
    client.login(&login.credentials).await.unwrap();
}

// AUTH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn realtime_login_with_a_revoked_token_is_unauthorized() {
    let fixture = Fixture::new();
    let login = fixture.login().await;
    fixture
        .client
        .session(login.credentials.clone())
        .logout()
        .await
        .unwrap();
    let (client, _events) = DdpClient::connect(&server_url()).await.unwrap();
    let result = client.login(&login.credentials).await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// AUTH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn realtime_login_with_a_made_up_token_is_unauthorized() {
    let fixture = Fixture::new();
    let credentials = Credentials {
        user_id: fixture.user.id.clone(),
        token: SecretString::from(format!("not-a-token-{}", unique())),
    };
    let (client, _events) = DdpClient::connect(&server_url()).await.unwrap();
    let result = client.login(&credentials).await;
    assert!(matches!(result, Err(Error::Unauthorized)), "{result:?}");
}

// RT-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn live_messages_arrive_as_events() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();

    let posted = fixture.admin.post_message(&room_id, "hello live");

    let event = event_where(
        &mut events,
        |e| matches!(e, DdpEvent::Message(m) if m.id == posted.id),
    )
    .await;
    let DdpEvent::Message(message) = event else {
        panic!("not the event that was waited for")
    };
    assert_eq!(message.text, "hello live");
    assert_eq!(message.room_id, room_id);
    assert_eq!(message.kind, None);
    assert_eq!(message.author.username, fixture.admin.username);
}

// RT-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn live_system_messages_arrive_with_their_kind() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();

    fixture.admin.set_topic(&room_id, "A live topic");

    let event = event_where(&mut events, |e| {
        matches!(e, DdpEvent::Message(m) if m.room_id == room_id
            && m.kind.as_deref() == Some("room_changed_topic"))
    })
    .await;
    let DdpEvent::Message(message) = event else {
        panic!("not the event that was waited for")
    };
    assert_eq!(message.text, "A live topic");
}

// RT-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn messages_of_private_rooms_the_user_is_not_in_do_not_arrive() {
    let fixture = Fixture::new();
    // Not a channel: the server also sends the messages of public channels the user isn't in.
    let (elsewhere, _) = fixture.admin.create_group(&[]);
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();

    fixture.admin.post_message(&elsewhere, "not for the user");
    let last = fixture.admin.post_message(&room_id, "for the user");

    // Events keep their order, so seeing the second message without the first proves it.
    let seen = tokio::time::timeout(EVENT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            match event {
                DdpEvent::Message(m) if m.room_id == elsewhere => panic!("{m:?}"),
                DdpEvent::Message(m) if m.id == last.id => return true,
                _ => {}
            }
        }
        false
    })
    .await
    .unwrap();
    assert!(seen);
}

// RT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn subscription_changes_arrive_as_events() {
    let fixture = Fixture::new();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client
        .subscribe_subscription_changes(&fixture.user.id)
        .await
        .unwrap();

    let (room_id, name) = fixture.create_channel();
    let event = event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionChanged(s) if s.room_id == room_id),
    )
    .await;
    let DdpEvent::SubscriptionChanged(subscription) = event else {
        panic!("not the event that was waited for")
    };
    assert_eq!(subscription.room_type, RoomType::Channel);
    assert_eq!(subscription.name, name);

    fixture.admin.kick(&room_id, &fixture.user.id);
    let removed = event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionRemoved { room_id: r } if *r == room_id),
    )
    .await;
    assert_eq!(removed, DdpEvent::SubscriptionRemoved { room_id });
}

// RT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_message_raises_the_unread_count_of_its_room() {
    let fixture = Fixture::new();
    // A channel only counts mentions as unread by default, a direct message counts everything.
    let room_id = fixture.create_dm();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client
        .subscribe_subscription_changes(&fixture.user.id)
        .await
        .unwrap();

    fixture.admin.post_message(&room_id, "unread one");

    let event = event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionChanged(s) if s.room_id == room_id && s.unread >= 1),
    )
    .await;
    let DdpEvent::SubscriptionChanged(subscription) = event else {
        panic!("not the event that was waited for")
    };
    assert_eq!(subscription.room_type, RoomType::Direct);
    assert!(subscription.alert);
}

// ROOM-006, ROOM-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn favorites_unreads_and_leaving_arrive_as_subscription_changes() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let (left_id, _) = fixture.create_channel();
    fixture
        .admin
        .post_message(&room_id, "something to mark unread");
    let login = fixture.login().await;
    let session = fixture.client.session(login.credentials.clone());
    session.mark_read(&room_id).await.unwrap();
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client
        .subscribe_subscription_changes(&fixture.user.id)
        .await
        .unwrap();

    session.set_favorite(&room_id, true).await.unwrap();
    event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionChanged(s) if s.room_id == room_id && s.favorite),
    )
    .await;
    session.set_favorite(&room_id, false).await.unwrap();
    event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionChanged(s) if s.room_id == room_id && !s.favorite),
    )
    .await;

    session.mark_unread(&room_id).await.unwrap();
    let event = event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionChanged(s) if s.room_id == room_id && s.alert),
    )
    .await;
    let DdpEvent::SubscriptionChanged(subscription) = event else {
        panic!("not the event that was waited for")
    };
    assert_eq!(subscription.unread, 1);

    session.leave(&left_id, &RoomType::Channel).await.unwrap();
    event_where(
        &mut events,
        |e| matches!(e, DdpEvent::SubscriptionRemoved { room_id: r } if *r == left_id),
    )
    .await;
}

// RT-001, RT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn all_streams_deliver_on_one_connection() {
    let fixture = Fixture::new();
    let room_id = fixture.create_dm();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    client
        .subscribe_subscription_changes(&fixture.user.id)
        .await
        .unwrap();
    client
        .subscribe_room_activity(&fixture.user.id)
        .await
        .unwrap();

    let posted = fixture.admin.post_message(&room_id, "to every stream");

    let (mut message, mut subscription, mut activity) = (false, false, false);
    event_where(&mut events, |e| {
        match e {
            DdpEvent::Message(m) => message |= m.id == posted.id,
            DdpEvent::SubscriptionChanged(s) => {
                subscription |= s.room_id == room_id && s.unread >= 1;
            }
            DdpEvent::RoomActivity { room_id: r, .. } => activity |= *r == room_id,
            DdpEvent::SubscriptionRemoved { .. }
            | DdpEvent::AvatarChanged(_)
            | DdpEvent::MessageDeleted { .. } => {}
        }
        message && subscription && activity
    })
    .await;
}

// RT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn subscriptions_the_server_does_not_allow_are_refused() {
    let fixture = Fixture::new();
    let other = fixture.admin.create_user();
    let login = fixture.login().await;
    let (client, _events) = DdpClient::connect(&server_url()).await.unwrap();

    // Not logged in yet.
    let result = client
        .subscribe_subscription_changes(&fixture.user.id)
        .await;
    assert!(matches!(result, Err(Error::Refused { .. })), "{result:?}");

    client.login(&login.credentials).await.unwrap();
    // Logged in, but the streams are per user.
    let result = client.subscribe_subscription_changes(&other.id).await;
    assert!(matches!(result, Err(Error::Refused { .. })), "{result:?}");
    let result = client.subscribe_room_activity(&other.id).await;
    assert!(matches!(result, Err(Error::Refused { .. })), "{result:?}");

    // A refusal leaves the connection usable.
    client
        .subscribe_subscription_changes(&fixture.user.id)
        .await
        .unwrap();
}

// RT-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn an_idle_connection_outlives_the_servers_heartbeat() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();

    // Meteor pings after 15 s of silence and drops the connection 15 s after an unanswered ping.
    tokio::time::sleep(Duration::from_secs(35)).await;

    let posted = fixture.admin.post_message(&room_id, "still here");
    event_where(
        &mut events,
        |e| matches!(e, DdpEvent::Message(m) if m.id == posted.id),
    )
    .await;
}

// AUTH-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn connecting_to_a_closed_port_is_a_websocket_error() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let result = DdpClient::connect(&format!("http://127.0.0.1:{port}")).await;
    assert!(
        matches!(result, Err(Error::WebSocket(_))),
        "{:?}",
        result.err()
    );
}

// SEND-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_sent_message_is_stored_under_the_id_we_picked() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let id = unique().replace('-', "");

    let sent = session.send_message(&room_id, &id, "hello").await.unwrap();

    assert_eq!(
        (sent.id.as_str(), sent.text.as_str()),
        (id.as_str(), "hello")
    );
    assert_eq!(sent.room_id, room_id);
    assert_eq!(sent.author.id, fixture.user.id);
    let page = session
        .history(&room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap();
    let stored: Vec<_> = page.messages.iter().filter(|m| m.id == id).collect();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].ts, sent.ts);
}

// SEND-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn sending_the_same_id_twice_stores_one_message_and_answers_with_it() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let id = unique().replace('-', "");

    let first = session.send_message(&room_id, &id, "once").await.unwrap();
    // A retry whose first attempt had arrived: the text may even differ, the stored one wins.
    let second = session.send_message(&room_id, &id, "twice").await.unwrap();

    assert_eq!(second, first);
    let page = session
        .history(&room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap();
    let stored: Vec<_> = page.messages.iter().filter(|m| m.id == id).collect();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].text, "once");
}

// SEND-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn an_id_that_belongs_to_another_room_or_user_is_not_a_success() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let (other_room, _) = fixture.create_channel();
    let admin_message = fixture.admin.post_message(&room_id, "admin's");
    let session = fixture.client.session(fixture.login().await.credentials);

    // The id is taken by someone else's message.
    let result = session
        .send_message(&room_id, &admin_message.id, "mine")
        .await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );

    // The id is taken by one of ours, in another room.
    let id = unique().replace('-', "");
    session.send_message(&room_id, &id, "here").await.unwrap();
    let result = session.send_message(&other_room, &id, "there").await;
    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );
}

// SEND-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn sending_to_a_room_that_does_not_exist_is_a_server_error() {
    let fixture = Fixture::new();
    let session = fixture.client.session(fixture.login().await.credentials);
    let id = unique().replace('-', "");

    let result = session.send_message("noSuchRoom", &id, "hi").await;

    assert!(
        matches!(result, Err(Error::Server { status: 400, .. })),
        "{result:?}"
    );
}

// RICH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_new_user_has_a_generated_avatar_with_their_initial() {
    let fixture = Fixture::new();
    let session = fixture.client.session(fixture.login().await.credentials);

    let avatar = session.user_avatar(&fixture.user.username).await.unwrap();

    assert_eq!(avatar.content_type.as_deref(), Some("image/svg+xml"));
    let svg = String::from_utf8(avatar.bytes).unwrap();
    assert!(svg.starts_with("<svg"), "{svg}");
    // Usernames are `ct-user-...`.
    assert!(svg.contains(">\nC\n<"), "{svg}");
}

// RICH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_room_has_an_avatar_and_so_does_a_name_the_server_has_never_seen() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);

    let room = session.room_avatar(&room_id).await.unwrap();
    let stranger = session
        .user_avatar(&format!("nobody-{}/../x y", unique()))
        .await
        .unwrap();

    assert_eq!(room.content_type.as_deref(), Some("image/svg+xml"));
    assert_eq!(stranger.content_type.as_deref(), Some("image/svg+xml"));
}

// RICH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn users_and_rooms_with_the_generated_avatar_have_no_etag() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);

    assert_eq!(
        session
            .user_avatar_etag(&fixture.user.username)
            .await
            .unwrap(),
        None
    );
    assert_eq!(session.room_avatar_etag(&room_id).await.unwrap(), None);
    let rooms = session.subscriptions().await.unwrap();
    assert_eq!(find(&rooms, &room_id).avatar_etag, None);
}

// RICH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_new_avatar_is_announced_by_username() {
    let fixture = Fixture::new();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_avatar_changes().await.unwrap();

    fixture.admin.set_avatar(&fixture.user.username);

    let username = fixture.user.username.clone();
    event_where(&mut events, |e| {
        matches!(e, DdpEvent::AvatarChanged(rc_api::AvatarOwner::User { username: u }) if *u == username)
    })
    .await;
}

// RICH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_user_and_a_room_with_an_avatar_of_their_own_have_an_etag() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    fixture.admin.set_avatar(&fixture.user.username);
    fixture.admin.set_room_avatar(&room_id);
    let session = fixture.client.session(fixture.login().await.credentials);

    let user = session
        .user_avatar_etag(&fixture.user.username)
        .await
        .unwrap();
    let room = session.room_avatar_etag(&room_id).await.unwrap();
    let rooms = session.subscriptions().await.unwrap();

    assert!(user.is_some_and(|etag| !etag.is_empty()));
    assert!(room.is_some());
    assert_eq!(find(&rooms, &room_id).avatar_etag, room);
    // The download is the uploaded image, not the drawn one.
    let avatar = session.user_avatar(&fixture.user.username).await.unwrap();
    assert_eq!(avatar.content_type.as_deref(), Some("image/png"));
    let avatar = session.room_avatar(&room_id).await.unwrap();
    assert_eq!(avatar.content_type.as_deref(), Some("image/png"));
}

/// Uploads a one-pixel PNG with text and a description, and a text file, to the channel as admin.
/// Returns the ids of the two messages.
fn upload_image_and_file(fixture: &Fixture, room_id: &str) -> (String, String) {
    let image = fixture.admin.upload_file(
        room_id,
        "pixel.png",
        "image/png",
        &PIXEL_PNG,
        "look **here**",
        "a pixel",
    );
    let file = fixture.admin.upload_file(
        room_id,
        "notes.txt",
        "text/plain",
        b"hello\n",
        "",
        "my notes",
    );
    (image.id, file.id)
}

// RICH-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn uploaded_files_come_back_as_attachments_and_download_with_the_login() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let (image_id, file_id) = upload_image_and_file(&fixture, &room_id);
    let session = fixture.client.session(fixture.login().await.credentials);

    let page = session
        .history(&room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap();

    let message = |id: &str| page.messages.iter().find(|m| m.id == id).unwrap();
    let image = &message(&image_id);
    assert_eq!(image.text, "look **here**");
    assert_eq!(image.attachments.len(), 1);
    let attachment = &image.attachments[0];
    assert_eq!(attachment.title, "pixel.png");
    assert_eq!(attachment.description.as_deref(), Some("a pixel"));
    assert!(
        attachment.link.starts_with("/file-upload/"),
        "{attachment:?}"
    );
    let picture = attachment.image.as_ref().unwrap();
    assert!(picture.path.starts_with("/file-upload/"), "{picture:?}");
    assert_eq!(picture.content_type.as_deref(), Some("image/png"));
    assert_eq!(picture.dimensions, Some((1, 1)));
    // A file of one pixel has no thumbnail: the picture is the original.
    let download = session.file(&picture.path).await.unwrap();
    assert_eq!(download.bytes, PIXEL_PNG);
    assert_eq!(download.content_type.as_deref(), Some("image/png"));
    assert_eq!(
        session.file(&attachment.link).await.unwrap().bytes,
        PIXEL_PNG
    );

    let file = &message(&file_id);
    assert_eq!(file.text, "");
    assert_eq!(file.attachments.len(), 1);
    let attachment = &file.attachments[0];
    assert_eq!(attachment.title, "notes.txt");
    assert_eq!(attachment.description.as_deref(), Some("my notes"));
    assert_eq!(attachment.size, Some(6));
    assert_eq!(attachment.image, None);
    let download = session.file(&attachment.link).await.unwrap();
    assert_eq!(download.bytes, b"hello\n");
    assert_eq!(download.content_type.as_deref(), Some("text/plain"));
}

// RICH-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn an_uploaded_file_is_not_served_without_the_login() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let (image_id, _) = upload_image_and_file(&fixture, &room_id);
    let session = fixture.client.session(fixture.login().await.credentials);
    let page = session
        .history(&room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap();
    let message = page.messages.iter().find(|m| m.id == image_id).unwrap();

    let link = &message.attachments[0].link;

    let url = format!("{}{link}", server_url().trim_end_matches('/'));
    assert_eq!(status_without_login(&url), 403);
}

/// The live message with this id.
async fn message_event(events: &mut DdpEvents, id: &str) -> Message {
    let event = event_where(events, |e| matches!(e, DdpEvent::Message(m) if m.id == id)).await;
    let DdpEvent::Message(message) = event else {
        panic!("not the event that was waited for")
    };
    message
}

// RICH-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_live_message_carries_its_attachments() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();

    let (image_id, file_id) = upload_image_and_file(&fixture, &room_id);

    let image = message_event(&mut events, &image_id).await;
    assert_eq!(image.attachments.len(), 1);
    assert_eq!(image.attachments[0].title, "pixel.png");
    assert!(image.attachments[0].image.is_some());
    let file = message_event(&mut events, &file_id).await;
    assert_eq!(file.attachments.len(), 1);
    assert_eq!(file.attachments[0].title, "notes.txt");
    assert_eq!(file.attachments[0].image, None);
}

/// The next live version of the message with this id that `matches` accepts.
async fn message_event_where(
    events: &mut DdpEvents,
    id: &str,
    mut matches: impl FnMut(&Message) -> bool,
) -> Message {
    let event = event_where(
        events,
        |e| matches!(e, DdpEvent::Message(m) if m.id == id && matches(m)),
    )
    .await;
    let DdpEvent::Message(message) = event else {
        panic!("not the event that was waited for")
    };
    message
}

// RICH-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_live_edit_arrives_as_the_whole_message_marked_edited() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    let posted = fixture.admin.post_message(&room_id, "before");
    let original = message_event(&mut events, &posted.id).await;
    assert!(!original.edited);

    fixture
        .admin
        .edit_message(&room_id, &posted.id, "after the edit");

    let edited = message_event_where(&mut events, &posted.id, |m| m.edited).await;
    assert_eq!(edited.text, "after the edit");
    assert_eq!(edited.room_id, room_id);
    assert_eq!(edited.ts, original.ts);
    assert!(edited.md.is_some());
}

// RICH-007
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_live_reaction_arrives_and_taking_it_back_leaves_none() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    let posted = fixture.admin.post_message(&room_id, "react to me");
    message_event(&mut events, &posted.id).await;

    fixture.admin.react(&posted.id, ":thumbsup:", true);
    let reacted = message_event_where(&mut events, &posted.id, |m| !m.reactions.is_empty()).await;
    assert_eq!(reacted.reactions.len(), 1);
    assert_eq!(reacted.reactions[0].emoji, ":thumbsup:");
    assert_eq!(
        reacted.reactions[0].usernames,
        std::slice::from_ref(&fixture.admin.username)
    );
    assert!(!reacted.edited, "a reaction is not an edit");

    fixture.admin.react(&posted.id, ":thumbsup:", false);
    let cleared = message_event_where(&mut events, &posted.id, |m| m.reactions.is_empty()).await;
    assert_eq!(cleared.text, "react to me");
}

// RICH-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_deletion_arrives_on_the_rooms_deletion_subscription() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    client.subscribe_room_deletions(&room_id).await.unwrap();
    let posted = fixture.admin.post_message(&room_id, "to delete");

    fixture.admin.delete_message(&room_id, &posted.id);

    let expected = DdpEvent::MessageDeleted {
        room_id: room_id.clone(),
        message_id: posted.id.clone(),
    };
    assert_eq!(event_where(&mut events, |e| *e == expected).await, expected);
}

// RICH-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_room_that_was_unsubscribed_from_reports_no_deletions() {
    let fixture = Fixture::new();
    let (ended_room, _) = fixture.create_channel();
    let (watched_room, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    let subscription = client.subscribe_room_deletions(&ended_room).await.unwrap();
    client.unsubscribe(&subscription);
    // The server takes the frames of a connection in order, so once this one is ready the
    // unsubscription has been done.
    client
        .subscribe_room_deletions(&watched_room)
        .await
        .unwrap();
    let ended = fixture.admin.post_message(&ended_room, "deleted unseen");
    let watched = fixture.admin.post_message(&watched_room, "deleted seen");

    fixture.admin.delete_message(&ended_room, &ended.id);
    fixture.admin.delete_message(&watched_room, &watched.id);

    // The deletions happened in this order, so the first event is the first one the connection
    // was told about.
    let event = event_where(&mut events, |e| {
        matches!(e, DdpEvent::MessageDeleted { .. })
    })
    .await;
    assert_eq!(
        event,
        DdpEvent::MessageDeleted {
            room_id: watched_room,
            message_id: watched.id,
        }
    );
}

// RICH-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn sync_messages_reports_an_edited_message_and_a_deleted_one() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let untouched = fixture.admin.post_message(&room_id, "untouched");
    let edited = fixture.admin.post_message(&room_id, "before");
    let deleted = fixture.admin.post_message(&room_id, "to delete");
    let since = jiff::Timestamp::now() - Duration::from_secs(60);
    fixture
        .admin
        .edit_message(&room_id, &edited.id, "after the edit");
    fixture.admin.delete_message(&room_id, &deleted.id);

    let changes = session.sync_messages(&room_id, since).await.unwrap();

    let edit = changes
        .updated
        .iter()
        .find(|m| m.id == edited.id)
        .expect("the edited message is in `updated`");
    assert_eq!(edit.text, "after the edit");
    assert!(edit.edited);
    assert_eq!(changes.deleted, std::slice::from_ref(&deleted.id));
    // Messages sent in the window are `updated` too, which is why the caller only applies the
    // ones it holds.
    assert!(changes.updated.iter().any(|m| m.id == untouched.id));
    assert!(changes.updated.iter().all(|m| m.id != deleted.id));
}

// RICH-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn sync_messages_of_a_room_the_user_cannot_see_is_a_server_error() {
    let fixture = Fixture::new();
    let outsiders_room = fixture.admin.create_group(&[]).0;
    let session = fixture.client.session(fixture.login().await.credentials);

    let result = session
        .sync_messages(&outsiders_room, jiff::Timestamp::now())
        .await;

    assert!(matches!(result, Err(Error::Server { .. })), "{result:?}");
}

impl Fixture {
    /// The new user, acting through the admin helpers: their own posts.
    fn as_user(&self) -> Admin {
        Admin::login_as(&self.user.username, &self.user.password)
    }
}

/// The newest messages of the channel.
async fn channel_history(session: &rc_api::RestSession, room_id: &str) -> Vec<Message> {
    session
        .history(room_id, &RoomType::Channel, None, 50)
        .await
        .unwrap()
        .messages
}

// ACT-001
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_user_edits_their_own_message_and_the_history_shows_it_edited() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let posted = fixture.as_user().post_message(&room_id, "before");

    let stored = session
        .edit_message(&room_id, &posted.id, "after the edit")
        .await
        .unwrap();

    assert_eq!(stored.id, posted.id);
    assert_eq!(stored.text, "after the edit");
    assert!(stored.edited);
    let history = channel_history(&session, &room_id).await;
    let found = history.iter().find(|m| m.id == posted.id).unwrap();
    assert_eq!(found.text, "after the edit");
    assert!(found.edited);
}

// ACT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_user_deletes_their_own_message_and_it_leaves_the_history() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let kept = fixture.as_user().post_message(&room_id, "kept");
    let doomed = fixture.as_user().post_message(&room_id, "doomed");

    session.delete_message(&room_id, &doomed.id).await.unwrap();

    let history = channel_history(&session, &room_id).await;
    assert!(history.iter().all(|m| m.id != doomed.id));
    assert!(history.iter().any(|m| m.id == kept.id));
}

// ACT-001, ACT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_user_cannot_edit_or_delete_the_message_of_somebody_else() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let theirs = fixture.admin.post_message(&room_id, "not yours");

    let edit = session.edit_message(&room_id, &theirs.id, "mine now").await;
    let delete = session.delete_message(&room_id, &theirs.id).await;

    assert!(matches!(edit, Err(Error::Server { .. })), "{edit:?}");
    assert!(matches!(delete, Err(Error::Server { .. })), "{delete:?}");
    let history = channel_history(&session, &room_id).await;
    let found = history.iter().find(|m| m.id == theirs.id).unwrap();
    assert_eq!(found.text, "not yours");
}

// ACT-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_user_reacts_and_takes_the_reaction_back_and_the_history_follows() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let theirs = fixture.admin.post_message(&room_id, "react to me");

    session.react(&theirs.id, ":tada:", true).await.unwrap();
    let history = channel_history(&session, &room_id).await;
    let found = history.iter().find(|m| m.id == theirs.id).unwrap();
    assert_eq!(found.reactions.len(), 1);
    assert_eq!(found.reactions[0].emoji, ":tada:");
    assert_eq!(
        found.reactions[0].usernames,
        std::slice::from_ref(&fixture.user.username)
    );

    session.react(&theirs.id, ":tada:", false).await.unwrap();
    let history = channel_history(&session, &room_id).await;
    let found = history.iter().find(|m| m.id == theirs.id).unwrap();
    assert!(found.reactions.is_empty());
}

// ACT-003
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn reacting_twice_leaves_one_user_and_an_unknown_emoji_is_refused() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let posted = fixture.admin.post_message(&room_id, "react to me");

    session.react(&posted.id, ":tada:", true).await.unwrap();
    session.react(&posted.id, ":tada:", true).await.unwrap();
    let refused = session
        .react(&posted.id, ":no_such_emoji_here:", true)
        .await;

    assert!(matches!(refused, Err(Error::Server { .. })), "{refused:?}");
    let history = channel_history(&session, &room_id).await;
    let found = history.iter().find(|m| m.id == posted.id).unwrap();
    assert_eq!(found.reactions.len(), 1);
    assert_eq!(found.reactions[0].usernames.len(), 1);
}

// ACT-001, ACT-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn message_settings_are_the_servers_with_editing_and_deleting_allowed_without_limit() {
    let fixture = Fixture::new();
    let session = fixture.client.session(fixture.login().await.credentials);

    let settings = session.message_settings().await.unwrap();

    // The test server keeps the defaults for these settings.
    assert_eq!(settings, rc_api::MessageSettings::default());
}

// ACT-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn the_site_url_is_known_without_a_login() {
    let fixture = Fixture::new();

    let site_url = fixture.client.site_url().await.unwrap();

    assert_eq!(site_url, Some(fixture.admin.site_url()));
}

// ACT-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_reply_with_the_permalink_of_a_message_comes_back_with_its_quote() {
    let fixture = Fixture::new();
    let (room_id, name) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let original = fixture
        .admin
        .post_message(&room_id, "original **bold** text");

    let site_url = fixture.client.site_url().await.unwrap().unwrap();
    let link = format!("{site_url}/channel/{name}?msg={}", original.id);
    let reply_id = unique().replace('-', "");
    session
        .send_message(&room_id, &reply_id, &format!("[ ]({link}) my reply"))
        .await
        .unwrap();

    let history = channel_history(&session, &room_id).await;
    let reply = history.iter().find(|m| m.id == reply_id).unwrap();
    assert_eq!(reply.text, format!("[ ]({link}) my reply"));
    let [quote] = reply.quotes.as_slice() else {
        panic!("expected one quote, got {:?}", reply.quotes);
    };
    assert_eq!(quote.link, link);
    assert_eq!(quote.author, fixture.admin.username);
    assert_eq!(quote.text, "original **bold** text");
    assert!(quote.md.is_some());
    assert_eq!(quote.ts, Some(original.ts.parse().unwrap()));
    assert_eq!(quote.quoted_links, Vec::<String>::new());
    let original = history.iter().find(|m| m.id == original.id).unwrap();
    assert_eq!(original.quotes, []);
}

// ACT-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_quote_of_a_quote_has_the_link_of_the_inner_one() {
    let fixture = Fixture::new();
    let (room_id, name) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let path = format!("channel/{name}");
    let original = fixture.admin.post_message(&room_id, "original");
    let first = fixture
        .admin
        .post_quote(&room_id, &path, &original.id, "first");
    let second = fixture
        .admin
        .post_quote(&room_id, &path, &first.id, "second");

    let history = channel_history(&session, &room_id).await;

    let reply = history.iter().find(|m| m.id == second.id).unwrap();
    let [quote] = reply.quotes.as_slice() else {
        panic!("expected one quote, got {:?}", reply.quotes);
    };
    assert!(quote.link.ends_with(&first.id));
    assert!(quote.text.ends_with(") first"));
    assert_eq!(quote.quoted_links.len(), 1);
    assert!(quote.quoted_links[0].ends_with(&original.id));
}

// ACT-005
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_permalink_with_another_host_makes_no_quote() {
    let fixture = Fixture::new();
    let (room_id, name) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let original = fixture.admin.post_message(&room_id, "original");

    let link = format!(
        "http://not-the-site-url.invalid/channel/{name}?msg={}",
        original.id
    );
    fixture
        .admin
        .post_message(&room_id, &format!("[ ]({link}) reply"));

    let history = channel_history(&session, &room_id).await;
    assert!(history.iter().all(|m| m.quotes.is_empty()));
}

/// A thread in a new channel: the parent, and replies `r1`, `r2`... posted by admin in that order,
/// every other one also to the channel (`r2`, `r4`...).
struct Thread {
    room_id: String,
    parent: rc_test_server::Posted,
    replies: Vec<rc_test_server::Posted>,
}

fn thread_of(fixture: &Fixture, replies: usize) -> Thread {
    let (room_id, _) = fixture.create_channel();
    let parent = fixture.admin.post_message(&room_id, "parent");
    let replies = (1..=replies)
        .map(|n| {
            fixture
                .admin
                .post_in_thread(&room_id, &parent.id, &format!("r{n}"), n % 2 == 0)
        })
        .collect();
    Thread {
        room_id,
        parent,
        replies,
    }
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn thread_messages_are_the_replies_newest_first_without_the_parent() {
    let fixture = Fixture::new();
    let thread = thread_of(&fixture, 3);
    let session = fixture.client.session(fixture.login().await.credentials);

    let page = session
        .thread_messages(&thread.parent.id, 50, 0)
        .await
        .unwrap();

    let texts: Vec<_> = page.messages.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts, ["r3", "r2", "r1"]);
    assert_eq!(page.total, 3);
    assert!(page.reached_start);
    for message in &page.messages {
        assert_eq!(message.thread_id.as_ref(), Some(&thread.parent.id));
        assert_eq!(message.room_id, thread.room_id);
    }
    let shown: Vec<_> = page.messages.iter().map(|m| m.thread_shown).collect();
    assert_eq!(shown, [false, true, false]);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn thread_messages_page_back_by_offset_until_the_start() {
    let fixture = Fixture::new();
    let thread = thread_of(&fixture, 5);
    let session = fixture.client.session(fixture.login().await.credentials);

    let first = session
        .thread_messages(&thread.parent.id, 2, 0)
        .await
        .unwrap();
    let second = session
        .thread_messages(&thread.parent.id, 2, 2)
        .await
        .unwrap();
    let third = session
        .thread_messages(&thread.parent.id, 2, 4)
        .await
        .unwrap();

    let texts = |page: &rc_api::ThreadPage| -> Vec<String> {
        page.messages.iter().map(|m| m.text.clone()).collect()
    };
    assert_eq!(texts(&first), ["r5", "r4"]);
    assert_eq!(texts(&second), ["r3", "r2"]);
    assert_eq!(texts(&third), ["r1"]);
    assert_eq!(
        [
            first.reached_start,
            second.reached_start,
            third.reached_start
        ],
        [false, false, true]
    );
    assert_eq!(first.total, 5);
    // A page that ends exactly at the start knows it, without a further request.
    let exact = session
        .thread_messages(&thread.parent.id, 3, 2)
        .await
        .unwrap();
    assert_eq!(texts(&exact), ["r3", "r2", "r1"]);
    assert!(exact.reached_start);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn thread_messages_of_something_that_is_not_a_message_is_a_server_error() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);

    for id in ["noSuchMessage", room_id.as_str()] {
        let result = session.thread_messages(id, 50, 0).await;
        assert!(matches!(result, Err(Error::Server { .. })), "{result:?}");
    }
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_thread_without_replies_is_empty() {
    let fixture = Fixture::new();
    let thread = thread_of(&fixture, 0);
    let session = fixture.client.session(fixture.login().await.credentials);

    let page = session
        .thread_messages(&thread.parent.id, 50, 0)
        .await
        .unwrap();

    assert_eq!((page.messages.len(), page.total), (0, 0));
    assert!(page.reached_start);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_thread_reply_is_stored_in_the_thread_and_the_parent_counts_it() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let parent = fixture.admin.post_message(&room_id, "parent");
    let id = unique().replace('-', "");

    let sent = session
        .send_thread_reply(&room_id, &id, "in the thread", &parent.id, false)
        .await
        .unwrap();

    assert_eq!(sent.id, id);
    assert_eq!(sent.thread_id.as_ref(), Some(&parent.id));
    assert!(!sent.thread_shown);
    let page = session.thread_messages(&parent.id, 50, 0).await.unwrap();
    assert_eq!(page.messages, std::slice::from_ref(&sent));
    let history = channel_history(&session, &room_id).await;
    let held = history.iter().find(|m| m.id == parent.id).unwrap();
    assert_eq!(held.thread_replies, 1);
    assert_eq!(held.thread_last_reply, Some(sent.ts));
    // Not sent to the channel, the reply is not in its history.
    assert!(history.iter().all(|m| m.id != id));
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_thread_reply_also_sent_to_the_channel_is_in_its_history_too() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let parent = fixture.admin.post_message(&room_id, "parent");
    let id = unique().replace('-', "");

    session
        .send_thread_reply(&room_id, &id, "for everybody", &parent.id, true)
        .await
        .unwrap();

    let history = channel_history(&session, &room_id).await;
    let shown = history.iter().find(|m| m.id == id).unwrap();
    assert_eq!(shown.thread_id.as_ref(), Some(&parent.id));
    assert!(shown.thread_shown);
    let page = session.thread_messages(&parent.id, 50, 0).await.unwrap();
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].id, id);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_retried_thread_reply_is_stored_once() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let session = fixture.client.session(fixture.login().await.credentials);
    let parent = fixture.admin.post_message(&room_id, "parent");
    let id = unique().replace('-', "");

    let first = session
        .send_thread_reply(&room_id, &id, "once", &parent.id, false)
        .await
        .unwrap();
    let second = session
        .send_thread_reply(&room_id, &id, "twice", &parent.id, false)
        .await
        .unwrap();

    assert_eq!(second, first);
    let page = session.thread_messages(&parent.id, 50, 0).await.unwrap();
    assert_eq!(page.total, 1);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn live_thread_replies_arrive_as_events_with_their_thread() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    let parent = fixture.admin.post_message(&room_id, "parent");

    let hidden = fixture
        .admin
        .post_in_thread(&room_id, &parent.id, "in the thread", false);
    let shown = fixture
        .admin
        .post_in_thread(&room_id, &parent.id, "and the channel", true);

    // Replies are pushed whether they are shown in the channel or not.
    let hidden = message_event(&mut events, &hidden.id).await;
    assert_eq!(hidden.thread_id.as_ref(), Some(&parent.id));
    assert!(!hidden.thread_shown);
    assert_eq!(hidden.room_id, room_id);
    let shown = message_event(&mut events, &shown.id).await;
    assert_eq!(shown.thread_id.as_ref(), Some(&parent.id));
    assert!(shown.thread_shown);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn the_server_pushes_the_parent_again_with_its_new_reply_count_when_a_reply_is_posted() {
    let fixture = Fixture::new();
    let (room_id, _) = fixture.create_channel();
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    let parent = fixture.admin.post_message(&room_id, "parent");
    let original = message_event(&mut events, &parent.id).await;
    assert_eq!(original.thread_replies, 0);
    assert_eq!(original.thread_last_reply, None);

    let first = fixture
        .admin
        .post_in_thread(&room_id, &parent.id, "first", false);
    let counted = message_event_where(&mut events, &parent.id, |m| m.thread_replies == 1).await;
    assert_eq!(counted.thread_last_reply, Some(first.ts.parse().unwrap()));
    assert_eq!(counted.text, "parent");
    assert!(!counted.edited, "a reply is not an edit");

    let second = fixture
        .admin
        .post_in_thread(&room_id, &parent.id, "second", false);
    let counted = message_event_where(&mut events, &parent.id, |m| m.thread_replies == 2).await;
    assert_eq!(counted.thread_last_reply, Some(second.ts.parse().unwrap()));
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn the_server_does_not_push_the_parent_when_a_reply_is_deleted() {
    let fixture = Fixture::new();
    let thread = thread_of(&fixture, 2);
    let login = fixture.login().await;
    let session = fixture.client.session(login.credentials.clone());
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    client
        .subscribe_room_deletions(&thread.room_id)
        .await
        .unwrap();

    fixture
        .admin
        .delete_message(&thread.room_id, &thread.replies[1].id);
    // Events come in the order the server made them, so once the marker has arrived a push of the
    // parent that the deletion caused would have too.
    let marker = fixture.admin.post_message(&thread.room_id, "marker");
    let mut seen = Vec::new();
    loop {
        let event = event_where(&mut events, |_| true).await;
        if matches!(&event, DdpEvent::Message(m) if m.id == marker.id) {
            break;
        }
        seen.push(event);
    }

    let deleted = DdpEvent::MessageDeleted {
        room_id: thread.room_id.clone(),
        message_id: thread.replies[1].id.clone(),
    };
    assert!(seen.contains(&deleted), "{seen:?}");
    assert!(
        !seen
            .iter()
            .any(|e| matches!(e, DdpEvent::Message(m) if m.id == thread.parent.id)),
        "{seen:?}"
    );
    // The server did count it, and left the time of the last reply alone.
    let history = channel_history(&session, &thread.room_id).await;
    let parent = history.iter().find(|m| m.id == thread.parent.id).unwrap();
    assert_eq!(parent.thread_replies, 1);
    assert_eq!(
        parent.thread_last_reply,
        Some(thread.replies[1].ts.parse().unwrap())
    );
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn sync_messages_reports_the_changed_and_the_deleted_thread_replies_and_the_parent() {
    let fixture = Fixture::new();
    let thread = thread_of(&fixture, 3);
    let session = fixture.client.session(fixture.login().await.credentials);
    let since = jiff::Timestamp::now() - Duration::from_secs(60);
    fixture
        .admin
        .edit_message(&thread.room_id, &thread.replies[0].id, "r1 edited");
    fixture
        .admin
        .delete_message(&thread.room_id, &thread.replies[2].id);

    let changes = session.sync_messages(&thread.room_id, since).await.unwrap();

    // Replies are in the catch-up like any message, shown in the channel or not.
    let edited = changes
        .updated
        .iter()
        .find(|m| m.id == thread.replies[0].id)
        .expect("the edited reply is in `updated`");
    assert_eq!(edited.text, "r1 edited");
    assert_eq!(edited.thread_id.as_ref(), Some(&thread.parent.id));
    assert!(!edited.thread_shown);
    assert_eq!(changes.deleted, std::slice::from_ref(&thread.replies[2].id));
    // The parent comes with the count the server has now.
    let parent = changes
        .updated
        .iter()
        .find(|m| m.id == thread.parent.id)
        .expect("the parent is in `updated`");
    assert_eq!(parent.thread_replies, 2);
}

// ACT-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn a_deleted_parent_is_pushed_as_a_removed_message_and_not_as_a_deletion() {
    let fixture = Fixture::new();
    let thread = thread_of(&fixture, 1);
    let login = fixture.login().await;
    let (client, mut events) = connect_and_login(&login.credentials).await;
    client.subscribe_my_messages().await.unwrap();
    client
        .subscribe_room_deletions(&thread.room_id)
        .await
        .unwrap();

    fixture
        .admin
        .delete_message(&thread.room_id, &thread.parent.id);

    // The server keeps a parent that has replies, as a message of the removed kind without text.
    let removed = message_event_where(&mut events, &thread.parent.id, |m| {
        m.kind.as_deref() == Some("rm")
    })
    .await;
    assert_eq!(removed.text, "");
    assert_eq!(removed.thread_replies, 1);
    // Its reply is still there.
    let session = fixture.client.session(login.credentials);
    let page = session
        .thread_messages(&thread.parent.id, 50, 0)
        .await
        .unwrap();
    assert_eq!(page.messages.len(), 1);
}
