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
use rc_test_server::{Admin, TestUser, server_url, unique};
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
            DdpEvent::SubscriptionRemoved { .. } => {}
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
