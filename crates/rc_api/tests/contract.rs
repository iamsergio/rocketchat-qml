// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The REST client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Every test makes its own user and rooms with unique names and only looks at those, so the tests
//! hold on a dirty server, in any order and in parallel. Server settings are never changed.

// Test helpers outside #[test] fns may unwrap: a failure should just fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use rc_api::{Error, RestClient, RoomType, ServerVersion, Subscription, User};
use rc_test_server::{Admin, TestUser, server_url, unique};
use secrecy::{ExposeSecret, SecretString};

const POLL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

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
