// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The REST client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Every test makes its own user and rooms with unique names and only looks at those, so the tests
//! hold on a dirty server, in any order and in parallel. Server settings are never changed.

// Test helpers outside #[test] fns may unwrap: a failure should just fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rc_api::{Error, RestClient, RoomType, ServerVersion, Subscription, User};
use reqwest::header::{HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

const POLL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn server_url() -> String {
    std::env::var("RC_TEST_SERVER_URL")
        .expect("RC_TEST_SERVER_URL is not set: point it at a Rocket.Chat server, e.g. http://127.0.0.1:3000")
}

/// Unique across processes, runs and parallel tests.
fn unique() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{}-{nanos}-{}",
        process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// A fresh user, plus what's needed to create rooms and messages for it as admin.
struct Fixture {
    client: RestClient,
    admin_http: reqwest::Client,
    admin: User,
    user: User,
    password: SecretString,
}

impl Fixture {
    async fn new() -> Self {
        let client = RestClient::new(&server_url()).unwrap();
        let admin_name = std::env::var("RC_ADMIN_USER").unwrap_or_else(|_| "admin".into());
        let admin_pass = std::env::var("RC_ADMIN_PASS").unwrap_or_else(|_| "admin".into());
        let admin = client
            .login(&admin_name, &SecretString::from(admin_pass))
            .await
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-User-Id",
            HeaderValue::from_str(&admin.credentials.user_id).unwrap(),
        );
        headers.insert(
            "X-Auth-Token",
            HeaderValue::from_str(admin.credentials.token.expose_secret()).unwrap(),
        );
        let admin_http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let suffix = unique();
        let username = format!("ct-user-{suffix}");
        let password = SecretString::from(format!("pw-{suffix}"));
        let mut fixture = Self {
            client,
            admin_http,
            admin: admin.user,
            user: User {
                id: String::new(),
                username: username.clone(),
                name: Some(format!("Contract {suffix}")),
            },
            password,
        };
        let created = fixture
            .admin_post(
                "users.create",
                &json!({
                    "email": format!("{username}@example.com"),
                    "name": fixture.user.name,
                    "username": username,
                    "password": fixture.password.expose_secret(),
                    "verified": true,
                }),
            )
            .await;
        created["user"]["_id"]
            .as_str()
            .unwrap()
            .clone_into(&mut fixture.user.id);
        fixture
    }

    /// POSTs to an admin-only endpoint and returns the JSON, which must say `success`.
    async fn admin_post(&self, endpoint: &str, body: &Value) -> Value {
        let url = format!("{}/api/v1/{endpoint}", server_url().trim_end_matches('/'));
        let response = self.admin_http.post(url).json(body).send().await.unwrap();
        let status = response.status();
        let json: Value = response.json().await.unwrap();
        assert!(
            status.is_success() && json["success"] == true,
            "{endpoint}: {status} {json}"
        );
        json
    }

    /// The new user logs in, as the app would.
    async fn login(&self) -> rc_api::LoginResult {
        self.client
            .login(&self.user.username, &self.password)
            .await
            .unwrap()
    }

    async fn create_channel(&self) -> (String, String) {
        let name = format!("ct-chan-{}", unique());
        let json = self
            .admin_post(
                "channels.create",
                &json!({ "name": name, "members": [self.user.username] }),
            )
            .await;
        (json["channel"]["_id"].as_str().unwrap().to_owned(), name)
    }

    async fn create_group(&self) -> (String, String) {
        let name = format!("ct-priv-{}", unique());
        let json = self
            .admin_post(
                "groups.create",
                &json!({ "name": name, "members": [self.user.username] }),
            )
            .await;
        (json["group"]["_id"].as_str().unwrap().to_owned(), name)
    }

    /// A direct message between admin and the user.
    async fn create_dm(&self) -> String {
        let json = self
            .admin_post("im.create", &json!({ "username": self.user.username }))
            .await;
        json["room"]["_id"].as_str().unwrap().to_owned()
    }

    async fn post_message(&self, room_id: &str, text: &str) {
        self.admin_post(
            "chat.postMessage",
            &json!({ "roomId": room_id, "text": text }),
        )
        .await;
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
    let fixture = Fixture::new().await;
    let result = fixture.login().await;
    assert_eq!(result.credentials.user_id, fixture.user.id);
    assert!(!result.credentials.token.expose_secret().is_empty());
    assert_eq!(result.user, fixture.user);
}

// AUTH-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn wrong_password_is_invalid_credentials() {
    let fixture = Fixture::new().await;
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
    let fixture = Fixture::new().await;
    let login = fixture.login().await;
    let me = fixture
        .client
        .session(login.credentials)
        .me()
        .await
        .unwrap();
    assert_eq!(me, fixture.user);
}

// AUTH-004
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn stored_credentials_resume_the_session() {
    let fixture = Fixture::new().await;
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
    let fixture = Fixture::new().await;
    let (channel_id, channel_name) = fixture.create_channel().await;
    let (group_id, group_name) = fixture.create_group().await;
    let dm_id = fixture.create_dm().await;
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
    let fixture = Fixture::new().await;
    let (channel_id, _) = fixture.create_channel().await;
    let dm_id = fixture.create_dm().await;
    fixture
        .post_message(&channel_id, &format!("hello @{}", fixture.user.username))
        .await;
    fixture.post_message(&dm_id, "hi there").await;
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
    let fixture = Fixture::new().await;
    let session = fixture.client.session(fixture.login().await.credentials);
    session.me().await.unwrap();

    session.logout().await.unwrap();

    let me = session.me().await;
    assert!(matches!(me, Err(Error::Unauthorized)), "{me:?}");
    let again = session.logout().await;
    assert!(matches!(again, Err(Error::Unauthorized)), "{again:?}");
}
