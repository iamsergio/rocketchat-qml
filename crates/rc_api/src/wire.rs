// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The JSON as Rocket.Chat sends it. Private to the crate: everything is converted into the public
//! types. Unknown fields are ignored and optional ones default, so newer servers keep working.

use secrecy::SecretString;
use serde::Deserialize;

use crate::types::{
    Author, Credentials, HistoryPage, LoginResult, Message, RoomType, ServerInfo, ServerVersion,
    Subscription, User,
};

#[derive(Deserialize)]
struct InfoBody {
    version: String,
}

#[derive(Deserialize)]
struct LoginBody {
    data: LoginData,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginData {
    user_id: String,
    auth_token: String,
    me: UserBody,
}

#[derive(Deserialize)]
struct UserBody {
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct SubscriptionsBody {
    #[serde(default)]
    update: Vec<SubscriptionBody>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionBody {
    rid: String,
    t: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    fname: Option<String>,
    #[serde(default)]
    unread: u32,
    #[serde(default)]
    user_mentions: u32,
    #[serde(default)]
    group_mentions: u32,
    #[serde(default)]
    alert: bool,
    #[serde(default)]
    open: bool,
}

#[derive(Deserialize)]
struct HistoryBody {
    #[serde(default)]
    messages: Vec<MessageBody>,
}

#[derive(Deserialize)]
struct MessageBody {
    #[serde(rename = "_id")]
    id: String,
    rid: String,
    ts: jiff::Timestamp,
    u: AuthorBody,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    tmid: Option<String>,
    #[serde(default)]
    tshow: bool,
}

#[derive(Deserialize)]
struct AuthorBody {
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: Option<String>,
}

/// The error body of failed calls: `message` is the human-readable one, `error` the short one.
#[derive(Deserialize)]
pub(crate) struct ErrorBody {
    message: Option<String>,
    error: Option<String>,
}

impl ErrorBody {
    pub(crate) fn into_message(self) -> Option<String> {
        self.message.or(self.error).filter(|m| !m.is_empty())
    }
}

impl From<UserBody> for User {
    fn from(body: UserBody) -> Self {
        Self {
            id: body.id,
            username: body.username,
            name: body.name,
        }
    }
}

impl From<SubscriptionBody> for Subscription {
    fn from(body: SubscriptionBody) -> Self {
        let room_type = match body.t.as_str() {
            "c" => RoomType::Channel,
            "p" => RoomType::Private,
            "d" => RoomType::Direct,
            _ => RoomType::Other(body.t),
        };
        Self {
            room_id: body.rid,
            room_type,
            name: body.name,
            display_name: body.fname,
            unread: body.unread,
            user_mentions: body.user_mentions,
            group_mentions: body.group_mentions,
            alert: body.alert,
            open: body.open,
        }
    }
}

impl From<MessageBody> for Message {
    fn from(body: MessageBody) -> Self {
        Self {
            id: body.id,
            room_id: body.rid,
            ts: body.ts,
            author: Author {
                id: body.u.id,
                username: body.u.username,
                name: body.u.name,
            },
            text: body.msg,
            kind: body.t,
            thread_id: body.tmid,
            thread_shown: body.tshow,
        }
    }
}

/// `None` if the body isn't JSON with a parsable `version`.
pub(crate) fn parse_info(bytes: &[u8]) -> Option<ServerInfo> {
    let body: InfoBody = serde_json::from_slice(bytes).ok()?;
    Some(ServerInfo {
        version: ServerVersion::parse(&body.version)?,
    })
}

pub(crate) fn parse_login(bytes: &[u8]) -> Result<LoginResult, serde_json::Error> {
    let data = serde_json::from_slice::<LoginBody>(bytes)?.data;
    Ok(LoginResult {
        credentials: Credentials {
            user_id: data.user_id,
            token: SecretString::from(data.auth_token),
        },
        user: data.me.into(),
    })
}

pub(crate) fn parse_user(bytes: &[u8]) -> Result<User, serde_json::Error> {
    Ok(serde_json::from_slice::<UserBody>(bytes)?.into())
}

/// `remove` is ignored: it only matters with `updatedSince`.
pub(crate) fn parse_subscriptions(bytes: &[u8]) -> Result<Vec<Subscription>, serde_json::Error> {
    let body: SubscriptionsBody = serde_json::from_slice(bytes)?;
    Ok(body.update.into_iter().map(Subscription::from).collect())
}

/// `count` is what was asked for: fewer messages than that means the history has ended.
pub(crate) fn parse_history(bytes: &[u8], count: usize) -> Result<HistoryPage, serde_json::Error> {
    let body: HistoryBody = serde_json::from_slice(bytes)?;
    Ok(HistoryPage {
        reached_start: body.messages.len() < count,
        messages: body.messages.into_iter().map(Message::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    const INFO: &str = include_str!("../tests/fixtures/info.json");
    const LOGIN: &str = include_str!("../tests/fixtures/login.json");
    const ME: &str = include_str!("../tests/fixtures/me.json");
    const SUBSCRIPTIONS: &str = include_str!("../tests/fixtures/subscriptions.json");
    const HISTORY: &str = include_str!("../tests/fixtures/history.json");

    // AUTH-003
    #[test]
    fn parses_info() {
        let info = parse_info(INFO.as_bytes()).unwrap();
        assert_eq!(info.version, ServerVersion { major: 8, minor: 0 });
        assert!(parse_info(b"<html></html>").is_none());
        assert!(parse_info(br#"{"version":"x"}"#).is_none());
    }

    // AUTH-001
    #[test]
    fn parses_login() {
        let result = parse_login(LOGIN.as_bytes()).unwrap();
        assert_eq!(result.credentials.user_id, "6ad649cecaea8883e7e075ec");
        assert_eq!(
            result.credentials.token.expose_secret(),
            "fake-auth-token-0123456789abcdefghijklmnopq"
        );
        assert_eq!(result.user.id, "6ad649cecaea8883e7e075ec");
        assert_eq!(result.user.username, "admin");
        assert_eq!(result.user.name.as_deref(), Some("Administrator"));
    }

    // AUTH-001
    #[test]
    fn credentials_debug_hides_the_token() {
        let result = parse_login(LOGIN.as_bytes()).unwrap();
        let debug = format!("{:?}", result.credentials);
        assert!(!debug.contains("fake-auth-token"), "{debug}");
    }

    // AUTH-004
    #[test]
    fn parses_me() {
        let user = parse_user(ME.as_bytes()).unwrap();
        assert_eq!(user.id, "u8paxsGQXMH9vSnSc");
        assert_eq!(user.username, "fx9300");
        assert_eq!(user.name.as_deref(), Some("Fixture User"));
    }

    // ROOM-002
    #[test]
    fn parses_subscription_names_and_types() {
        let rooms = parse_subscriptions(SUBSCRIPTIONS.as_bytes()).unwrap();
        assert_eq!(rooms.len(), 4);

        assert_eq!(rooms[0].room_id, "GENERAL");
        assert_eq!(rooms[0].room_type, RoomType::Channel);
        assert_eq!(rooms[0].name, "general");
        assert_eq!(rooms[0].display_name, None);

        assert_eq!(rooms[1].room_type, RoomType::Channel);
        assert_eq!(rooms[1].name, "fx9300-chan");
        assert_eq!(rooms[1].display_name.as_deref(), Some("fx9300-chan"));

        assert_eq!(rooms[2].room_type, RoomType::Private);
        assert_eq!(rooms[2].name, "fx9300-priv");

        assert_eq!(rooms[3].room_type, RoomType::Direct);
        assert_eq!(rooms[3].name, "admin");
        assert_eq!(rooms[3].display_name.as_deref(), Some("Administrator"));
    }

    // ROOM-003
    #[test]
    fn parses_subscription_counts() {
        let rooms = parse_subscriptions(SUBSCRIPTIONS.as_bytes()).unwrap();
        let counts: Vec<_> = rooms
            .iter()
            .map(|r| (r.unread, r.user_mentions, r.group_mentions, r.alert, r.open))
            .collect();
        assert_eq!(
            counts,
            [
                (1, 1, 0, true, true),
                (1, 1, 0, true, true),
                (0, 0, 0, false, true),
                (1, 0, 0, true, true),
            ]
        );
    }

    // ROOM-003
    #[test]
    fn missing_subscription_fields_default() {
        let rooms = parse_subscriptions(br#"{"update":[{"rid":"r1","t":"l"}]}"#).unwrap();
        assert_eq!(
            rooms,
            [Subscription {
                room_id: "r1".into(),
                room_type: RoomType::Other("l".into()),
                name: String::new(),
                display_name: None,
                unread: 0,
                user_mentions: 0,
                group_mentions: 0,
                alert: false,
                open: false,
            }]
        );
    }

    // ROOM-001
    #[test]
    fn empty_update_is_no_rooms() {
        assert!(parse_subscriptions(br#"{"remove":[]}"#).unwrap().is_empty());
    }

    fn ts(text: &str) -> jiff::Timestamp {
        text.parse().unwrap()
    }

    // MSG-001, MSG-003
    #[test]
    fn parses_messages_with_author_and_time() {
        let page = parse_history(HISTORY.as_bytes(), 50).unwrap();
        // Newest first, as the server sends them.
        let ids: Vec<_> = page.messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "vWJvPiA5seYwr9Exh",
                "PgCoLuN95ZJTxpBTQ",
                "6ad776eb3a2871a71eddd4c9",
                "6ad776eb3a2871a71eddd4c5"
            ]
        );
        let parent = &page.messages[1];
        assert_eq!(parent.room_id, "6ad776f43a2871a71eddd4ce");
        assert_eq!(parent.ts, ts("2026-10-20T14:13:08.207Z"));
        assert_eq!(parent.text, "parent");
        assert_eq!(
            parent.author,
            Author {
                id: "6ad72ad3b6de6b54ca599f26".into(),
                username: "admin".into(),
                name: Some("Administrator".into()),
            }
        );
        assert_eq!(parent.kind, None);
        assert_eq!(parent.thread_id, None);
        assert!(!parent.thread_shown);
    }

    // MSG-005
    #[test]
    fn parses_system_messages() {
        let page = parse_history(HISTORY.as_bytes(), 50).unwrap();
        let added = &page.messages[2];
        assert_eq!(added.kind.as_deref(), Some("au"));
        assert_eq!(added.text, "probe2502");
        let topic = &page.messages[3];
        assert_eq!(topic.kind.as_deref(), Some("room_changed_topic"));
        assert_eq!(topic.text, "Hello topic");
    }

    // MSG-001
    #[test]
    fn parses_thread_replies() {
        let page = parse_history(HISTORY.as_bytes(), 50).unwrap();
        let reply = &page.messages[0];
        assert_eq!(reply.thread_id.as_deref(), Some("PgCoLuN95ZJTxpBTQ"));
        assert!(reply.thread_shown);
    }

    // MSG-001
    #[test]
    fn missing_message_fields_default() {
        let json = br#"{"messages":[{"_id":"m","rid":"r","ts":"2026-10-20T14:00:00.000Z","u":{"_id":"u"}}]}"#;
        let page = parse_history(json, 50).unwrap();
        let message = &page.messages[0];
        assert_eq!(message.text, "");
        assert_eq!(message.kind, None);
        assert_eq!(message.thread_id, None);
        assert!(!message.thread_shown);
        assert_eq!(message.author.username, "");
        assert_eq!(message.author.name, None);
    }

    // MSG-001
    #[test]
    fn a_message_without_a_valid_time_is_a_decode_error() {
        let json = br#"{"messages":[{"_id":"m","rid":"r","ts":"yesterday","u":{"_id":"u"}}]}"#;
        assert!(parse_history(json, 50).is_err());
    }

    // MSG-002
    #[test]
    fn fewer_messages_than_asked_for_is_the_start() {
        assert!(parse_history(HISTORY.as_bytes(), 5).unwrap().reached_start);
        assert!(!parse_history(HISTORY.as_bytes(), 4).unwrap().reached_start);
        assert!(!parse_history(HISTORY.as_bytes(), 3).unwrap().reached_start);
        assert!(
            parse_history(br#"{"success":true}"#, 50)
                .unwrap()
                .reached_start
        );
    }
}
