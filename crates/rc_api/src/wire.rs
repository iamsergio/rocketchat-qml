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
    HistoryPage, LoginResult, Message, MessageChanges, MessageSettings, Quote, Reaction, RoomType,
    ServerInfo, ServerVersion, Subscription, ThreadPage, User,
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
struct SettingsBody {
    #[serde(default)]
    settings: Vec<SettingEntry>,
}

#[derive(Deserialize)]
struct SettingEntry {
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    value: serde_json::Value,
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
    #[serde(default)]
    f: bool,
    /// When the user joined.
    #[serde(default, deserialize_with = "optional_timestamp")]
    ts: Option<jiff::Timestamp>,
}

#[derive(Deserialize)]
struct RoomsBody {
    #[serde(default)]
    update: Vec<RoomBody>,
}

/// A room as `rooms.get` and the `rooms-changed` event send it: the parts of it that the
/// subscription lacks.
#[derive(Deserialize)]
pub(crate) struct RoomBody {
    #[serde(rename = "_id")]
    id: String,
    /// The time of the last message; absent in a room without messages.
    #[serde(default, deserialize_with = "optional_timestamp")]
    pub(crate) lm: Option<jiff::Timestamp>,
    /// Present once the room has an avatar of its own.
    #[serde(default, rename = "avatarETag")]
    pub(crate) avatar_etag: Option<String>,
    /// Present once the room has had a topic. Empty when the topic was taken away.
    #[serde(default)]
    pub(crate) topic: Option<String>,
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
struct ThreadBody {
    #[serde(default)]
    messages: Vec<MessageBody>,
    #[serde(default)]
    total: u32,
}

#[derive(Deserialize)]
struct SyncBody {
    result: SyncResult,
}

#[derive(Deserialize)]
struct SyncResult {
    #[serde(default)]
    updated: Vec<MessageBody>,
    #[serde(default)]
    deleted: Vec<DeletedBody>,
}

#[derive(Deserialize)]
struct DeletedBody {
    #[serde(rename = "_id")]
    id: String,
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
    /// Only on the parent of a thread, and only once it has replies.
    #[serde(default)]
    tcount: u32,
    #[serde(default, deserialize_with = "optional_timestamp")]
    tlm: Option<jiff::Timestamp>,
    #[serde(default)]
    md: Option<serde_json::Value>,
    /// Read entry by entry (see [`parse_attachments`] and [`parse_quotes`]), so one odd entry
    /// doesn't fail the message.
    #[serde(default)]
    attachments: Option<serde_json::Value>,
    #[serde(default, rename = "editedAt", deserialize_with = "optional_timestamp")]
    edited_at: Option<jiff::Timestamp>,
    #[serde(default)]
    reactions: Option<ReactionsBody>,
}

/// The `reactions` of a message: an object from shortcode to who reacted. Anything else means no
/// reactions. It must keep the server's order, so the map is read by a visitor (a `Value` keeps it
/// too, with the `preserve_order` feature of `serde_json`). Untagged, so that the visitor works on
/// buffered input and the odd entry can be skipped without upsetting the parser.
#[derive(Deserialize)]
#[serde(untagged)]
enum ReactionsBody {
    Map(ReactionList),
    Other(serde::de::IgnoredAny),
}

struct ReactionList(Vec<Reaction>);

impl<'de> Deserialize<'de> for ReactionList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ReactionList;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a map of reactions")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut reactions = Vec::new();
                while let Some((emoji, who)) = map.next_entry::<String, Lenient<ReactionBody>>()? {
                    // A reaction nobody made is no reaction.
                    if let Some(ReactionBody { usernames }) = who.0
                        && !usernames.is_empty()
                    {
                        reactions.push(Reaction { emoji, usernames });
                    }
                }
                Ok(ReactionList(reactions))
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

#[derive(Deserialize)]
struct ReactionBody {
    #[serde(default)]
    usernames: Vec<String>,
}

/// `None` instead of an error when the value isn't a `T`. Only for input that is buffered.
struct Lenient<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(T::deserialize(deserializer).ok()))
    }
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

/// A quote: an attachment with the `message_link` of the quoted message. The fields are the quoted
/// message's, and its own quotes are in `attachments`.
#[derive(Deserialize)]
struct QuoteBody {
    message_link: String,
    #[serde(default)]
    author_name: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    md: Option<serde_json::Value>,
    #[serde(default)]
    ts: Option<Lenient<Time>>,
    #[serde(default)]
    attachments: Option<serde_json::Value>,
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
            favorite: body.f,
            created: body.ts,
            last_message: None,
            avatar_etag: None,
            topic: String::new(),
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
            thread_replies: body.tcount,
            thread_last_reply: body.tlm,
            md: body.md.as_ref().and_then(md::parse),
            attachments: parse_attachments(body.attachments.as_ref()),
            quotes: parse_quotes(body.attachments.as_ref()),
            edited: body.edited_at.is_some(),
            reactions: match body.reactions {
                Some(ReactionsBody::Map(list)) => list.0,
                _ => Vec::new(),
            },
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

/// The quotes of a message: the attachments that have a `message_link` and aren't files. Entries
/// that are malformed are skipped. What the quoted message quotes in turn is reduced to its links.
fn parse_quotes(value: Option<&serde_json::Value>) -> Vec<Quote> {
    let Some(entries) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| entry.get("type").and_then(serde_json::Value::as_str) != Some("file"))
        .filter_map(|entry| QuoteBody::deserialize(entry).ok())
        .map(|body| Quote {
            link: body.message_link,
            author: body.author_name,
            text: body.text,
            md: body.md.as_ref().and_then(md::parse),
            ts: body
                .ts
                .and_then(|time| time.0)
                .and_then(|time| time.into_timestamp::<serde_json::Error>().ok()),
            quoted_links: parse_quotes(body.attachments.as_ref())
                .into_iter()
                .map(|quote| quote.link)
                .collect(),
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

/// The answer of `settings.public` for the message settings. What is missing or not a boolean
/// (a number of minutes) keeps its default.
pub(crate) fn parse_message_settings(bytes: &[u8]) -> Result<MessageSettings, serde_json::Error> {
    let body: SettingsBody = serde_json::from_slice(bytes)?;
    let mut settings = MessageSettings::default();
    for entry in body.settings {
        let minutes = || entry.value.as_u64().and_then(|n| u32::try_from(n).ok());
        match entry.id.as_str() {
            "Message_AllowEditing" => {
                settings.editing_allowed = entry.value.as_bool().unwrap_or(true);
            }
            "Message_AllowEditing_BlockEditInMinutes" => {
                settings.edit_minutes = minutes().unwrap_or(0);
            }
            "Message_AllowDeleting" => {
                settings.deleting_allowed = entry.value.as_bool().unwrap_or(true);
            }
            "Message_AllowDeleting_BlockDeleteInMinutes" => {
                settings.delete_minutes = minutes().unwrap_or(0);
            }
            _ => {}
        }
    }
    Ok(settings)
}

/// The answer of `settings.public` for `Site_Url`: the URL without a trailing slash. `None` if it
/// is absent, empty or not a text.
pub(crate) fn parse_site_url(bytes: &[u8]) -> Result<Option<String>, serde_json::Error> {
    let body: SettingsBody = serde_json::from_slice(bytes)?;
    Ok(body
        .settings
        .iter()
        .find(|entry| entry.id == "Site_Url")
        .and_then(|entry| entry.value.as_str())
        .map(|url| url.trim_end_matches('/'))
        .filter(|url| !url.is_empty())
        .map(str::to_owned))
}

/// The rooms of a `rooms.get` answer, by room id.
pub(crate) fn parse_rooms(bytes: &[u8]) -> Result<HashMap<String, RoomBody>, serde_json::Error> {
    let body: RoomsBody = serde_json::from_slice(bytes)?;
    Ok(body
        .update
        .into_iter()
        .map(|room| (room.id.clone(), room))
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

/// The answer of `chat.getThreadMessages`. `offset` is what was asked for: the page ends the
/// thread if it reaches `total`.
pub(crate) fn parse_thread_messages(
    bytes: &[u8],
    offset: usize,
) -> Result<ThreadPage, serde_json::Error> {
    let body: ThreadBody = serde_json::from_slice(bytes)?;
    let total = usize::try_from(body.total).unwrap_or(usize::MAX);
    Ok(ThreadPage {
        reached_start: offset.saturating_add(body.messages.len()) >= total,
        total: body.total,
        messages: body.messages.into_iter().map(Message::from).collect(),
    })
}

/// The answer of `chat.syncMessages`.
pub(crate) fn parse_changes(bytes: &[u8]) -> Result<MessageChanges, serde_json::Error> {
    let body: SyncBody = serde_json::from_slice(bytes)?;
    Ok(MessageChanges {
        updated: body.result.updated.into_iter().map(Message::from).collect(),
        deleted: body.result.deleted.into_iter().map(|d| d.id).collect(),
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
        // What happened, then the room.
        "stream-notify-user" if fields.event_name.ends_with("/rooms-changed") => {
            next_arg()?;
            let room: RoomBody = serde_json::from_value(next_arg()?)?;
            Ok(Some(DdpEvent::RoomChanged {
                room_id: room.id,
                last_message: room.lm,
                topic: room.topic,
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
        // The id of the deleted message. The room is in the event name.
        "stream-notify-room" => {
            let Some(room_id) = fields.event_name.strip_suffix("/deleteMessage") else {
                return Ok(None);
            };
            let deleted: DeletedBody = serde_json::from_value(next_arg()?)?;
            Ok(Some(DdpEvent::MessageDeleted {
                room_id: room_id.to_owned(),
                message_id: deleted.id,
            }))
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
    const THREAD_MESSAGES: &str = include_str!("../tests/fixtures/thread_messages.json");

    const HISTORY_ATTACHMENTS: &str = include_str!("../tests/fixtures/history_attachments.json");
    const HISTORY_QUOTES: &str = include_str!("../tests/fixtures/history_quotes.json");
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

    fn quoted(text: &str) -> Message {
        parse_history(HISTORY_QUOTES.as_bytes(), 50)
            .unwrap()
            .messages
            .into_iter()
            .find(|m| m.text.ends_with(text))
            .unwrap_or_else(|| panic!("no message {text:?} in the fixture"))
    }

    // ACT-005
    #[test]
    fn reads_the_quote_of_a_message() {
        let original = quoted("original **bold** text");
        let message = quoted(") first reply");
        let [quote] = message.quotes.as_slice() else {
            panic!("expected one quote, got {:?}", message.quotes);
        };
        assert_eq!(
            quote.link,
            format!(
                "http://127.0.0.1:3000/channel/quoteprobe22373?msg={}",
                original.id
            )
        );
        assert_eq!(quote.author, "admin");
        assert_eq!(quote.text, "original **bold** text");
        assert_eq!(quote.ts, Some(original.ts));
        assert_eq!(quote.quoted_links, Vec::<String>::new());
        let md = quote.md.as_ref().unwrap();
        assert!(matches!(md.as_slice(), [md::Block::Paragraph(inlines)] if inlines.len() == 3));
        // The reply's own text and markdown keep the link.
        assert_eq!(message.text, format!("[ ]({}) first reply", quote.link));
        assert!(matches!(
            message.md.as_deref(),
            Some([md::Block::Paragraph(inlines)])
                if matches!(&inlines[0], md::Inline::Link { url, .. } if *url == quote.link)
        ));
        assert!(message.attachments.is_empty());
    }

    // ACT-005
    #[test]
    fn a_quote_of_a_quote_keeps_the_links_of_what_it_quotes() {
        let first = quoted(") first reply");
        let message = quoted(") second reply");
        let [outer] = message.quotes.as_slice() else {
            panic!("expected one quote, got {:?}", message.quotes);
        };
        assert_eq!(outer.text, first.text);
        assert_eq!(outer.quoted_links, [first.quotes[0].link.clone()]);
        assert!(outer.link.ends_with(&first.id));
    }

    // ACT-005
    #[test]
    fn a_message_without_quotes_has_none() {
        assert_eq!(quoted("original **bold** text").quotes, []);
        assert_eq!(attached("msgtext").quotes, []);
        assert_eq!(attached("msgimage").quotes, []);
    }

    // ACT-005
    #[test]
    fn a_malformed_quote_is_skipped_and_the_message_stays() {
        let json = serde_json::json!({ "messages": [{
            "_id": "m", "rid": "r", "ts": "2026-10-20T14:00:00.000Z", "u": { "_id": "u" },
            "msg": "x",
            "attachments": [
                { "message_link": 3 },
                { "text": "no link" },
                "not an object",
                { "type": "file", "title_link": "/f", "message_link": "http://a/b?msg=file" },
                { "message_link": "http://a/b?msg=odd", "ts": "yesterday", "text": 4 },
                { "message_link": "http://a/b?msg=fine", "ts": "yesterday" }
            ]
        }]});
        let page = parse_history(json.to_string().as_bytes(), 50).unwrap();
        let quotes = &page.messages[0].quotes;
        assert_eq!(quotes.len(), 1, "{quotes:?}");
        assert_eq!(quotes[0].link, "http://a/b?msg=fine");
        // The time is odd, the quote stays.
        assert_eq!((quotes[0].author.as_str(), quotes[0].ts), ("", None));
    }

    // ACT-005
    #[test]
    fn a_live_message_carries_its_quotes() {
        let fixture = serde_json::json!({
            "eventName": "r",
            "args": [{
                "_id": "m",
                "rid": "r",
                "msg": "[ ](http://s/channel/c?msg=q) hi",
                "ts": { "$date": 1_792_511_428_626_i64 },
                "u": { "_id": "u", "username": "bob" },
                "attachments": [{
                    "text": "quoted",
                    "message_link": "http://s/channel/c?msg=q",
                    "author_name": "alice",
                    "ts": "2026-10-21T09:00:00.000Z"
                }]
            }]
        });
        let event = parse_stream_event("stream-room-messages", fixture).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(message.quotes[0].author, "alice");
        assert_eq!(message.quotes[0].text, "quoted");
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

    // ACT-001, ACT-002
    #[test]
    fn reads_the_message_settings_of_the_server() {
        let body = br#"{"settings":[
            {"_id":"Message_AllowDeleting","value":false},
            {"_id":"Message_AllowDeleting_BlockDeleteInMinutes","value":15},
            {"_id":"Message_AllowEditing","value":true},
            {"_id":"Message_AllowEditing_BlockEditInMinutes","value":5},
            {"_id":"Something_Else","value":7}],"success":true}"#;
        assert_eq!(
            parse_message_settings(body).unwrap(),
            MessageSettings {
                editing_allowed: true,
                edit_minutes: 5,
                deleting_allowed: false,
                delete_minutes: 15,
            }
        );
    }

    // ACT-001, ACT-002
    #[test]
    fn missing_or_odd_message_settings_have_their_defaults() {
        let body = br#"{"settings":[
            {"_id":"Message_AllowEditing","value":"yes"},
            {"_id":"Message_AllowEditing_BlockEditInMinutes","value":-3},
            {"_id":"Message_AllowDeleting_BlockDeleteInMinutes"}],"success":true}"#;
        assert_eq!(
            parse_message_settings(body).unwrap(),
            MessageSettings::default()
        );
        let empty = br#"{"success":true}"#;
        assert_eq!(
            parse_message_settings(empty).unwrap(),
            MessageSettings::default()
        );
    }

    // ACT-005
    #[test]
    fn the_site_url_is_a_non_empty_text_or_nothing() {
        let site_url = |value: &str| {
            let body = format!(r#"{{"settings":[{{"_id":"Site_Url","value":{value}}}]}}"#);
            parse_site_url(body.as_bytes()).unwrap()
        };
        assert_eq!(site_url("\"https://a.b\""), Some("https://a.b".into()));
        assert_eq!(site_url("\"https://a.b//\""), Some("https://a.b".into()));
        for odd in ["\"\"", "\"/\"", "null", "3", "true", "[]"] {
            assert_eq!(site_url(odd), None, "{odd}");
        }
        let absent = br#"{"settings":[],"success":true}"#;
        assert_eq!(parse_site_url(absent).unwrap(), None);
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
                favorite: false,
                created: None,
                last_message: None,
                avatar_etag: None,
                topic: String::new(),
            }]
        );
    }

    // ROOM-006
    #[test]
    fn a_subscription_says_whether_it_is_a_favorite() {
        let rooms = parse_subscriptions(
            br#"{"update":[{"rid":"a","t":"c","f":true},{"rid":"b","t":"c","f":false},
            {"rid":"c","t":"c"}]}"#,
        )
        .unwrap();
        let favorites: Vec<_> = rooms.iter().map(|r| r.favorite).collect();
        assert_eq!(favorites, [true, false, false]);
    }

    // ROOM-006
    #[test]
    fn a_pushed_subscription_says_whether_it_is_a_favorite() {
        let fixture =
            DDP_SUBSCRIPTION_UPDATED.replacen(r#""open": true,"#, r#""open": true, "f": true,"#, 1);
        let favorite = |fixture: &str| match stream_event("stream-notify-user", fixture).unwrap() {
            Some(DdpEvent::SubscriptionChanged(subscription)) => subscription.favorite,
            other => panic!("{other:?}"),
        };
        assert!(favorite(&fixture));
        assert!(!favorite(DDP_SUBSCRIPTION_UPDATED));
    }

    // RICH-004
    #[test]
    fn only_the_rooms_with_an_avatar_of_their_own_have_an_etag() {
        let rooms = br#"{"update":[{"_id":"r1","avatarETag":"e1"},{"_id":"r2"}],"remove":[]}"#;
        let rooms = parse_rooms(rooms).unwrap();
        assert_eq!(rooms["r1"].avatar_etag.as_deref(), Some("e1"));
        assert_eq!(rooms["r2"].avatar_etag, None);
    }

    // ROOM-009
    #[test]
    fn rooms_have_a_topic_unless_it_is_empty_or_missing() {
        let rooms = br#"{"update":[{"_id":"r1","topic":"Plans for May"},{"_id":"r2","topic":""},
            {"_id":"r3"}],"remove":[]}"#;
        let rooms = parse_rooms(rooms).unwrap();
        assert_eq!(rooms["r1"].topic.as_deref(), Some("Plans for May"));
        assert_eq!(rooms["r2"].topic.as_deref(), Some(""));
        assert_eq!(rooms["r3"].topic, None);
    }

    // ROOM-009
    #[test]
    fn a_room_push_says_the_topic_it_carries_and_an_empty_one_is_a_cleared_topic() {
        let topic = |fixture: &str| match stream_event("stream-notify-user", fixture).unwrap() {
            Some(DdpEvent::RoomChanged { topic, .. }) => topic,
            other => panic!("{other:?}"),
        };
        let with = |topic: &str| {
            DDP_ROOMS_CHANGED_MESSAGE.replacen(
                r#""msgs": 1,"#,
                &format!(r#""msgs": 1, "topic": "{topic}","#),
                1,
            )
        };
        assert_eq!(topic(&with("Plans")).as_deref(), Some("Plans"));
        assert_eq!(topic(&with("")).as_deref(), Some(""));
        assert_eq!(topic(DDP_ROOMS_CHANGED_MESSAGE), None);
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
        let rooms = parse_rooms(ROOMS.as_bytes()).unwrap();
        let last = |id: &str| rooms[id].lm;
        assert_eq!(last("GENERAL"), Some(ts("2026-10-19T22:38:04.040Z")));
        assert_eq!(
            last("6ad69bcccaea8883e7e075fe"),
            Some(ts("2026-10-19T22:38:04.360Z"))
        );
        assert_eq!(
            last("6ad69bcccaea8883e7e07604"),
            Some(ts("2026-10-19T22:38:04.420Z"))
        );
        assert_eq!(last("6ad69bcccaea8883e7e07601"), None);
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

    // ACT-004
    #[test]
    fn the_parent_of_a_thread_has_its_reply_count_and_last_reply_time() {
        let page = parse_history(HISTORY.as_bytes(), 50).unwrap();
        let parent = &page.messages[1];
        assert_eq!(parent.thread_replies, 2);
        assert_eq!(
            parent.thread_last_reply,
            Some(ts("2026-10-20T14:13:08.250Z"))
        );
        let reply = &page.messages[0];
        assert_eq!(reply.thread_replies, 0);
        assert_eq!(reply.thread_last_reply, None);
        assert_eq!(page.messages[2].thread_replies, 0);
    }

    // ACT-004
    #[test]
    fn a_pushed_parent_carries_its_new_reply_count() {
        let fixture = DDP_MESSAGE.replacen(
            r#""msg": "#,
            r#""tcount": 4, "tlm": {"$date": 1792511428650}, "msg": "#,
            1,
        );
        let event = stream_event("stream-room-messages", &fixture).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(message.thread_replies, 4);
        assert_eq!(
            message.thread_last_reply,
            jiff::Timestamp::from_millisecond(1_792_511_428_650).ok()
        );
    }

    // ACT-004
    #[test]
    fn thread_messages_are_the_replies_with_the_thread_they_belong_to() {
        let page = parse_thread_messages(THREAD_MESSAGES.as_bytes(), 0).unwrap();
        let texts: Vec<_> = page.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["reply 3", "reply 2"]);
        assert_eq!(page.total, 3);
        assert!(page.messages.iter().all(|m| m.thread_id.is_some()));
        assert!(!page.messages[0].thread_shown);
        assert!(page.messages[1].thread_shown);
    }

    // ACT-004
    #[test]
    fn a_thread_page_reaches_the_start_when_the_offset_and_the_page_cover_the_total() {
        // The fixture holds 2 of 3 replies.
        let reached = |offset| {
            parse_thread_messages(THREAD_MESSAGES.as_bytes(), offset)
                .unwrap()
                .reached_start
        };
        assert!(!reached(0));
        assert!(reached(1));
        assert!(reached(5));
    }

    // ACT-004
    #[test]
    fn a_thread_without_replies_is_an_empty_page_at_the_start() {
        let page =
            parse_thread_messages(br#"{"messages":[],"count":0,"offset":0,"total":0}"#, 0).unwrap();
        assert_eq!((page.messages.len(), page.total), (0, 0));
        assert!(page.reached_start);
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
            Some(DdpEvent::RoomChanged {
                room_id: "6ad7a58d3a2871a71ede2062".into(),
                last_message: Some(ts("2026-10-20T17:31:59.831Z")),
                topic: None,
            })
        );
    }

    // ROOM-005
    #[test]
    fn a_room_change_without_a_last_message_has_none() {
        assert!(matches!(
            stream_event("stream-notify-user", DDP_ROOMS_CHANGED).unwrap(),
            Some(DdpEvent::RoomChanged {
                last_message: None,
                ..
            })
        ));
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

    fn history_message(extra: &str) -> Message {
        let json = format!(
            r#"{{"messages":[{{"_id":"m","rid":"r","ts":"2026-10-20T14:00:00.000Z","u":{{"_id":"u"}}{extra}}}]}}"#
        );
        parse_history(json.as_bytes(), 50)
            .unwrap()
            .messages
            .remove(0)
    }

    fn reaction(emoji: &str, usernames: &[&str]) -> Reaction {
        Reaction {
            emoji: emoji.into(),
            usernames: usernames.iter().map(|&name| name.into()).collect(),
        }
    }

    // RICH-006
    #[test]
    fn an_edited_message_says_so() {
        assert!(!history_message("").edited);
        let edited = history_message(
            r#","editedAt":"2026-10-20T14:05:00.000Z","editedBy":{"_id":"u","username":"bob"}"#,
        );
        assert!(edited.edited);
    }

    // RICH-006
    #[test]
    fn a_live_edit_is_the_whole_message_with_an_ejson_edit_time() {
        let fixture = serde_json::json!({
            "eventName": "r",
            "args": [{
                "_id": "m",
                "rid": "r",
                "msg": "new text",
                "ts": { "$date": 1_792_511_428_626_i64 },
                "u": { "_id": "u", "username": "bob" },
                "editedAt": { "$date": 1_792_511_528_626_i64 },
                "editedBy": { "_id": "u", "username": "bob" }
            }]
        });
        let event = parse_stream_event("stream-room-messages", fixture).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(message.text, "new text");
        assert!(message.edited);
    }

    // RICH-007
    #[test]
    fn reactions_keep_the_servers_order() {
        let message = history_message(
            r#","reactions":{":zebra:":{"usernames":["a"]},":apple:":{"usernames":["b","c"],"names":["B","C"]},":+1:":{"usernames":["d"]}}"#,
        );
        assert_eq!(
            message.reactions,
            [
                reaction(":zebra:", &["a"]),
                reaction(":apple:", &["b", "c"]),
                reaction(":+1:", &["d"]),
            ]
        );
    }

    // ACT-003
    #[test]
    fn a_pushed_message_keeps_the_order_of_the_reactions_too() {
        let fixture = serde_json::from_str(
            r#"{
                "eventName": "r",
                "args": [{
                    "_id": "m",
                    "rid": "r",
                    "msg": "hi",
                    "ts": { "$date": 1792511428626 },
                    "u": { "_id": "u", "username": "bob" },
                    "reactions": {
                        ":zebra:": { "usernames": ["a"] },
                        ":apple:": { "usernames": ["b"] }
                    }
                }]
            }"#,
        )
        .unwrap();
        let event = parse_stream_event("stream-room-messages", fixture).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(
            message.reactions,
            [reaction(":zebra:", &["a"]), reaction(":apple:", &["b"])]
        );
    }

    // RICH-007
    #[test]
    fn a_message_without_reactions_has_none() {
        assert_eq!(history_message("").reactions, []);
        assert_eq!(history_message(r#","reactions":{}"#).reactions, []);
    }

    // RICH-007
    #[test]
    fn malformed_reactions_mean_none_and_the_message_stays() {
        for reactions in ["null", "[]", "[1,{\"a\":2}]", "\"x\"", "3", "true"] {
            let message = history_message(&format!(r#","reactions":{reactions}"#));
            assert_eq!(message.reactions, [], "{reactions}");
            assert_eq!(message.id, "m");
        }
    }

    // RICH-007
    #[test]
    fn a_malformed_reaction_is_skipped_and_the_others_stay() {
        let message = history_message(
            r#","reactions":{":a:":5,":b:":{"usernames":["x"]},":c:":{"usernames":"y"},":d:":{"usernames":[]},":e:":{"usernames":["z"]}},"msg":"after""#,
        );
        assert_eq!(
            message.reactions,
            [reaction(":b:", &["x"]), reaction(":e:", &["z"])]
        );
        assert_eq!(message.text, "after");
    }

    // RICH-007
    #[test]
    fn a_live_reaction_arrives_with_the_whole_message() {
        let fixture = serde_json::json!({
            "eventName": "r",
            "args": [{
                "_id": "m",
                "rid": "r",
                "msg": "hi",
                "ts": { "$date": 1_792_511_428_626_i64 },
                "u": { "_id": "u", "username": "bob" },
                "reactions": { ":thumbsup:": { "usernames": ["admin"] } }
            }]
        });
        let event = parse_stream_event("stream-room-messages", fixture).unwrap();
        let Some(DdpEvent::Message(message)) = event else {
            panic!("expected a message, got {event:?}");
        };
        assert_eq!(message.reactions, [reaction(":thumbsup:", &["admin"])]);
    }

    // RICH-006
    #[test]
    fn a_deletion_names_the_room_and_the_message() {
        let fixture = r#"{"eventName":"room1/deleteMessage","args":[{"_id":"msg1"}]}"#;
        assert_eq!(
            stream_event("stream-notify-room", fixture).unwrap(),
            Some(DdpEvent::MessageDeleted {
                room_id: "room1".into(),
                message_id: "msg1".into(),
            })
        );
    }

    // RICH-006
    #[test]
    fn other_room_notifications_are_not_deletions() {
        let typing = r#"{"eventName":"room1/typing","args":["bob",true]}"#;
        assert_eq!(stream_event("stream-notify-room", typing).unwrap(), None);
        let no_id = r#"{"eventName":"room1/deleteMessage","args":[]}"#;
        assert!(stream_event("stream-notify-room", no_id).is_err());
    }

    // RICH-006
    #[test]
    fn parses_the_changes_since_a_time() {
        let body = r#"{"result":{
            "updated":[{"_id":"m1","rid":"r","msg":"edited","ts":"2026-10-20T14:00:00.000Z",
                "u":{"_id":"u"},"editedAt":"2026-10-20T14:05:00.000Z"}],
            "deleted":[{"_id":"m2","_deletedAt":"2026-10-20T23:36:21.795Z"}]},"success":true}"#;
        let changes = parse_changes(body.as_bytes()).unwrap();
        assert_eq!(changes.updated.len(), 1);
        assert_eq!(changes.updated[0].text, "edited");
        assert!(changes.updated[0].edited);
        assert_eq!(changes.deleted, ["m2"]);
    }

    // RICH-006
    #[test]
    fn no_changes_is_empty() {
        let changes = parse_changes(br#"{"result":{},"success":true}"#).unwrap();
        assert_eq!(changes.updated, []);
        assert_eq!(changes.deleted, Vec::<String>::new());
    }
}
