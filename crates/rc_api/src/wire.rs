// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The JSON as Rocket.Chat sends it. Private to the crate: everything is converted into the public
//! types. Unknown fields are ignored and optional ones default, so newer servers keep working.

use std::collections::HashMap;

use secrecy::SecretString;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use crate::md;
use crate::types::{
    Attachment, AttachmentImage, Author, AvatarOwner, Credentials, CustomEmoji, DdpEvent,
    HistoryPage, LoginResult, Message, RoomType, ServerInfo, ServerVersion, Subscription, User,
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
struct CustomEmojiBody {
    emojis: CustomEmojiList,
}

#[derive(Deserialize)]
struct CustomEmojiList {
    #[serde(default)]
    update: Vec<CustomEmojiEntry>,
}

#[derive(Deserialize)]
struct CustomEmojiEntry {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    extension: String,
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
    /// When the user joined.
    #[serde(default, deserialize_with = "optional_timestamp")]
    ts: Option<jiff::Timestamp>,
}

#[derive(Deserialize)]
struct RoomsBody {
    #[serde(default)]
    update: Vec<RoomBody>,
}

/// A room as `rooms.get` and the `rooms-changed` event send it: the parts that tell when it was
/// last active.
#[derive(Deserialize)]
struct RoomBody {
    #[serde(rename = "_id")]
    id: String,
    /// The time of the last message; absent in a room without messages.
    #[serde(default, deserialize_with = "optional_timestamp")]
    lm: Option<jiff::Timestamp>,
    /// Present once the room has an avatar of its own.
    #[serde(default, rename = "avatarETag")]
    avatar_etag: Option<String>,
}

/// The `user` of `users.info` and the `room` of `rooms.info`: whether it has an avatar of its own.
#[derive(Deserialize)]
struct AvatarOwnerBody {
    #[serde(default, rename = "avatarETag")]
    avatar_etag: Option<String>,
}

#[derive(Deserialize)]
struct UserInfoBody {
    user: AvatarOwnerBody,
}

#[derive(Deserialize)]
struct RoomInfoBody {
    room: AvatarOwnerBody,
}

#[derive(Deserialize)]
struct HistoryBody {
    #[serde(default)]
    messages: Vec<MessageBody>,
}

#[derive(Deserialize)]
struct SentBody {
    message: MessageBody,
}

#[derive(Deserialize)]
struct MessageBody {
    #[serde(rename = "_id")]
    id: String,
    rid: String,
    #[serde(deserialize_with = "timestamp")]
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
    #[serde(default)]
    md: Option<serde_json::Value>,
    /// Read entry by entry (see [`parse_attachments`]), so one odd entry doesn't fail the message.
    #[serde(default)]
    attachments: Option<serde_json::Value>,
}

/// A file attachment (`type` is `file`). The fields of images are `image_*`.
#[derive(Deserialize)]
struct AttachmentBody {
    #[serde(default)]
    title: String,
    title_link: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    image_url: Option<String>,
    #[serde(default)]
    image_type: Option<String>,
    #[serde(default)]
    image_dimensions: Option<DimensionsBody>,
}

#[derive(Deserialize)]
struct DimensionsBody {
    width: u32,
    height: u32,
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

/// A time as REST sends it (ISO 8601) or as DDP does (EJSON: `{"$date": <ms since the epoch>}`).
#[derive(Deserialize)]
#[serde(untagged)]
enum Time {
    Iso(jiff::Timestamp),
    Ejson {
        #[serde(rename = "$date")]
        date: i64,
    },
}

impl Time {
    fn into_timestamp<E: serde::de::Error>(self) -> Result<jiff::Timestamp, E> {
        match self {
            Self::Iso(ts) => Ok(ts),
            Self::Ejson { date } => jiff::Timestamp::from_millisecond(date).map_err(E::custom),
        }
    }
}

fn timestamp<'de, D: Deserializer<'de>>(deserializer: D) -> Result<jiff::Timestamp, D::Error> {
    Time::deserialize(deserializer)?.into_timestamp()
}

fn optional_timestamp<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<jiff::Timestamp>, D::Error> {
    Option::<Time>::deserialize(deserializer)?
        .map(Time::into_timestamp)
        .transpose()
}

/// The `fields` of a stream's `changed` frame: which event, and its arguments.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamFields {
    event_name: String,
    #[serde(default)]
    args: Vec<serde_json::Value>,
}

/// What `updateAvatar` carries: the username of the user, or the id of the room.
#[derive(Deserialize)]
struct AvatarChangeBody {
    username: Option<String>,
    rid: Option<String>,
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
            created: body.ts,
            last_message: None,
            avatar_etag: None,
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
            md: body.md.as_ref().and_then(md::parse),
            attachments: parse_attachments(body.attachments.as_ref()),
        }
    }
}

/// The file attachments of a message. Entries of another type, and entries that are malformed, are
/// skipped.
fn parse_attachments(value: Option<&serde_json::Value>) -> Vec<Attachment> {
    let Some(entries) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| entry.get("type").and_then(serde_json::Value::as_str) == Some("file"))
        .filter_map(|entry| AttachmentBody::deserialize(entry).ok())
        .map(|body| Attachment {
            title: body.title,
            link: body.title_link,
            description: body.description.filter(|text| !text.is_empty()),
            size: body.size,
            image: body.image_url.map(|path| AttachmentImage {
                path,
                content_type: body.image_type,
                dimensions: body.image_dimensions.map(|d| (d.width, d.height)),
            }),
        })
        .collect()
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

pub(crate) fn parse_custom_emoji(bytes: &[u8]) -> Result<Vec<CustomEmoji>, serde_json::Error> {
    let body: CustomEmojiBody = serde_json::from_slice(bytes)?;
    Ok(body
        .emojis
        .update
        .into_iter()
        .map(|entry| CustomEmoji {
            name: entry.name,
            aliases: entry.aliases,
            extension: entry.extension,
        })
        .collect())
}

/// The time of each room's last message, by room id, for the rooms that have messages.
pub(crate) fn parse_room_activity(
    bytes: &[u8],
) -> Result<HashMap<String, jiff::Timestamp>, serde_json::Error> {
    let body: RoomsBody = serde_json::from_slice(bytes)?;
    Ok(body
        .update
        .into_iter()
        .filter_map(|room| Some((room.id, room.lm?)))
        .collect())
}

/// The avatar etag of each room that has an avatar of its own, by room id.
pub(crate) fn parse_room_avatars(
    bytes: &[u8],
) -> Result<HashMap<String, String>, serde_json::Error> {
    let body: RoomsBody = serde_json::from_slice(bytes)?;
    Ok(body
        .update
        .into_iter()
        .filter_map(|room| Some((room.id, room.avatar_etag?)))
        .collect())
}

/// The avatar etag of a `users.info` answer: none for a user with the generated avatar.
pub(crate) fn parse_user_avatar_etag(bytes: &[u8]) -> Result<Option<String>, serde_json::Error> {
    Ok(serde_json::from_slice::<UserInfoBody>(bytes)?
        .user
        .avatar_etag)
}

/// The avatar etag of a `rooms.info` answer: none for a room with the generated avatar.
pub(crate) fn parse_room_avatar_etag(bytes: &[u8]) -> Result<Option<String>, serde_json::Error> {
    Ok(serde_json::from_slice::<RoomInfoBody>(bytes)?
        .room
        .avatar_etag)
}

/// `count` is what was asked for: fewer messages than that means the history has ended.
pub(crate) fn parse_history(bytes: &[u8], count: usize) -> Result<HistoryPage, serde_json::Error> {
    let body: HistoryBody = serde_json::from_slice(bytes)?;
    Ok(HistoryPage {
        reached_start: body.messages.len() < count,
        messages: body.messages.into_iter().map(Message::from).collect(),
    })
}

/// The answer of `chat.sendMessage` and `chat.getMessage`: the message as the server stored it.
pub(crate) fn parse_message(bytes: &[u8]) -> Result<Message, serde_json::Error> {
    Ok(serde_json::from_slice::<SentBody>(bytes)?.message.into())
}

/// The event in the `fields` of a `changed` frame of `collection`. `None` for the streams and
/// events this crate doesn't follow.
pub(crate) fn parse_stream_event(
    collection: &str,
    fields: serde_json::Value,
) -> Result<Option<DdpEvent>, serde_json::Error> {
    let fields: StreamFields = serde_json::from_value(fields)?;
    let mut args = fields.args.into_iter();
    let mut next_arg = || {
        args.next()
            .ok_or_else(|| serde_json::Error::custom("missing argument"))
    };
    match collection {
        // The message, then what the server says about the room.
        "stream-room-messages" => {
            let message: MessageBody = serde_json::from_value(next_arg()?)?;
            Ok(Some(DdpEvent::Message(message.into())))
        }
        // What happened, then the subscription. A removed one still comes whole.
        "stream-notify-user" if fields.event_name.ends_with("/subscriptions-changed") => {
            let action = next_arg()?;
            let body: SubscriptionBody = serde_json::from_value(next_arg()?)?;
            Ok(match action.as_str() {
                Some("inserted" | "updated") => Some(DdpEvent::SubscriptionChanged(body.into())),
                Some("removed") => Some(DdpEvent::SubscriptionRemoved { room_id: body.rid }),
                _ => None,
            })
        }
        // What happened, then the room. Rooms without a last message (just created) say nothing.
        "stream-notify-user" if fields.event_name.ends_with("/rooms-changed") => {
            next_arg()?;
            let room: RoomBody = serde_json::from_value(next_arg()?)?;
            Ok(room.lm.map(|last_message| DdpEvent::RoomActivity {
                room_id: room.id,
                last_message,
            }))
        }
        "stream-notify-logged" if fields.event_name == "updateAvatar" => {
            let body: AvatarChangeBody = serde_json::from_value(next_arg()?)?;
            Ok(match (body.username, body.rid) {
                (Some(username), _) => Some(AvatarOwner::User { username }),
                (None, Some(room_id)) => Some(AvatarOwner::Room { room_id }),
                (None, None) => None,
            }
            .map(DdpEvent::AvatarChanged))
        }
        _ => Ok(None),
    }
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

    const HISTORY_ATTACHMENTS: &str = include_str!("../tests/fixtures/history_attachments.json");
    const HISTORY_RICH: &str = include_str!("../tests/fixtures/history_rich.json");
    const EMOJI_CUSTOM: &str = include_str!("../tests/fixtures/emoji_custom.json");

    fn rich(text: &str) -> Message {
        parse_history(HISTORY_RICH.as_bytes(), 50)
            .unwrap()
            .messages
            .into_iter()
            .find(|m| m.text == text)
            .unwrap_or_else(|| panic!("no message {text:?} in the fixture"))
    }

    fn attached(id: &str) -> Message {
        parse_history(HISTORY_ATTACHMENTS.as_bytes(), 50)
            .unwrap()
            .messages
            .into_iter()
            .find(|m| m.id == id)
            .unwrap()
    }

    // RICH-005
    #[test]
    fn reads_an_image_attachment() {
        let message = attached("msgimage");
        assert_eq!(message.text, "hello **text**");
        assert_eq!(
            message.attachments,
            [Attachment {
                title: "big pic.png".into(),
                link: "/file-upload/6ad7f2c6bb/big%20pic.png".into(),
                description: None,
                size: None,
                image: Some(AttachmentImage {
                    path: "/file-upload/6ad7f2c6cc/big%20pic.png".into(),
                    content_type: Some("image/png".into()),
                    dimensions: Some((480, 360)),
                }),
            }]
        );
    }

    // RICH-005
    #[test]
    fn reads_a_file_attachment_without_an_image() {
        let message = attached("msgtext");
        assert_eq!(message.text, "");
        assert_eq!(
            message.attachments,
            [Attachment {
                title: "notes.txt".into(),
                link: "/file-upload/6ad7f2c6aa/notes.txt".into(),
                description: Some("my notes".into()),
                size: Some(6),
                image: None,
            }]
        );
    }

    // RICH-005
    #[test]
    fn attachments_that_are_not_files_are_left_out() {
        assert_eq!(attached("msgquote").attachments, []);
        assert_eq!(attached("msgtext").attachments.len(), 1);
    }

    // RICH-005
    #[test]
    fn a_malformed_attachment_is_skipped_and_the_message_stays() {
        let message = attached("msgmalformed");
        assert_eq!(message.text, "odd ones");
        let titles: Vec<_> = message
            .attachments
            .iter()
            .map(|a| a.title.as_str())
            .collect();
        assert_eq!(titles, ["ok.txt"]);
    }

    // RICH-005
    #[test]
    fn attachments_that_are_not_a_list_mean_none() {
        for attachments in ["null", "{}", "\"x\"", "3"] {
            let json = format!(
                r#"{{"messages":[{{"_id":"m","rid":"r","ts":"2026-10-20T14:00:00.000Z","u":{{"_id":"u"}},"attachments":{attachments}}}]}}"#
            );
            let page = parse_history(json.as_bytes(), 50).unwrap();
            assert_eq!(page.messages[0].attachments, [], "{attachments}");
        }
    }

    // RICH-005
    #[test]
    fn a_live_message_carries_its_attachments() {
        let fixture = serde_json::json!({
            "eventName": "r",
            "args": [{
                "_id": "m",
                "rid": "r",
                "msg": "",
                "ts": { "$date": 1_792_511_428_626_i64 },
                "u": { "_id": "u", "username": "bob" },
                "attachments": [{
                    "type": "file",
                    "title": "p.png",
                    "title_link": "/file-upload/id/p.png",
                    "image_url": "/file-upload/id/p.png",
                    "image_type": "image/png",
                    "image_dimensions": { "width": 2, "height": 1 }
                }]
            }]
        });
        let event = parse_stream_event("stream-room-messages", fixture).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        let image = message.attachments[0].image.as_ref().unwrap();
        assert_eq!(image.path, "/file-upload/id/p.png");
        assert_eq!(image.dimensions, Some((2, 1)));
    }

    // RICH-003
    #[test]
    fn reads_the_custom_emoji_of_the_server() {
        let emoji = parse_custom_emoji(EMOJI_CUSTOM.as_bytes()).unwrap();
        assert_eq!(
            emoji,
            [CustomEmoji {
                name: "probe_emoji".into(),
                aliases: vec!["probe_alias".into()],
                extension: "png".into(),
            }]
        );
    }

    // RICH-003
    #[test]
    fn a_server_without_custom_emoji_lists_none() {
        let body = br#"{"emojis":{"update":[],"remove":[]},"success":true}"#;
        assert!(parse_custom_emoji(body).unwrap().is_empty());
    }

    // RICH-001
    #[test]
    fn every_message_of_the_rich_history_has_markdown() {
        let page = parse_history(HISTORY_RICH.as_bytes(), 50).unwrap();
        assert_eq!(page.messages.len(), 35);
        for message in page.messages {
            let md = message.md.unwrap_or_else(|| panic!("{:?}", message.text));
            assert!(!md.is_empty(), "{:?}", message.text);
            let mut unknown = Vec::new();
            for block in &md {
                if let md::Block::Unknown { kind, .. } = block {
                    unknown.push(kind.clone());
                }
            }
            assert!(unknown.is_empty(), "{:?}: {unknown:?}", message.text);
        }
    }

    // RICH-001
    #[test]
    fn reads_formatting_and_blocks_from_the_server() {
        use md::{Block, Inline};
        assert_eq!(
            rich("`inline code` here").md.unwrap(),
            [Block::Paragraph(vec![
                Inline::Code("inline code".into()),
                Inline::Text(" here".into())
            ])]
        );
        assert_eq!(
            rich("```js\nconsole.log(1)\n```").md.unwrap(),
            [Block::Code {
                language: Some("js".into()),
                lines: vec!["console.log(1)".into()]
            }]
        );
        assert!(matches!(
            &rich("```\nno lang\nblock\n```").md.unwrap()[..],
            [Block::Code { language: None, lines }] if *lines == ["no lang", "block"]
        ));
        // A single asterisk is bold for Rocket.Chat.
        let md = rich("**bold** *italic* ~strike~ _it2_ __b2__ ~~st2~~")
            .md
            .unwrap();
        let [Block::Paragraph(inlines)] = &md[..] else {
            panic!("{md:?}")
        };
        assert!(matches!(&inlines[0], Inline::Bold(_)));
        assert!(matches!(&inlines[2], Inline::Bold(_)));
        assert!(matches!(&inlines[4], Inline::Strike(_)));
        assert!(matches!(&inlines[6], Inline::Italic(_)));
        assert!(matches!(&inlines[10], Inline::Strike(_)));
        assert!(matches!(
            &rich("> a quote\n> second line").md.unwrap()[..],
            [Block::Quote(paragraphs)] if paragraphs.len() == 2
        ));
        assert!(matches!(
            &rich("# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6")
                .md
                .unwrap()[..],
            [
                Block::Heading { level: 1, .. },
                Block::LineBreak,
                Block::Heading { level: 2, .. },
                ..
            ]
        ));
    }

    // RICH-001
    #[test]
    fn reads_lists_and_tasks_from_the_server() {
        use md::{Block, Inline, ListItem, Task};
        let item = |number, text: &str| ListItem {
            number,
            content: vec![Inline::Text(text.into())],
        };
        assert_eq!(
            rich("3. starts at three\n4. four").md.unwrap(),
            [Block::OrderedList(vec![
                item(Some(3), "starts at three"),
                item(Some(4), "four")
            ])]
        );
        assert_eq!(
            rich("- [ ] todo\n- [x] done").md.unwrap(),
            [Block::Tasks(vec![
                Task {
                    done: false,
                    content: vec![Inline::Text("todo".into())]
                },
                Task {
                    done: true,
                    content: vec![Inline::Text("done".into())]
                }
            ])]
        );
    }

    // RICH-001
    #[test]
    fn reads_links_from_the_server() {
        use md::{Block, Inline};
        assert_eq!(
            rich("https://example.com/a?b=1").md.unwrap(),
            [Block::Paragraph(vec![Inline::Link {
                url: "https://example.com/a?b=1".into(),
                label: vec![Inline::Text("https://example.com/a?b=1".into())]
            }])]
        );
        let md = rich("[a link](https://example.com) and <https://example.org|label>")
            .md
            .unwrap();
        let [Block::Paragraph(inlines)] = &md[..] else {
            panic!("{md:?}")
        };
        assert_eq!(
            inlines[0],
            Inline::Link {
                url: "https://example.com".into(),
                label: vec![Inline::Text("a link".into())]
            }
        );
        assert_eq!(
            inlines[2],
            Inline::Link {
                url: "https://example.org".into(),
                label: vec![Inline::Text("label".into())]
            }
        );
    }

    // RICH-002
    #[test]
    fn reads_mentions_from_the_server() {
        use md::{Block, Inline};
        let md = rich("hello @admin and @all and @here").md.unwrap();
        let [Block::Paragraph(inlines)] = &md[..] else {
            panic!("{md:?}")
        };
        let users: Vec<_> = inlines
            .iter()
            .filter_map(|i| match i {
                Inline::MentionUser(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(users, ["admin", "all", "here"]);
        assert_eq!(
            rich("see #general").md.unwrap(),
            [Block::Paragraph(vec![
                Inline::Text("see ".into()),
                Inline::MentionChannel("general".into())
            ])]
        );
    }

    // RICH-003
    #[test]
    fn reads_emoji_from_the_server() {
        use md::{Block, Inline};
        let emoji = |short: Option<&str>, unicode: Option<&str>| Inline::Emoji {
            short_code: short.map(str::to_owned),
            unicode: unicode.map(str::to_owned),
        };
        assert_eq!(
            rich("use :smile: and :+1: here").md.unwrap(),
            [Block::Paragraph(vec![
                Inline::Text("use ".into()),
                emoji(Some("smile"), None),
                Inline::Text(" and ".into()),
                emoji(Some("+1"), None),
                Inline::Text(" here".into())
            ])]
        );
        assert_eq!(
            rich("😀").md.unwrap(),
            [Block::BigEmoji(vec![emoji(None, Some("😀"))])]
        );
        assert_eq!(
            rich("😀😀 :smile:").md.unwrap(),
            [Block::BigEmoji(vec![
                emoji(None, Some("😀")),
                emoji(None, Some("😀")),
                emoji(Some("smile"), None)
            ])]
        );
    }

    // RICH-001
    #[test]
    fn reads_the_rarer_nodes_from_the_server() {
        use md::{Block, Inline};
        assert_eq!(
            rich("\\[\\frac{1}{2}\\]").md.unwrap(),
            [Block::Katex("\\frac{1}{2}".into())]
        );
        let md = rich("$$x^2 + y^2$$ and \\(a+b\\)").md.unwrap();
        assert!(matches!(&md[..], [Block::Paragraph(i)] if i[1] == Inline::Katex("a+b".into())));
        assert_eq!(
            rich("color:#ff0000").md.unwrap(),
            [Block::Paragraph(vec![Inline::Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }])]
        );
        let md = rich("<t:1735689600:R> <t:1735689600:f>").md.unwrap();
        let [Block::Paragraph(inlines)] = &md[..] else {
            panic!("{md:?}")
        };
        assert_eq!(
            inlines[0],
            Inline::Timestamp {
                timestamp: "1735689600".into(),
                format: "R".into(),
                fallback: "<t:1735689600:R>".into()
            }
        );
        assert_eq!(
            rich("![alt](https://example.com/i.png)").md.unwrap(),
            [Block::Paragraph(vec![Inline::Image {
                url: "https://example.com/i.png".into(),
                alt: "alt".into()
            }])]
        );
        assert!(matches!(
            &rich("two\n\nparagraphs").md.unwrap()[..],
            [Block::Paragraph(_), Block::LineBreak, Block::Paragraph(_)]
        ));
    }

    // RICH-001
    #[test]
    fn a_message_with_odd_markdown_still_parses() {
        let message = |md: &str| {
            let body = format!(
                r#"{{"message":{{"_id":"m","rid":"r","msg":"hi","ts":"2026-10-20T17:00:57.427Z",
                "u":{{"_id":"u"}}{md}}}}}"#
            );
            parse_message(body.as_bytes()).unwrap()
        };
        assert_eq!(message("").md, None);
        assert_eq!(message(r#","md":null"#).md, None);
        assert_eq!(message(r#","md":"text""#).md, None);
        assert_eq!(message(r#","md":{"type":"PARAGRAPH"}"#).md, None);
        assert_eq!(message(r#","md":[]"#).md, Some(vec![]));
        let unknown =
            message(r#","md":[{"type":"FUTURE","value":"x"},{"type":"PARAGRAPH","value":[]}]"#);
        assert_eq!(unknown.md.unwrap().len(), 2);
    }

    // RICH-001
    #[test]
    fn a_live_message_carries_its_markdown() {
        let event = stream_event("stream-room-messages", DDP_MESSAGE).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(
            message.md,
            Some(vec![md::Block::Paragraph(vec![md::Inline::Text(
                "hello live".into()
            )])])
        );
    }

    // SEND-001
    #[test]
    fn parses_a_stored_message() {
        let body =
            br#"{"message":{"_id":"m1","rid":"r1","msg":"hi","ts":"2026-10-20T17:00:57.427Z",
            "u":{"_id":"u1","username":"admin","name":"Administrator"},"md":[]},"success":true}"#;
        let message = parse_message(body).unwrap();
        assert_eq!(message.id, "m1");
        assert_eq!(message.room_id, "r1");
        assert_eq!(message.text, "hi");
        assert_eq!(message.ts.to_string(), "2026-10-20T17:00:57.427Z");
        assert_eq!(message.author.id, "u1");
        assert!(parse_message(br#"{"success":true}"#).is_err());
    }

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
                created: None,
                last_message: None,
                avatar_etag: None,
            }]
        );
    }

    // RICH-004
    #[test]
    fn only_the_rooms_with_an_avatar_of_their_own_have_an_etag() {
        let rooms = br#"{"update":[{"_id":"r1","avatarETag":"e1"},{"_id":"r2"}],"remove":[]}"#;
        let etags = parse_room_avatars(rooms).unwrap();
        assert_eq!(etags.len(), 1);
        assert_eq!(etags["r1"], "e1");
    }

    // RICH-004
    #[test]
    fn users_and_rooms_say_whether_they_have_an_avatar_of_their_own() {
        let with = br#"{"user":{"_id":"u","avatarETag":"e1"},"success":true}"#;
        let without = br#"{"user":{"_id":"u"},"success":true}"#;
        assert_eq!(parse_user_avatar_etag(with).unwrap().as_deref(), Some("e1"));
        assert_eq!(parse_user_avatar_etag(without).unwrap(), None);
        let room = br#"{"room":{"_id":"r","avatarETag":"e2","avatarOrigin":"upload"}}"#;
        assert_eq!(parse_room_avatar_etag(room).unwrap().as_deref(), Some("e2"));
        assert_eq!(
            parse_room_avatar_etag(br#"{"room":{"_id":"r"}}"#).unwrap(),
            None
        );
        assert!(parse_user_avatar_etag(b"{}").is_err());
    }

    // ROOM-005
    #[test]
    fn subscriptions_have_the_join_time() {
        let rooms = parse_subscriptions(SUBSCRIPTIONS.as_bytes()).unwrap();
        assert_eq!(rooms[0].created, Some(ts("2026-10-19T22:38:04.040Z")));
        assert!(rooms.iter().all(|r| r.last_message.is_none()));
    }

    // ROOM-005
    #[test]
    fn rooms_have_the_time_of_their_last_message_unless_empty() {
        let activity = parse_room_activity(ROOMS.as_bytes()).unwrap();
        assert_eq!(activity.len(), 3);
        assert_eq!(activity["GENERAL"], ts("2026-10-19T22:38:04.040Z"));
        assert_eq!(
            activity["6ad69bcccaea8883e7e075fe"],
            ts("2026-10-19T22:38:04.360Z")
        );
        assert_eq!(
            activity["6ad69bcccaea8883e7e07604"],
            ts("2026-10-19T22:38:04.420Z")
        );
        assert!(!activity.contains_key("6ad69bcccaea8883e7e07601"));
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

    const DDP_MESSAGE: &str = include_str!("../tests/fixtures/ddp_message.json");
    const DDP_SYSTEM_MESSAGE: &str = include_str!("../tests/fixtures/ddp_system_message.json");
    const DDP_SUBSCRIPTION_INSERTED: &str =
        include_str!("../tests/fixtures/ddp_subscription_inserted.json");
    const DDP_SUBSCRIPTION_UPDATED: &str =
        include_str!("../tests/fixtures/ddp_subscription_updated.json");
    const DDP_SUBSCRIPTION_REMOVED: &str =
        include_str!("../tests/fixtures/ddp_subscription_removed.json");
    const DDP_ROOMS_CHANGED: &str = include_str!("../tests/fixtures/ddp_rooms_changed.json");
    const DDP_ROOMS_CHANGED_MESSAGE: &str =
        include_str!("../tests/fixtures/ddp_rooms_changed_message.json");
    const ROOMS: &str = include_str!("../tests/fixtures/rooms.json");

    fn stream_event(
        collection: &str,
        fixture: &str,
    ) -> Result<Option<DdpEvent>, serde_json::Error> {
        parse_stream_event(collection, serde_json::from_str(fixture).unwrap())
    }

    // RT-001
    #[test]
    fn parses_a_live_message() {
        let event = stream_event("stream-room-messages", DDP_MESSAGE).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(message.id, "u3ypgHtNkxNsvxXpZ");
        assert_eq!(message.room_id, "6ad78dc43a2871a71eddee74");
        assert_eq!(message.text, "hello live");
        assert_eq!(message.kind, None);
        assert_eq!(message.author.username, "admin");
        assert_eq!(message.author.name.as_deref(), Some("Administrator"));
        // EJSON dates are milliseconds since the epoch.
        assert_eq!(message.ts, ts("2026-10-20T15:50:28.626Z"));
    }

    // RT-001, MSG-005
    #[test]
    fn parses_a_live_system_message() {
        let event = stream_event("stream-room-messages", DDP_SYSTEM_MESSAGE).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(message.kind.as_deref(), Some("ru"));
        assert_eq!(message.text, "probe29920");
    }

    // RT-001
    #[test]
    fn a_stream_message_missing_its_argument_is_a_decode_error() {
        let fields = r#"{"eventName":"__my_messages__","args":[]}"#;
        assert!(stream_event("stream-room-messages", fields).is_err());
        assert!(stream_event("stream-room-messages", "{}").is_err());
        let not_a_message = r#"{"eventName":"__my_messages__","args":["text"]}"#;
        assert!(stream_event("stream-room-messages", not_a_message).is_err());
    }

    // RT-001
    #[test]
    fn message_times_can_be_iso_or_ejson() {
        let message = |ts: &str| {
            let fields = format!(
                r#"{{"eventName":"x","args":[{{"_id":"m","rid":"r","ts":{ts},"u":{{"_id":"u"}}}}]}}"#
            );
            stream_event("stream-room-messages", &fields)
        };
        let time = |event: Option<DdpEvent>| match event {
            Some(DdpEvent::Message(message)) => message.ts,
            other => panic!("{other:?}"),
        };
        let expected = ts("2026-10-20T14:13:08.207Z");
        assert_eq!(
            time(message(r#""2026-10-20T14:13:08.207Z""#).unwrap()),
            expected
        );
        assert_eq!(
            time(message(r#"{"$date":1792505588207}"#).unwrap()),
            expected
        );
        assert!(message(r#"{"$date":"soon"}"#).is_err());
        assert!(message(r#"{"$date":9223372036854775807}"#).is_err());
        assert!(message("12").is_err());
    }

    // RT-002
    #[test]
    fn parses_subscription_changes() {
        let changed = |fixture| match stream_event("stream-notify-user", fixture).unwrap() {
            Some(DdpEvent::SubscriptionChanged(subscription)) => subscription,
            other => panic!("{other:?}"),
        };
        let inserted = changed(DDP_SUBSCRIPTION_INSERTED);
        assert_eq!(inserted.room_id, "6ad78dc43a2871a71eddee74");
        assert_eq!(inserted.room_type, RoomType::Channel);
        assert_eq!(inserted.name, "probe29920-c");
        assert_eq!(inserted.display_name.as_deref(), Some("probe29920-c"));
        assert!(inserted.open);
        assert!(!inserted.alert);

        let updated = changed(DDP_SUBSCRIPTION_UPDATED);
        assert_eq!(updated.room_id, inserted.room_id);
        assert!(updated.alert);
    }

    // RT-002
    #[test]
    fn a_removed_subscription_names_its_room() {
        assert_eq!(
            stream_event("stream-notify-user", DDP_SUBSCRIPTION_REMOVED).unwrap(),
            Some(DdpEvent::SubscriptionRemoved {
                room_id: "6ad78dc43a2871a71eddee74".into()
            })
        );
    }

    // ROOM-005
    #[test]
    fn a_room_change_with_a_last_message_is_activity() {
        assert_eq!(
            stream_event("stream-notify-user", DDP_ROOMS_CHANGED_MESSAGE).unwrap(),
            Some(DdpEvent::RoomActivity {
                room_id: "6ad7a58d3a2871a71ede2062".into(),
                last_message: ts("2026-10-20T17:31:59.831Z"),
            })
        );
    }

    // ROOM-005
    #[test]
    fn a_room_change_without_a_last_message_is_not_activity() {
        assert_eq!(
            stream_event("stream-notify-user", DDP_ROOMS_CHANGED).unwrap(),
            None
        );
    }

    // RT-002
    #[test]
    fn other_streams_and_events_are_not_followed() {
        assert_eq!(
            stream_event("stream-notify-room", DDP_MESSAGE).unwrap(),
            None
        );
        let unknown_action =
            r#"{"eventName":"u/subscriptions-changed","args":["moved",{"rid":"r","t":"c"}]}"#;
        assert_eq!(
            stream_event("stream-notify-user", unknown_action).unwrap(),
            None
        );
    }
}
