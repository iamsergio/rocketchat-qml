// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Test support: creates users, rooms and messages on the local Rocket.Chat test server
//! (`scripts/run_test_server.sh`), named by `RC_TEST_SERVER_URL`, through its admin API. The
//! contract tests and the GUI tests share it.
//!
//! Everything is named with [`unique`], so tests hold on a dirty server, in any order and in
//! parallel. Calls block: they only stand in for what a person would do in another client.

#![forbid(unsafe_code)]
// Test support: a failed call should just fail the test.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

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

/// A user made by [`Admin::create_user`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestUser {
    pub id: String,
    pub username: String,
    pub password: String,
    /// The display name.
    pub name: String,
}

/// The admin account, logged in. Its token is its own: every `Admin` is a new login.
pub struct Admin {
    agent: ureq::Agent,
    base: String,
    user_id: String,
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

    /// Posts `text` to the room as admin.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn post_message(&self, room_id: &str, text: &str) {
        self.post(
            "chat.postMessage",
            &json!({ "roomId": room_id, "text": text }),
        );
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
