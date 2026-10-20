// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the core needs from the outside world. Use cases depend on these traits only; adapters
//! implement them for real I/O and tests implement them with fakes.

use rc_api::ServerVersion;
use secrecy::SecretString;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::domain::{Credentials, Message, MessageId, RoomId, RoomKind, StoredSession, User};

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

    /// Opens the realtime connection for the session: connects, logs in with the credentials and
    /// subscribes to the user's messages and subscription changes. What the server pushes from then
    /// on comes out of the receiver, which ends when the connection does. Dropping the receiver
    /// closes the connection. Reconnecting is up to the caller.
    ///
    /// # Errors
    ///
    /// [`ApiError::Unauthorized`] if the credentials have expired or were revoked.
    fn realtime(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<PushedEvents, ApiError>> + Send;

    /// Revokes the token on the server.
    fn logout(
        &self,
        credentials: &Credentials,
    ) -> impl Future<Output = Result<(), ApiError>> + Send;
}

/// What the server pushes over the realtime connection, as the core understands it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pushed {
    /// A new or edited message. Not only of the user's rooms: the server also pushes the messages
    /// of public channels the user isn't in, so check the room.
    Message(Message),
    /// The user joined a room, or one of their rooms changed: its unread count, its name...
    Subscription(rc_api::Subscription),
    /// The user left a room or was removed from it.
    SubscriptionRemoved(RoomId),
}

/// What [`ServerApi::realtime`] returns: the pushed events in the order the server sent them.
pub type PushedEvents = UnboundedReceiver<Pushed>;

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
