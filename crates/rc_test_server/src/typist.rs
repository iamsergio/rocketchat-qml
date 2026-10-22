// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! [`Typist`]: another client whose user types.

use std::net::TcpStream;

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

use crate::{Admin, TIMEOUT, TestUser, server_url};

/// A DDP connection logged in as a test user, which says that the user starts and stops typing in
/// a room or a thread, the way the web client does: with the `stream-notify-room` method on
/// `<room id>/user-activity`. Everyone who follows the room hears it, except the typist's own
/// connection. The server refuses it under another user's name, so the typist is the user it was
/// made for. Closes when dropped.
pub struct Typist {
    ws: WebSocket<TcpStream>,
    username: String,
    next_id: u32,
}

impl Typist {
    /// Connects and logs in as `user`.
    ///
    /// # Panics
    ///
    /// If the server is unset or unreachable, or refuses the login.
    #[must_use]
    pub fn connect(user: &TestUser) -> Self {
        let url = server_url();
        let authority = url
            .trim_end_matches('/')
            .strip_prefix("http://")
            .unwrap_or_else(|| panic!("{url}: a Typist needs a plain http:// server URL"));
        let stream = TcpStream::connect(authority).unwrap();
        stream.set_read_timeout(Some(TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(TIMEOUT)).unwrap();
        let (ws, _) = tungstenite::client(format!("ws://{authority}/websocket"), stream).unwrap();
        let mut typist = Self {
            ws,
            username: user.username.clone(),
            next_id: 0,
        };
        typist.send(&json!({ "msg": "connect", "version": "1", "support": ["1"] }));
        typist.receive_until(|frame| frame["msg"] == "connected");
        // The token of a REST login, as the app's own connection logs in.
        let token = Admin::login_as(&user.username, &user.password).token;
        typist.call("login", &json!([{ "resume": token }]));
        typist
    }

    /// The user starts typing in the room.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn start(&mut self, room_id: &str) {
        self.notify(room_id, true, &json!({}));
    }

    /// The user starts typing in the thread whose parent message is `thread_id`.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn start_in_thread(&mut self, room_id: &str, thread_id: &str) {
        self.notify(room_id, true, &json!({ "tmid": thread_id }));
    }

    /// The user stops typing in the room.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn stop(&mut self, room_id: &str) {
        self.notify(room_id, false, &json!({}));
    }

    /// The user stops typing in the thread whose parent message is `thread_id`.
    ///
    /// # Panics
    ///
    /// If the server refuses.
    pub fn stop_in_thread(&mut self, room_id: &str, thread_id: &str) {
        self.notify(room_id, false, &json!({ "tmid": thread_id }));
    }

    fn notify(&mut self, room_id: &str, typing: bool, extras: &Value) {
        let activities = if typing {
            json!(["user-typing"])
        } else {
            json!([])
        };
        let params = json!([
            format!("{room_id}/user-activity"),
            self.username,
            activities,
            extras
        ]);
        self.call("stream-notify-room", &params);
    }

    /// Calls a method and waits for its answer.
    fn call(&mut self, method: &str, params: &Value) {
        self.next_id += 1;
        let id = self.next_id.to_string();
        self.send(&json!({ "msg": "method", "method": method, "params": params, "id": id }));
        let result = self.receive_until(|frame| frame["msg"] == "result" && frame["id"] == id);
        assert!(result.get("error").is_none(), "{method}: {result}");
    }

    fn send(&mut self, frame: &Value) {
        self.ws.send(Message::text(frame.to_string())).unwrap();
    }

    /// Reads frames, answering the server's pings, up to the first that `wanted` accepts.
    fn receive_until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        loop {
            let Message::Text(text) = self.ws.read().unwrap() else {
                continue;
            };
            let frame: Value = serde_json::from_str(text.as_str()).unwrap();
            if frame["msg"] == "ping" {
                self.send(&json!({ "msg": "pong" }));
            } else if wanted(&frame) {
                return frame;
            }
        }
    }
}

impl Drop for Typist {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
    }
}
