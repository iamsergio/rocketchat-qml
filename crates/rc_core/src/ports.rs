// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the core needs from the outside world. Use cases depend on these traits only; adapters
//! implement them for real I/O and tests implement them with fakes.

use std::path::PathBuf;

use rc_api::ServerVersion;
use secrecy::SecretString;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::domain::{
    AvatarOwner, Credentials, Message, MessageId, RoomId, RoomKind, StoredSession, User,
};

/// Why a server call failed. Owned strings only, so that fakes can build and compare them.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error("not a valid server URL: {0}")]
    InvalidUrl(String),
    #[error("can't reach the server: {0}")]
    Unreachable(String),
    #[error("the server doesn't look like Rocket.Chat")]
    NotRocketChat,
    #[error("wrong username or password")]
    InvalidCredentials,
    #[error("the session is no longer valid")]
    Unauthorized,
    #[error("the server answered {status}: {message}")]
    Server { status: u16, message: String },
    #[error("unexpected answer from the server: {0}")]
    Unexpected(String),
}

/// Opens a [`ServerApi`] for a URL. Sync: it only validates and builds, no I/O.
pub trait Connector: Send + Sync + 'static {
    type Api: ServerApi;

    /// # Errors
    ///
    /// [`ApiError::InvalidUrl`] if `server` can't be used.
    fn connect(&self, server: &str) -> Result<Self::Api, ApiError>;
}

/// One Rocket.Chat server. Cheap to clone.
pub trait ServerApi: Clone + Send + Sync + 'static {
    fn version(&self) -> impl Future<Output = Result<ServerVersion, ApiError>> + Send;

    /// The server's own URL (its `Site_Url` setting), without a trailing slash: `None` if it has
    /// none. Needs no login.
    fn site_url(&self) -> impl Future<Output = Result<Option<String>, ApiError>> + Send;

    fn login(
        &self,
        username: &str,
        password: &SecretString,
    ) -> impl Future<Output = Result<(Credentials, User), ApiError>> + Send;

    /// The user the credentials belong to. Fails with [`ApiError::Unauthorized`] if they have
    /// expired or were revoked.
    fn me(&self, credentials: &Credentials) -> impl Future<Output = Result<User, ApiError>> + Send;

    /// The rooms the user has joined, hidden ones included.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked.
    fn subscriptions(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<Vec<rc_api::Subscription>, ApiError>> + Send;

    /// The custom emoji of the server.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked.
    fn custom_emoji(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<Vec<rc_api::CustomEmoji>, ApiError>> + Send;

    /// What the server lets users do with their own messages.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked.
    fn message_settings(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<rc_api::MessageSettings, ApiError>> + Send;

    /// The avatar a user or room has uploaded: `None` if it has none, as the server draws one
    /// for those and the frontend shows initials instead. Asks the server whether there is one
    /// before downloading.
    ///
    /// # Errors
    ///
    /// [`ApiError::Server`] if the server refuses, [`ApiError::Unauthorized`] if it rejects the
    /// credentials.
    fn avatar(
        &self,
        credentials: &Credentials,
        owner: &AvatarOwner,
    ) -> impl Future<Output = Result<Option<Image>, ApiError>> + Send;

    /// A file uploaded to a room, at the server path a message gives (`/file-upload/...`).
    ///
    /// # Errors
    ///
    /// [`ApiError::Server`] if the server refuses or the path isn't a file's, and
    /// [`ApiError::Unauthorized`] if it rejects the credentials.
    fn file(
        &self,
        credentials: &Credentials,
        path: &str,
    ) -> impl Future<Output = Result<Image, ApiError>> + Send;

    /// A page of the room's history: the newest messages sent before `before`, or the newest of all
    /// without it, at most `count`. `before` is inclusive, so the message sent at that instant
    /// comes back too: the caller de-duplicates.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked.
    fn history(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        kind: RoomKind,
        before: Option<jiff::Timestamp>,
        count: usize,
    ) -> impl Future<Output = Result<Page, ApiError>> + Send;

    /// Posts a text message under the id the caller picked and returns it as the server stored it,
    /// with the server's time. The server stores a message once per id, and sending an id it holds
    /// already (a retry whose first attempt got through) succeeds with the stored message, so a
    /// caller that doesn't know whether an attempt arrived may always send again.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked,
    /// [`ApiError::Server`] if the server refuses the message.
    fn send_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
    ) -> impl Future<Output = Result<Message, ApiError>> + Send;

    /// Replaces the text of a message of the user's and returns it as the server stored it.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked,
    /// [`ApiError::Server`] if the server refuses the edit.
    fn edit_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
    ) -> impl Future<Output = Result<Message, ApiError>> + Send;

    /// Deletes a message of the user's.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked,
    /// [`ApiError::Server`] if the server refuses the deletion.
    fn delete_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;

    /// Adds the user's reaction to a message, or takes it back. `emoji` is the shortcode with its
    /// colons. Asking for what is already so succeeds.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked,
    /// [`ApiError::Server`] if the server refuses the reaction (an emoji it doesn't know...).
    fn react(
        &self,
        credentials: &Credentials,
        id: &MessageId,
        emoji: &str,
        react: bool,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;

    /// Tells the server the user has seen everything in the room: it zeroes the room's unread and
    /// mention counts and pushes the changed subscription.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked,
    /// [`ApiError::Server`] if the user isn't in the room.
    fn mark_read(
        &self,
        credentials: &Credentials,
        room: &RoomId,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;

    /// What happened to the room's messages since `since`: the ones sent or changed (whole) and the
    /// ids of the ones deleted. It may list more than changed, so the caller applies it to what
    /// it holds. The server decides by its own clock, so the caller leaves a margin.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked,
    /// [`ApiError::Server`] if the room doesn't exist or isn't visible to the user.
    fn changes_since(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        since: jiff::Timestamp,
    ) -> impl Future<Output = Result<Changes, ApiError>> + Send;

    /// Opens the realtime connection for the session: connects, logs in with the credentials and
    /// subscribes to the user's messages, subscription changes and room activity. What the server
    /// pushes from then on comes out of [`Realtime::events`], which ends when the connection
    /// does. Dropping the events closes the connection. Reconnecting is up to the caller.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked.
    fn realtime(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<Realtime, ApiError>> + Send;

    /// Revokes the token on the server.
    fn logout(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;
}

/// An image as the server sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub bytes: Vec<u8>,
    /// The media type the server said, lower case and without parameters.
    pub content_type: Option<String>,
}

/// What the server pushes over the realtime connection, as the core understands it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pushed {
    /// A new or changed message (edited, reacted to...), whole. Not only of the user's rooms: the
    /// server also pushes the messages of public channels the user isn't in, so check the room.
    Message(Message),
    /// The user joined a room, or one of their rooms changed: its unread count, its name...
    Subscription(rc_api::Subscription),
    /// The user left a room or was removed from it.
    SubscriptionRemoved(RoomId),
    /// One of the user's rooms has a message at that time. Comes with every new message, and with
    /// other changes of the room too, then with a time the room has already.
    Activity { room: RoomId, at: jiff::Timestamp },
    /// The server says this avatar changed, whoever's it is: fetch it again if it is shown.
    AvatarChanged(AvatarOwner),
    /// A message was deleted. Only comes for the room the connection was told is open.
    MessageDeleted { room: RoomId, id: MessageId },
}

/// What [`ServerApi::realtime`] returns: the pushed events in the order the server sent them.
pub type PushedEvents = UnboundedReceiver<Pushed>;

/// An open realtime connection.
#[derive(Debug)]
pub struct Realtime {
    pub events: PushedEvents,
    /// Says which room is open. The server pushes a deleted message only for the room it was told
    /// about, so the connection follows the room that is open: send `Some(room)` when one opens
    /// and `None` when it closes. Only one room is followed at a time. A room that can't be
    /// followed is logged and nothing else.
    pub watch: UnboundedSender<Option<RoomId>>,
}

/// What [`ServerApi::changes_since`] returns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Messages sent or changed since then, whole. The timeline shows only those of the room's
    /// own messages, as in a page.
    pub updated: Vec<Message>,
    /// Messages deleted since then.
    pub deleted: Vec<MessageId>,
}

/// What [`ServerApi::history`] returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The messages the timeline shows, in any order.
    pub messages: Vec<Message>,
    /// There is nothing before them.
    pub reached_start: bool,
}

/// What is held of one room's messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomHistory {
    /// Oldest first, unique by id. The oldest one is where the next page starts.
    pub messages: Vec<Message>,
    /// Nothing exists before the first message.
    pub reached_start: bool,
    /// When the messages were last known to be up to date: the client's clock just before it sent
    /// the request that brought them up to date (the newest page, or a catch-up). `None` if they
    /// never were.
    pub synced_at: Option<jiff::Timestamp>,
}

/// The messages loaded so far, per room. Owned by the session actor, which calls it one room at a
/// time, so it needs no locking and may block briefly.
pub trait MessageStore: Send + 'static {
    /// What is held for the room: nothing, if it was never loaded.
    fn load(&self, room: &RoomId) -> RoomHistory;

    fn save(&mut self, room: &RoomId, history: &RoomHistory);

    /// Forgets everything, for all rooms.
    fn clear(&mut self);
}

/// Why the secret store failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// No keyring service, it's locked, or access was denied.
    #[error("the secret store isn't available: {0}")]
    Unavailable(String),
    /// An entry that can't be read back.
    #[error("the stored session is unreadable: {0}")]
    Corrupt(String),
}

/// Where the session survives restarts. The calls block: the core runs them off the async threads,
/// one at a time and in order.
pub trait SecretStore: Send + Sync + 'static {
    /// `None` if nothing is stored.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store can't be read.
    fn load(&self) -> Result<Option<StoredSession>, StoreError>;

    /// # Errors
    ///
    /// [`StoreError`] if the store can't be written.
    fn save(&self, session: &StoredSession) -> Result<(), StoreError>;

    /// Succeeds if nothing was stored.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store can't be written.
    fn clear(&self) -> Result<(), StoreError>;
}

impl SecretStore for Box<dyn SecretStore> {
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        (**self).load()
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        (**self).save(session)
    }

    fn clear(&self) -> Result<(), StoreError> {
        (**self).clear()
    }
}

/// What a cached file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CacheKind {
    UserAvatar,
    RoomAvatar,
    /// A file attached to a message, named by its path on the server.
    Attachment,
}

/// Names a cached file: the server it came from, what kind of file it is, and its name there (a
/// username or a room id). Adapters turn it into a file name, so any text is fine in `name`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// The host and port of the server, plus its path prefix if it has one.
    pub server: String,
    pub kind: CacheKind,
    pub name: String,
}

impl CacheKey {
    /// The key of an avatar from `server`, the normalized URL the session logged in with.
    #[must_use]
    pub fn avatar(server: &str, owner: &AvatarOwner) -> Self {
        let (kind, name) = match owner {
            AvatarOwner::User(username) => (CacheKind::UserAvatar, username.as_str()),
            AvatarOwner::Room(room) => (CacheKind::RoomAvatar, room.as_str()),
        };
        Self::new(server, kind, name)
    }

    /// The key of the attachment image at `path` (`/file-upload/...`) on `server`.
    #[must_use]
    pub fn attachment(server: &str, path: &str) -> Self {
        Self::new(server, CacheKind::Attachment, path)
    }

    fn new(server: &str, kind: CacheKind, name: &str) -> Self {
        let rest = server.split_once("://").map_or(server, |(_, rest)| rest);
        Self {
            server: rest.trim_end_matches('/').to_ascii_lowercase(),
            kind,
            name: name.to_owned(),
        }
    }
}

/// Why a file couldn't be cached.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CacheError {
    #[error("not a kind of image the cache keeps: {0:?}")]
    UnsupportedType(Option<String>),
    #[error("the cache can't be written: {0}")]
    Io(String),
}

/// A file just stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub path: PathBuf,
    /// The key held other bytes before, or none.
    pub changed: bool,
}

/// Downloaded files, kept where the frontend can open them. The calls block: the core runs them off
/// the async threads, except [`get`](Self::get), which only looks.
pub trait FileCache: Send + Sync + 'static {
    /// The file stored under the key, if any.
    fn get(&self, key: &CacheKey) -> Option<PathBuf>;

    /// Stores `bytes` under the key, in the file type `content_type` says, and drops what the key
    /// held in another type. A reader never sees a half-written file: the file is complete when it
    /// shows up at its path, and the path stays valid while a new copy replaces it.
    ///
    /// # Errors
    ///
    /// [`CacheError::UnsupportedType`] for a type the cache doesn't keep, [`CacheError::Io`] if
    /// the file can't be written.
    fn put(
        &self,
        key: &CacheKey,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<Stored, CacheError>;

    /// Deletes the file stored under the key. Returns whether there was one.
    ///
    /// # Errors
    ///
    /// [`CacheError::Io`] if the file exists and can't be deleted.
    fn remove(&self, key: &CacheKey) -> Result<bool, CacheError>;
}

/// The file extension for a media type, if the cache keeps files of that type.
#[must_use]
pub fn image_extension(content_type: &str) -> Option<&'static str> {
    let media = content_type.split(';').next().unwrap_or_default();
    match media.trim().to_ascii_lowercase().as_str() {
        "image/svg+xml" => Some("svg"),
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Every extension [`image_extension`] can return.
pub const IMAGE_EXTENSIONS: [&str; 5] = ["svg", "png", "jpg", "gif", "webp"];

#[cfg(test)]
mod tests {
    use super::*;

    // RICH-004
    #[test]
    fn the_extension_follows_the_media_type() {
        for (content_type, extension) in [
            ("image/svg+xml", "svg"),
            ("image/png", "png"),
            ("image/jpeg", "jpg"),
            ("image/jpg", "jpg"),
            ("image/gif", "gif"),
            ("image/webp", "webp"),
            ("Image/PNG; charset=binary", "png"),
        ] {
            assert_eq!(
                image_extension(content_type),
                Some(extension),
                "{content_type}"
            );
        }
        for content_type in ["text/html", "application/json", "image/bmp", "", "png"] {
            assert_eq!(image_extension(content_type), None, "{content_type}");
        }
    }

    // RICH-005
    #[test]
    fn an_attachment_key_is_named_by_its_path() {
        let key = CacheKey::attachment("https://Chat.Example.com/team/", "/file-upload/a/b.png");
        assert_eq!(key.server, "chat.example.com/team");
        assert_eq!(key.kind, CacheKind::Attachment);
        assert_eq!(key.name, "/file-upload/a/b.png");
    }

    // RICH-004
    #[test]
    fn a_key_names_the_server_by_host_and_port() {
        let owner = AvatarOwner::User("alice".into());
        let key = CacheKey::avatar("http://127.0.0.1:3000/", &owner);
        assert_eq!(key.server, "127.0.0.1:3000");
        assert_eq!(key.kind, CacheKind::UserAvatar);
        assert_eq!(key.name, "alice");
        let prefixed = CacheKey::avatar("https://Chat.Example.com/team/", &owner);
        assert_eq!(prefixed.server, "chat.example.com/team");
        let room = CacheKey::avatar("http://h", &AvatarOwner::Room("r1".to_owned().into()));
        assert_eq!(
            (room.kind, room.name.as_str()),
            (CacheKind::RoomAvatar, "r1")
        );
    }
}
