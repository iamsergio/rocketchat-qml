// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The server port over the Rocket.Chat REST API.

use std::path::Path;

use rc_api::{DdpClient, DdpEvent, DdpEvents, RestClient, ServerVersion, SubscriptionId};
use secrecy::SecretString;
use tokio::sync::mpsc;

use crate::directory::{DirectoryChannel, Found, Person};
use crate::domain::{
    AvatarOwner, Credentials, Message, MessageId, RoomId, RoomKind, User, message_from,
};
use crate::ports::{
    ApiError, Changes, Connector, Image, Page, Pushed, Realtime, ServerApi, ThreadTarget,
    ThreadsPage,
};
use crate::threads::ThreadFilter;

pub struct RestConnector;

impl Connector for RestConnector {
    type Api = RestApi;

    fn connect(&self, server: &str) -> Result<RestApi, ApiError> {
        Ok(RestApi {
            client: RestClient::new(server)?,
            server: server.to_owned(),
        })
    }
}

#[derive(Clone)]
pub struct RestApi {
    client: RestClient,
    /// What the realtime connection is opened from; REST needs only the client.
    server: String,
}

impl ServerApi for RestApi {
    async fn version(&self) -> Result<ServerVersion, ApiError> {
        Ok(self.client.info().await?.version)
    }

    async fn site_url(&self) -> Result<Option<String>, ApiError> {
        Ok(self.client.site_url().await?)
    }

    async fn login(
        &self,
        username: &str,
        password: &SecretString,
    ) -> Result<(Credentials, User), ApiError> {
        let result = self.client.login(username, password).await?;
        Ok((credentials_from(result.credentials), user_from(result.user)))
    }

    async fn me(&self, credentials: &Credentials) -> Result<User, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(user_from(session.me().await?))
    }

    async fn subscriptions(
        &self,
        credentials: &Credentials,
    ) -> Result<Vec<rc_api::Subscription>, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.subscriptions().await?)
    }

    async fn custom_emoji(
        &self,
        credentials: &Credentials,
    ) -> Result<Vec<rc_api::CustomEmoji>, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.custom_emoji().await?)
    }

    async fn message_settings(
        &self,
        credentials: &Credentials,
    ) -> Result<rc_api::MessageSettings, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.message_settings().await?)
    }

    async fn avatar(
        &self,
        credentials: &Credentials,
        owner: &AvatarOwner,
    ) -> Result<Option<Image>, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let download = match owner {
            AvatarOwner::User(username) => {
                if session.user_avatar_etag(username).await?.is_none() {
                    return Ok(None);
                }
                session.user_avatar(username).await?
            }
            AvatarOwner::Room(room) => {
                if session.room_avatar_etag(room.as_str()).await?.is_none() {
                    return Ok(None);
                }
                session.room_avatar(room.as_str()).await?
            }
        };
        Ok(Some(Image {
            bytes: download.bytes,
            content_type: download.content_type,
        }))
    }

    async fn file(&self, credentials: &Credentials, path: &str) -> Result<Image, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let download = session.file(path).await?;
        Ok(Image {
            bytes: download.bytes,
            content_type: download.content_type,
        })
    }

    async fn emoji_image(
        &self,
        credentials: &Credentials,
        name: &str,
        extension: &str,
    ) -> Result<Image, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let download = session.emoji_image(name, extension).await?;
        Ok(Image {
            bytes: download.bytes,
            content_type: download.content_type,
        })
    }

    async fn history(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        kind: RoomKind,
        before: Option<jiff::Timestamp>,
        count: usize,
    ) -> Result<Page, ApiError> {
        let room_type = match kind {
            RoomKind::Channel => rc_api::RoomType::Channel,
            RoomKind::Private => rc_api::RoomType::Private,
            RoomKind::Direct => rc_api::RoomType::Direct,
        };
        let session = self.client.session(api_credentials(credentials));
        let page = session
            .history(room.as_str(), &room_type, before, count)
            .await?;
        Ok(Page {
            messages: page
                .messages
                .into_iter()
                .map(message_from)
                .filter(Message::shows_in_room)
                .collect(),
            reached_start: page.reached_start,
        })
    }

    async fn send_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
        thread: Option<&ThreadTarget>,
    ) -> Result<Message, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let stored = match thread {
            Some(target) => {
                session
                    .send_thread_reply(
                        room.as_str(),
                        id.as_str(),
                        text,
                        target.parent.as_str(),
                        target.also_to_channel,
                    )
                    .await?
            }
            None => {
                session
                    .send_message(room.as_str(), id.as_str(), text)
                    .await?
            }
        };
        Ok(message_from(stored))
    }

    async fn upload_file(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        path: &Path,
        name: &str,
        progress: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<String, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session
            .upload_file(room.as_str(), path, name, progress)
            .await?)
    }

    async fn confirm_upload(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        file: &str,
        text: &str,
        thread: Option<&MessageId>,
    ) -> Result<Message, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let stored = session
            .confirm_upload(room.as_str(), file, text, thread.map(MessageId::as_str))
            .await?;
        Ok(message_from(stored))
    }

    async fn thread_messages(
        &self,
        credentials: &Credentials,
        thread: &MessageId,
        offset: usize,
        count: usize,
    ) -> Result<Page, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let page = session
            .thread_messages(thread.as_str(), count, offset)
            .await?;
        Ok(Page {
            messages: page.messages.into_iter().map(message_from).collect(),
            reached_start: page.reached_start,
        })
    }

    async fn threads_list(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        filter: ThreadFilter,
        offset: usize,
        count: usize,
    ) -> Result<ThreadsPage, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let kind = match filter {
            ThreadFilter::All => rc_api::ThreadsKind::All,
            ThreadFilter::Following => rc_api::ThreadsKind::Following,
            ThreadFilter::Unread => rc_api::ThreadsKind::Unread,
        };
        let page = session
            .threads_list(room.as_str(), kind, count, offset)
            .await?;
        Ok(ThreadsPage {
            threads: page.threads.into_iter().map(message_from).collect(),
            reached_end: page.reached_end,
        })
    }

    async fn message(
        &self,
        credentials: &Credentials,
        id: &MessageId,
    ) -> Result<Message, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(message_from(session.message(id.as_str()).await?))
    }

    async fn edit_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
    ) -> Result<Message, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let stored = session
            .edit_message(room.as_str(), id.as_str(), text)
            .await?;
        Ok(message_from(stored))
    }

    async fn delete_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
    ) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.delete_message(room.as_str(), id.as_str()).await?)
    }

    async fn react(
        &self,
        credentials: &Credentials,
        id: &MessageId,
        emoji: &str,
        react: bool,
    ) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.react(id.as_str(), emoji, react).await?)
    }

    async fn mark_read(&self, credentials: &Credentials, room: &RoomId) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.mark_read(room.as_str()).await?)
    }

    async fn mark_unread(&self, credentials: &Credentials, room: &RoomId) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.mark_unread(room.as_str()).await?)
    }

    async fn set_favorite(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        favorite: bool,
    ) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.set_favorite(room.as_str(), favorite).await?)
    }

    async fn leave_room(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        kind: RoomKind,
    ) -> Result<(), ApiError> {
        let room_type = match kind {
            RoomKind::Channel => rc_api::RoomType::Channel,
            RoomKind::Private => rc_api::RoomType::Private,
            RoomKind::Direct => rc_api::RoomType::Direct,
        };
        let session = self.client.session(api_credentials(credentials));
        Ok(session.leave(room.as_str(), &room_type).await?)
    }

    async fn directory_users(
        &self,
        credentials: &Credentials,
        text: &str,
        count: usize,
    ) -> Result<Found<Person>, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let page = session.directory_users(text, count).await?;
        Ok(Found {
            total: page.total,
            items: (page.items.into_iter())
                .map(|user| Person {
                    username: user.username,
                    name: user.name,
                })
                .collect(),
        })
    }

    async fn directory_channels(
        &self,
        credentials: &Credentials,
        text: &str,
        count: usize,
    ) -> Result<Found<DirectoryChannel>, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let page = session.directory_channels(text, count).await?;
        Ok(Found {
            total: page.total,
            items: (page.items.into_iter())
                .map(|channel| DirectoryChannel {
                    id: channel.id.into(),
                    name: channel.display_name.unwrap_or(channel.name),
                    topic: channel.topic,
                    members: channel.users_count,
                })
                .collect(),
        })
    }

    async fn create_direct(
        &self,
        credentials: &Credentials,
        username: &str,
    ) -> Result<RoomId, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.create_direct(username).await?.into())
    }

    async fn join_channel(&self, credentials: &Credentials, room: &RoomId) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.join_channel(room.as_str()).await?)
    }

    async fn room_info(
        &self,
        credentials: &Credentials,
        room: &RoomId,
    ) -> Result<rc_api::RoomInfo, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.room_info(room.as_str()).await?)
    }

    async fn subscription(
        &self,
        credentials: &Credentials,
        room: &RoomId,
    ) -> Result<Option<rc_api::Subscription>, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.subscription(room.as_str()).await?)
    }

    async fn changes_since(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        since: jiff::Timestamp,
    ) -> Result<Changes, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let changes = session.sync_messages(room.as_str(), since).await?;
        Ok(Changes {
            updated: changes.updated.into_iter().map(message_from).collect(),
            deleted: changes.deleted.into_iter().map(Into::into).collect(),
        })
    }

    async fn realtime(&self, credentials: &Credentials) -> Result<Realtime, ApiError> {
        let (ddp, events) = DdpClient::connect(&self.server).await?;
        ddp.login(&api_credentials(credentials)).await?;
        ddp.subscribe_my_messages().await?;
        ddp.subscribe_subscription_changes(credentials.user_id.as_str())
            .await?;
        ddp.subscribe_room_activity(credentials.user_id.as_str())
            .await?;
        // Avatars show without it, only later.
        if let Err(error) = ddp.subscribe_avatar_changes().await {
            tracing::warn!(%error, "avatar changes won't be pushed");
        }
        let (pushed, receiver) = mpsc::unbounded_channel();
        let (watch, watched) = mpsc::unbounded_channel();
        let (read_threads, reads) = mpsc::unbounded_channel();
        tokio::spawn(forward(ddp, events, pushed, watched, reads));
        Ok(Realtime {
            events: receiver,
            watch,
            read_threads,
        })
    }

    async fn logout(&self, credentials: &Credentials) -> Result<(), ApiError> {
        Ok(self
            .client
            .session(api_credentials(credentials))
            .logout()
            .await?)
    }
}

/// Maps the connection's events until it ends or nobody listens any more. The task owns the client,
/// so ending it closes the connection. It also follows the open room: when `watched` names a room,
/// the previous room's subscriptions are ended and the new one's started.
async fn forward(
    ddp: DdpClient,
    mut events: DdpEvents,
    pushed: mpsc::UnboundedSender<Pushed>,
    mut watched: mpsc::UnboundedReceiver<Option<RoomId>>,
    mut reads: mpsc::UnboundedReceiver<MessageId>,
) {
    let mut followed: Vec<SubscriptionId> = Vec::new();
    let mut watching = true;
    let mut reading = true;
    loop {
        tokio::select! {
            room = watched.recv(), if watching => {
                let Some(room) = room else {
                    watching = false;
                    continue;
                };
                for previous in followed.drain(..) {
                    ddp.unsubscribe(&previous);
                }
                if let Some(room) = room {
                    followed = follow(&ddp, &room).await;
                }
            }
            thread = reads.recv(), if reading => {
                let Some(thread) = thread else {
                    reading = false;
                    continue;
                };
                if let Err(error) = ddp.read_thread(thread.as_str()).await {
                    tracing::warn!(%error, %thread, "can't mark the thread read");
                }
            }
            event = events.recv() => {
                let Some(event) = event else { break };
                if pushed.send(pushed_from(event)).is_err() {
                    break;
                }
            }
            () = pushed.closed() => break,
        }
    }
    drop(ddp);
}

/// Subscribes to what the server pushes only for a room it was told about: its deletions and its
/// users' activity. What can't be subscribed to is logged and nothing else.
async fn follow(ddp: &DdpClient, room: &RoomId) -> Vec<SubscriptionId> {
    let mut subscriptions = Vec::new();
    match ddp.subscribe_room_deletions(room.as_str()).await {
        Ok(subscription) => subscriptions.push(subscription),
        Err(error) => tracing::warn!(%error, %room, "deletions won't be pushed"),
    }
    match ddp.subscribe_user_activity(room.as_str()).await {
        Ok(subscription) => subscriptions.push(subscription),
        Err(error) => tracing::warn!(%error, %room, "typing won't be pushed"),
    }
    subscriptions
}

fn pushed_from(event: DdpEvent) -> Pushed {
    match event {
        DdpEvent::Message(message) => Pushed::Message(message_from(message)),
        DdpEvent::SubscriptionChanged(subscription) => Pushed::Subscription(subscription),
        DdpEvent::SubscriptionRemoved { room_id } => Pushed::SubscriptionRemoved(room_id.into()),
        DdpEvent::RoomChanged {
            room_id,
            last_message,
            topic,
        } => Pushed::RoomChanged {
            room: room_id.into(),
            last_message,
            topic,
        },
        DdpEvent::AvatarChanged(rc_api::AvatarOwner::User { username }) => {
            Pushed::AvatarChanged(AvatarOwner::User(username))
        }
        DdpEvent::AvatarChanged(rc_api::AvatarOwner::Room { room_id }) => {
            Pushed::AvatarChanged(AvatarOwner::Room(room_id.into()))
        }
        DdpEvent::MessageDeleted {
            room_id,
            message_id,
        } => Pushed::MessageDeleted {
            room: room_id.into(),
            id: message_id.into(),
        },
        DdpEvent::UserActivity {
            room_id,
            username,
            typing,
            thread_id,
        } => Pushed::Typing {
            room: room_id.into(),
            username,
            typing,
            thread: thread_id.map(Into::into),
        },
    }
}

fn api_credentials(credentials: &Credentials) -> rc_api::Credentials {
    rc_api::Credentials {
        user_id: credentials.user_id.as_str().to_owned(),
        token: credentials.token.clone(),
    }
}

fn credentials_from(credentials: rc_api::Credentials) -> Credentials {
    Credentials {
        user_id: credentials.user_id.into(),
        token: credentials.token,
    }
}

fn user_from(user: rc_api::User) -> User {
    User {
        id: user.id.into(),
        username: user.username,
        display_name: user.name,
    }
}

impl From<rc_api::Error> for ApiError {
    fn from(error: rc_api::Error) -> Self {
        match error {
            rc_api::Error::InvalidUrl(url) => Self::InvalidUrl(url),
            rc_api::Error::Unreachable(source) => Self::Unreachable(chain(&source)),
            rc_api::Error::NotRocketChat => Self::NotRocketChat,
            rc_api::Error::InvalidCredentials => Self::InvalidCredentials,
            rc_api::Error::Unauthorized => Self::Unauthorized,
            rc_api::Error::Server { status, message } => Self::Server { status, message },
            rc_api::Error::UnsupportedRoomType(code) => {
                Self::Unexpected(format!("rooms of type {code:?} are not supported here"))
            }
            rc_api::Error::Decode(source) => Self::Unexpected(chain(&source)),
            rc_api::Error::File(source) => Self::File(source.to_string()),
            rc_api::Error::WebSocket(source) => Self::Unreachable(chain(&source)),
            error @ rc_api::Error::ConnectionClosed => Self::Unreachable(error.to_string()),
            error @ rc_api::Error::Refused { .. } => Self::Unexpected(error.to_string()),
        }
    }
}

/// An error and its sources as one line: "error sending request: connection refused".
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(thread_id: Option<&str>) -> rc_api::Message {
        rc_api::Message {
            id: "m1".into(),
            room_id: "r1".into(),
            ts: "2026-10-20T14:12:48.587Z".parse().unwrap(),
            author: rc_api::Author {
                id: "u1".into(),
                username: "alice".into(),
                name: None,
            },
            text: "hello".into(),
            kind: None,
            thread_id: thread_id.map(str::to_owned),
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
            thread_followers: Vec::new(),
            md: None,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
        }
    }

    // RICH-006
    #[test]
    fn a_deletion_is_pushed_by_room_and_message_id() {
        let deleted = pushed_from(DdpEvent::MessageDeleted {
            room_id: "r1".into(),
            message_id: "m1".into(),
        });
        assert_eq!(
            deleted,
            Pushed::MessageDeleted {
                room: "r1".to_owned().into(),
                id: "m1".to_owned().into(),
            }
        );
    }

    // ACT-006
    #[test]
    fn typing_is_pushed_by_room_username_and_thread() {
        let in_thread = pushed_from(DdpEvent::UserActivity {
            room_id: "r1".into(),
            username: "bob".into(),
            typing: true,
            thread_id: Some("t1".into()),
        });
        assert_eq!(
            in_thread,
            Pushed::Typing {
                room: "r1".to_owned().into(),
                username: "bob".into(),
                typing: true,
                thread: Some("t1".to_owned().into()),
            }
        );
        let stopped = pushed_from(DdpEvent::UserActivity {
            room_id: "r1".into(),
            username: "bob".into(),
            typing: false,
            thread_id: None,
        });
        assert_eq!(
            stopped,
            Pushed::Typing {
                room: "r1".to_owned().into(),
                username: "bob".into(),
                typing: false,
                thread: None,
            }
        );
    }

    // RT-001
    #[test]
    fn pushed_messages_include_the_thread_replies_the_room_does_not_show() {
        let shown = pushed_from(DdpEvent::Message(entry(None)));
        assert!(matches!(shown, Pushed::Message(m) if m.id.as_str() == "m1"));
        let reply = pushed_from(DdpEvent::Message(entry(Some("t"))));
        assert!(matches!(reply, Pushed::Message(m) if !m.shows_in_room()));
    }

    // ROOM-005
    #[test]
    fn room_activity_is_pushed_by_room_id() {
        let at: jiff::Timestamp = "2026-10-20T14:12:48.587Z".parse().unwrap();
        let activity = pushed_from(DdpEvent::RoomChanged {
            room_id: "r1".into(),
            last_message: Some(at),
            topic: None,
        });
        assert_eq!(
            activity,
            Pushed::RoomChanged {
                room: "r1".to_owned().into(),
                last_message: Some(at),
                topic: None
            }
        );
    }

    // RICH-004
    #[test]
    fn an_avatar_change_is_pushed_by_username_or_room_id() {
        let user = pushed_from(DdpEvent::AvatarChanged(rc_api::AvatarOwner::User {
            username: "alice".into(),
        }));
        assert_eq!(
            user,
            Pushed::AvatarChanged(AvatarOwner::User("alice".into()))
        );
        let room = pushed_from(DdpEvent::AvatarChanged(rc_api::AvatarOwner::Room {
            room_id: "r1".into(),
        }));
        assert_eq!(
            room,
            Pushed::AvatarChanged(AvatarOwner::Room("r1".to_owned().into()))
        );
    }

    // RT-002
    #[test]
    fn a_removed_subscription_is_pushed_by_room_id() {
        let removed = pushed_from(DdpEvent::SubscriptionRemoved {
            room_id: "r1".into(),
        });
        assert_eq!(removed, Pushed::SubscriptionRemoved("r1".to_owned().into()));
    }
}
