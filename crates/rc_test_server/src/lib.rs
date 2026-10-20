// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Test support: creates users, rooms and messages on the local Rocket.Chat test server
//! (`scripts/run_test_server.sh`), named by `RC_TEST_SERVER_URL`, through its admin API. The
//! contract tests and the GUI tests share it.
//!
//! Everything is named with [`unique`], so tests hold on a dirty server, in any order and in
//! parallel. Calls block: they only stand in for what a person would do in another client.
//!
//! [`Proxy`] sits between a client and the server, to cut the connections.

#![forbid(unsafe_code)]
// Test support: a failed call should just fail the test.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

mod proxy;

use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

pub use proxy::Proxy;

/// The environment variable naming the server.
pub const SERVER_ENV: &str = "RC_TEST_SERVER_URL";

const TIMEOUT: Duration = Duration::from_secs(30);

/// The URL in [`SERVER_ENV`].
///
/// # Panics
///
/// If it is unset.
#[must_use]
pub fn server_url() -> String {
    std::env::var(SERVER_ENV).unwrap_or_else(|_| {
        panic!(
            "{SERVER_ENV} is not set: point it at a Rocket.Chat server, e.g. http://127.0.0.1:3000"
        )
    })
}

/// The status of an unauthenticated GET of `url`: whether the server hands out a file to anyone.
///
/// # Panics
///
/// If the server is unreachable.
#[must_use]
pub fn status_without_login(url: &str) -> u16 {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    agent.get(url).call().unwrap().status().as_u16()
}

/// A string that is unique across processes, runs and parallel tests.
#[must_use]
pub fn unique() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{}-{nanos}-{}",
        process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// The image of the custom emoji [`Admin::create_emoji`] makes: one pixel.
const PIXEL_PNG: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A user made by [`Admin::create_user`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestUser {
    pub id: String,
    pub username: String,
    pub password: String,
    /// The display name.
    pub name: String,
}

/// A message made by [`Admin::post_message`], or found by [`Admin::last_message`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    pub id: String,
    /// When the server stored it, in its ISO 8601 spelling (`2026-10-20T14:12:48.587Z`).
    pub ts: String,
}

/// The admin account, logged in. Its token is its own: every `Admin` is a new login.
pub struct Admin {
    agent: ureq::Agent,
    base: String,
    /// The user id, which messages carry as their author's.
    pub user_id: String,
    token: String,
    pub username: String,
    /// The display name, which is what a direct message with the admin is called.
    pub name: Option<String>,
}

impl Admin {
    /// Logs in as `RC_ADMIN_USER` / `RC_ADMIN_PASS` (`admin` / `admin` if unset).
    ///
    /// # Panics
    ///
    /// If the server is unset, unreachable or refuses the login.
    #[must_use]
    pub fn login() -> Self {
        let username = std::env::var("RC_ADMIN_USER").unwrap_or_else(|_| "admin".into());
        let password = std::env::var("RC_ADMIN_PASS").unwrap_or_else(|_| "admin".into());
        let mut admin = Self {
            // Error statuses are answered like any other, so that `post` can show the body.
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .http_status_as_error(false)
                .build()
                .into(),
            base: format!("{}/api/v1", server_url().trim_end_matches('/')),
            user_id: String::new(),
            token: String::new(),
            username,
            name: None,
        };
        let json = admin.send(
            "login",
            &json!({ "user": admin.username, "password": password }),
            false,
        );
        let data = &json["data"];
        data["userId"]
            .as_str()
            .unwrap()
            .clone_into(&mut admin.user_id);
        data["authToken"]
            .as_str()
            .unwrap()
            .clone_into(&mut admin.token);
        admin.name = data["me"]["name"].as_str().map(str::to_owned);
        admin
    }

    /// A new user, with a unique username, who can log in at once.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn create_user(&self) -> TestUser {
        let suffix = unique();
        let username = format!("ct-user-{suffix}");
        let password = format!("pw-{suffix}");
        let name = format!("Contract {suffix}");
        let json = self.post(
            "users.create",
            &json!({
                "email": format!("{username}@example.com"),
                "name": name,
                "username": username,
                "password": password,
                "verified": true,
            }),
        );
        TestUser {
            id: json["user"]["_id"].as_str().unwrap().to_owned(),
            username,
            password,
            name,
        }
    }

    /// A new public channel with `members` (admin is a member too, as its creator). Returns its id
    /// and name.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn create_channel(&self, members: &[&str]) -> (String, String) {
        let name = format!("ct-chan-{}", unique());
        let json = self.post(
            "channels.create",
            &json!({ "name": name, "members": members }),
        );
        (json["channel"]["_id"].as_str().unwrap().to_owned(), name)
    }

    /// A new private group with `members`. Returns its id and name.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn create_group(&self, members: &[&str]) -> (String, String) {
        let name = format!("ct-priv-{}", unique());
        let json = self.post(
            "groups.create",
            &json!({ "name": name, "members": members }),
        );
        (json["group"]["_id"].as_str().unwrap().to_owned(), name)
    }

    /// The direct message between admin and `username`. Returns its id.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn create_dm(&self, username: &str) -> String {
        let json = self.post("im.create", &json!({ "username": username }));
        json["room"]["_id"].as_str().unwrap().to_owned()
    }

    /// Posts `text` to the room as admin and returns the message as the server stored it.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    // Most callers only need the message to exist, so the result may be ignored.
    #[allow(clippy::must_use_candidate)]
    pub fn post_message(&self, room_id: &str, text: &str) -> Posted {
        let json = self.post(
            "chat.postMessage",
            &json!({ "roomId": room_id, "text": text }),
        );
        Posted {
            id: json["message"]["_id"].as_str().unwrap().to_owned(),
            ts: json["message"]["ts"].as_str().unwrap().to_owned(),
        }
    }

    /// Sets the topic of a channel, which adds a system message to it.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn set_topic(&self, room_id: &str, topic: &str) {
        self.post(
            "channels.setTopic",
            &json!({ "roomId": room_id, "topic": topic }),
        );
    }

    /// Adds the user to a channel, which adds a system message to it.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn invite(&self, room_id: &str, user_id: &str) {
        self.post(
            "channels.invite",
            &json!({ "roomId": room_id, "userId": user_id }),
        );
    }

    /// Removes the user from a channel, which adds a system message to it.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn kick(&self, room_id: &str, user_id: &str) {
        self.post(
            "channels.kick",
            &json!({ "roomId": room_id, "userId": user_id }),
        );
    }

    /// The newest message of a channel, whatever its kind: how a test finds the id of a system
    /// message.
    ///
    /// # Panics
    ///
    /// If the server refuses or the channel has no messages.
    #[must_use]
    pub fn last_message(&self, channel_id: &str) -> Posted {
        let json = self.get(&format!("channels.history?roomId={channel_id}&count=1"));
        let message = &json["messages"][0];
        Posted {
            id: message["_id"].as_str().unwrap().to_owned(),
            ts: message["ts"].as_str().unwrap().to_owned(),
        }
    }

    /// Adds a custom emoji called `name` (letters, digits and underscores), a one-pixel PNG, which
    /// the server then lists and serves.
    ///
    /// # Panics
    ///
    /// If the server refuses, for example because the name is taken.
    pub fn create_emoji(&self, name: &str) {
        const BOUNDARY: &str = "----rc-test-server-emoji";
        let mut body = Vec::new();
        let field = |body: &mut Vec<u8>, key: &str, value: &str| {
            body.extend(
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n"
                )
                .bytes(),
            );
        };
        field(&mut body, "name", name);
        body.extend(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"emoji\"; \
                 filename=\"emoji.png\"\r\nContent-Type: image/png\r\n\r\n"
            )
            .bytes(),
        );
        body.extend(PIXEL_PNG);
        body.extend(format!("\r\n--{BOUNDARY}--\r\n").bytes());
        let mut response = self
            .agent
            .post(format!("{}/emoji-custom.create", self.base))
            .header("X-User-Id", &self.user_id)
            .header("X-Auth-Token", &self.token)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .send(&body)
            .unwrap();
        let status = response.status();
        let text = response.body_mut().read_to_string().unwrap();
        assert!(
            status.is_success() && text.contains("\"success\":true"),
            "emoji-custom.create: {status} {text}"
        );
    }

    /// GETs an endpoint (with its query) and returns the JSON, which must say `success`.
    fn get(&self, endpoint: &str) -> Value {
        let mut response = self
            .agent
            .get(format!("{}/{endpoint}", self.base))
            .header("X-User-Id", &self.user_id)
            .header("X-Auth-Token", &self.token)
            .call()
            .unwrap();
        let status = response.status();
        let text = response.body_mut().read_to_string().unwrap();
        let json: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{endpoint}: {status} {text:?}: {e}"));
        assert!(
            status.is_success() && json["success"] == true,
            "{endpoint}: {status} {json}"
        );
        json
    }

    /// POSTs to an endpoint and returns the JSON, which must say `success`.
    fn post(&self, endpoint: &str, body: &Value) -> Value {
        self.send(endpoint, body, true)
    }

    fn send(&self, endpoint: &str, body: &Value, authenticated: bool) -> Value {
        let mut request = self
            .agent
            .post(format!("{}/{endpoint}", self.base))
            .header("Content-Type", "application/json");
        if authenticated {
            request = request
                .header("X-User-Id", &self.user_id)
                .header("X-Auth-Token", &self.token);
        }
        let mut response = request.send(body.to_string()).unwrap();
        let status = response.status();
        let text = response.body_mut().read_to_string().unwrap();
        let json: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{endpoint}: {status} {text:?}: {e}"));
        assert!(
            status.is_success() && (json["success"] == true || json["status"] == "success"),
            "{endpoint}: {status} {json}"
        );
        json
    }
}
