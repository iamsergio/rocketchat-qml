// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Test support: creates users, rooms and messages on the local Rocket.Chat test server
//! (`scripts/run_test_server.sh`), named by `RC_TEST_SERVER_URL`, through its admin API. The
//! contract tests and the GUI tests share it.
//!
//! Everything is named with [`unique`], so tests hold on a dirty server, in any order and in
//! parallel. Calls block: they only stand in for what a person would do in another client.
//!
//! [`Proxy`] sits between a client and the server, to cut the connections. [`Typist`] is another
//! client whose user types.

#![forbid(unsafe_code)]
// Test support: a failed call should just fail the test.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

mod proxy;
mod typist;

use std::process;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

pub use proxy::Proxy;
pub use typist::Typist;

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

/// A file named `name` with `bytes` in a folder of its own under the system's temporary one, for
/// the tests that upload. The folder is not removed: the system's cleaning does that.
///
/// # Panics
///
/// If the file can't be written.
#[must_use]
pub fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rc_test_{}", unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::write(&file, bytes).unwrap();
    file
}

/// The image of the custom emoji [`Admin::create_emoji`] makes, the avatar [`Admin::set_avatar`]
/// sets and the file the tests upload with [`Admin::upload_file`]: one pixel. Too small for the
/// server to make a thumbnail of it.
pub const PIXEL_PNG: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The avatar [`Admin::set_quadrant_room_avatar`] sets: 64x64 with one flat colour filling each of
/// its four quadrants, where [`QUADRANTS`] says which colour is where. The server re-encodes an
/// uploaded avatar (as a 200x200 PNG), which keeps the four colours and their places, so a pixel of
/// the drawn avatar says where in the image it comes from: a test can tell a whole image scaled
/// into the box that shows it from a window onto one corner of it.
pub const QUADRANTS_PNG: [u8; 164] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x08, 0x06, 0x00, 0x00, 0x00, 0xaa, 0x69, 0x71,
    0xde, 0x00, 0x00, 0x00, 0x6b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0xed, 0xd0, 0x81, 0x0d, 0x00,
    0x20, 0x0c, 0x02, 0x30, 0xfe, 0x7f, 0x7a, 0xfa, 0x05, 0xc6, 0x15, 0xc2, 0x01, 0x34, 0x93, 0x4c,
    0x73, 0x0f, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xf8, 0x1e, 0xa0, 0xfc, 0xff, 0x26, 0xd5, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x0e,
    0xbe, 0x2e, 0xde, 0x0e, 0x63, 0xb8, 0x48, 0xd9, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

/// Where the colour of each quadrant of [`QUADRANTS_PNG`] is: (x, y) as fractions of the image,
/// and the colour as red, green and blue. Reading the pixel of a drawn avatar at these fractions
/// says which corner of the image came out of the box.
pub const QUADRANTS: [(f64, f64, [u8; 3]); 4] = [
    (0.25, 0.25, [255, 0, 0]),
    (0.75, 0.25, [0, 255, 0]),
    (0.25, 0.75, [0, 0, 255]),
    (0.75, 0.75, [255, 255, 0]),
];

/// `bytes` as standard base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let [a, b, c] = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let group = (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c);
        for (at, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if at <= chunk.len() {
                out.push(char::from(ALPHABET[((group >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    // RICH-004
    #[test]
    fn base64_pads_and_encodes_like_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}

/// A message made by [`Admin::post_message`], or found by [`Admin::last_message`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    pub id: String,
    /// When the server stored it, in its ISO 8601 spelling (`2026-10-20T14:12:48.587Z`).
    pub ts: String,
}

/// The admin account, logged in. The admins of a process share one login (see [`Admin::login`]);
/// [`Admin::login_as`] is a new one.
#[derive(Clone)]
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
    /// Logs in as `RC_ADMIN_USER` / `RC_ADMIN_PASS` (`admin` / `admin` if unset), once per
    /// process: later calls share that login. The server keeps only a user's 50 newest logins
    /// (`MAX_RESUME_LOGIN_TOKENS`) and forgets older ones, so tests that each logged in, in
    /// parallel and in other runs on the same server, would log each other out.
    ///
    /// # Panics
    ///
    /// If the server is unset, unreachable or refuses the login.
    #[must_use]
    pub fn login() -> Self {
        static SHARED: OnceLock<Admin> = OnceLock::new();
        SHARED
            .get_or_init(|| {
                let username = std::env::var("RC_ADMIN_USER").unwrap_or_else(|_| "admin".into());
                let password = std::env::var("RC_ADMIN_PASS").unwrap_or_else(|_| "admin".into());
                Self::login_as(&username, &password)
            })
            .clone()
    }

    /// Logs in as any user, to act as them: the calls work as far as the user's roles allow, so
    /// for a test user that is reacting to a message in a room they are in, for one.
    ///
    /// # Panics
    ///
    /// If the server is unset, unreachable or refuses the login.
    #[must_use]
    pub fn login_as(username: &str, password: &str) -> Self {
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
            username: username.to_owned(),
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
        self.create_user_named(&format!("Contract {}", unique()))
    }

    /// A new user with a unique username, who can log in at once, under the display name `name`.
    /// Tests that care about how a name is laid out need a name that doesn't change from run to
    /// run.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn create_user_named(&self, name: &str) -> TestUser {
        let suffix = unique();
        let username = format!("ct-user-{suffix}");
        let password = format!("pw-{suffix}");
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
            name: name.to_owned(),
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
        self.create_channel_named(&format!("ct-chan-{}", unique()), members)
    }

    /// [`create_channel`](Self::create_channel) under a name of the caller's, which must be
    /// unique: tests that search the directory give their channels a name they can look for.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn create_channel_named(&self, name: &str, members: &[&str]) -> (String, String) {
        let json = self.post(
            "channels.create",
            &json!({ "name": name, "members": members }),
        );
        (
            json["channel"]["_id"].as_str().unwrap().to_owned(),
            name.to_owned(),
        )
    }

    /// A new private group with `members`. Returns its id and name. Its creator, the user this
    /// `Admin` is logged in as, is its owner, and the only one.
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

    /// Closes the direct message for the user this `Admin` is logged in as: it leaves their room
    /// list (the subscription stays, not open) until a message arrives or they open it again.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn close_dm(&self, room_id: &str) {
        self.post("im.close", &json!({ "roomId": room_id }));
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

    /// Posts `text` as a reply in the thread of `thread_id`, and in the channel too if
    /// `also_to_channel` (`chat.sendMessage` with `tmid` and `tshow`). The first reply makes the
    /// message a thread's parent.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    // Most callers only need the message to exist, so the result may be ignored.
    #[allow(clippy::must_use_candidate)]
    pub fn post_in_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        text: &str,
        also_to_channel: bool,
    ) -> Posted {
        let json = self.post(
            "chat.sendMessage",
            &json!({ "message": {
                "rid": room_id, "msg": text, "tmid": thread_id, "tshow": also_to_channel,
            } }),
        );
        Posted {
            id: json["message"]["_id"].as_str().unwrap().to_owned(),
            ts: json["message"]["ts"].as_str().unwrap().to_owned(),
        }
    }

    /// The server's own URL (`Site_Url`), which the links of quotes must begin with: the server
    /// makes no quote from a link to it by any other name (`localhost` for `127.0.0.1`, the
    /// container's name in CI).
    ///
    /// # Panics
    ///
    /// If the server refuses or has no such setting.
    #[must_use]
    pub fn site_url(&self) -> String {
        let json = self.get("settings.public?_id=Site_Url");
        let url = json["settings"][0]["value"].as_str().unwrap();
        url.trim_end_matches('/').to_owned()
    }

    /// Posts `text` as a reply that quotes a message: the text starts with an empty-label link to
    /// the quoted message's permalink. `room_path` is the room's part of the permalink, such as
    /// `channel/<name>`.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    // Most callers only need the message to exist, so the result may be ignored.
    #[allow(clippy::must_use_candidate)]
    pub fn post_quote(
        &self,
        room_id: &str,
        room_path: &str,
        quoted_id: &str,
        text: &str,
    ) -> Posted {
        let link = format!("{}/{room_path}?msg={quoted_id}", self.site_url());
        self.post_message(room_id, &format!("[ ]({link}) {text}"))
    }

    /// Edits the text of a message (`chat.update`). The server pushes the whole message again,
    /// with `editedAt`.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn edit_message(&self, room_id: &str, message_id: &str, text: &str) {
        self.post(
            "chat.update",
            &json!({ "roomId": room_id, "msgId": message_id, "text": text }),
        );
    }

    /// Deletes a message (`chat.delete`). The server announces it on the room's `deleteMessage`
    /// stream only.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn delete_message(&self, room_id: &str, message_id: &str) {
        self.post(
            "chat.delete",
            &json!({ "roomId": room_id, "msgId": message_id }),
        );
    }

    /// Adds this user's reaction to a message, or takes it back (`chat.react`). `emoji` is the
    /// shortcode, with or without its colons.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn react(&self, message_id: &str, emoji: &str, should_react: bool) {
        self.post(
            "chat.react",
            &json!({ "messageId": message_id, "emoji": emoji, "shouldReact": should_react }),
        );
    }

    /// Makes this user follow the thread of a parent message, or stop following it
    /// (`chat.followMessage`, `chat.unfollowMessage`).
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn follow_message(&self, message_id: &str, follow: bool) {
        let endpoint = if follow {
            "chat.followMessage"
        } else {
            "chat.unfollowMessage"
        };
        self.post(endpoint, &json!({ "mid": message_id }));
    }

    /// Ends every login of this user but this one's (`users.logoutOtherClients`): a client that was
    /// logged in as them finds its token refused, as when a session expires.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn logout_other_clients(&self) {
        self.post("users.logoutOtherClients", &json!({}));
    }

    /// Uploads a file to the room as admin and posts it as a message with the text `msg` (which
    /// may be empty) and the description `description`, the way the clients do: `rooms.media`, then
    /// `rooms.mediaConfirm`. Returns the message as the server stored it.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    // Most callers only need the message to exist, so the result may be ignored.
    #[allow(clippy::must_use_candidate)]
    pub fn upload_file(
        &self,
        room_id: &str,
        file_name: &str,
        content_type: &str,
        bytes: &[u8],
        msg: &str,
        description: &str,
    ) -> Posted {
        let uploaded = self.multipart(
            &format!("rooms.media/{room_id}"),
            &[],
            ("file", file_name, content_type, bytes),
        );
        let file_id = uploaded["file"]["_id"].as_str().unwrap();
        let json = self.post(
            &format!("rooms.mediaConfirm/{room_id}/{file_id}"),
            &json!({ "msg": msg, "description": description }),
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

    /// Marks the room as a favorite of the user this `Admin` is logged in as, or takes it back.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn set_favorite(&self, room_id: &str, favorite: bool) {
        self.post(
            "rooms.favorite",
            &json!({ "roomId": room_id, "favorite": favorite }),
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

    /// The newest `count` messages of a channel, whatever their kind, the newest first.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn last_messages(&self, channel_id: &str, count: usize) -> Vec<Posted> {
        let json = self.get(&format!(
            "channels.history?roomId={channel_id}&count={count}"
        ));
        json["messages"]
            .as_array()
            .map(|messages| {
                messages
                    .iter()
                    .map(|message| Posted {
                        id: message["_id"].as_str().unwrap().to_owned(),
                        ts: message["ts"].as_str().unwrap().to_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The newest reply in the thread of `thread_id` (`chat.getThreadMessages`, newest first),
    /// or `None` if it has none. The channel's history does not list a reply unless it was also
    /// sent to the channel.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    #[must_use]
    pub fn last_in_thread(&self, thread_id: &str) -> Option<Posted> {
        let json = self.get(&format!(
            "chat.getThreadMessages?tmid={thread_id}&count=1&sort=%7B%22ts%22%3A-1%7D"
        ));
        let message = json["messages"].as_array()?.first()?;
        Some(Posted {
            id: message["_id"].as_str().unwrap().to_owned(),
            ts: message["ts"].as_str().unwrap().to_owned(),
        })
    }

    /// Adds a custom emoji called `name` (letters, digits and underscores), a one-pixel PNG, which
    /// the server then lists and serves.
    ///
    /// # Panics
    ///
    /// If the server refuses, for example because the name is taken.
    pub fn create_emoji(&self, name: &str) {
        self.upload(
            "emoji-custom.create",
            &[("name", name)],
            "emoji",
            "emoji.png",
        );
    }

    /// [`create_emoji`](Self::create_emoji) unless the server already lists an emoji of that name,
    /// for callers that want a fixed name across runs. Returns whether it was created.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    // Callers that don't care whether it was new may ignore the result.
    #[allow(clippy::must_use_candidate)]
    pub fn create_emoji_if_missing(&self, name: &str) -> bool {
        let listed = self.get("emoji-custom.list");
        let exists = listed["emojis"]["update"]
            .as_array()
            .is_some_and(|emojis| emojis.iter().any(|emoji| emoji["name"] == name));
        if !exists {
            self.create_emoji(name);
        }
        !exists
    }

    /// Sets the avatar of the user to a one-pixel PNG (`users.setAvatar`) and waits until the
    /// server says the user has one (`avatarETag` of `users.info`), which it does a moment later.
    /// The server announces it on its stream of avatar changes, and serves the image from
    /// `/avatar/<username>` to a request that carries the login as cookies.
    ///
    /// # Panics
    ///
    /// If the server refuses, or still says there is none after a while.
    pub fn set_avatar(&self, username: &str) {
        self.upload(
            "users.setAvatar",
            &[("username", username)],
            "image",
            "avatar.png",
        );
        self.wait_for_etag(&format!("users.info?username={username}"), "user");
    }

    /// Sets the avatar of the room to a one-pixel PNG (`rooms.saveRoomSettings`) and waits until
    /// the server says the room has one (`avatarETag` of `rooms.info`). The server serves the image
    /// from `/avatar/room/<id>` to a request that carries the login as cookies.
    ///
    /// # Panics
    ///
    /// If the server refuses, or still says there is none after a while.
    pub fn set_room_avatar(&self, room_id: &str) {
        self.set_room_avatar_image(room_id, &PIXEL_PNG);
    }

    /// [`set_room_avatar`](Self::set_room_avatar) with [`QUADRANTS_PNG`], the avatar a test reads
    /// pixels from to see how the app drew it.
    ///
    /// # Panics
    ///
    /// As [`set_room_avatar`](Self::set_room_avatar).
    pub fn set_quadrant_room_avatar(&self, room_id: &str) {
        self.set_room_avatar_image(room_id, &QUADRANTS_PNG);
    }

    /// [`set_room_avatar`](Self::set_room_avatar) with `png` as the room's avatar image.
    fn set_room_avatar_image(&self, room_id: &str, png: &[u8]) {
        let uri = format!("data:image/png;base64,{}", base64(png));
        self.post(
            "rooms.saveRoomSettings",
            &json!({ "rid": room_id, "roomAvatar": uri }),
        );
        self.wait_for_etag(&format!("rooms.info?roomId={room_id}"), "room");
    }

    fn wait_for_etag(&self, endpoint: &str, field: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.get(endpoint)[field]["avatarETag"].as_str().is_none() {
            assert!(
                Instant::now() < deadline,
                "{endpoint} never had an avatarETag"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// POSTs a multipart form with the text `fields` and the one-pixel PNG as the file `file`.
    fn upload(&self, endpoint: &str, fields: &[(&str, &str)], file: &str, filename: &str) {
        self.multipart(endpoint, fields, (file, filename, "image/png", &PIXEL_PNG));
    }

    /// POSTs a multipart form with the text `fields` and a file, given as the name of its field,
    /// its file name, its media type and its bytes. Returns the JSON, which must say `success`.
    fn multipart(
        &self,
        endpoint: &str,
        fields: &[(&str, &str)],
        (file, filename, content_type, bytes): (&str, &str, &str, &[u8]),
    ) -> Value {
        const BOUNDARY: &str = "----rc-test-server-upload";
        let mut body = Vec::new();
        for (key, value) in fields {
            body.extend(
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n"
                )
                .bytes(),
            );
        }
        body.extend(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{file}\"; \
                 filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
            )
            .bytes(),
        );
        body.extend(bytes);
        body.extend(format!("\r\n--{BOUNDARY}--\r\n").bytes());
        let mut response = self
            .agent
            .post(format!("{}/{endpoint}", self.base))
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
        let json: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{endpoint}: {status} {text:?}: {e}"));
        assert!(
            status.is_success() && json["success"] == true,
            "{endpoint}: {status} {text}"
        );
        json
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
