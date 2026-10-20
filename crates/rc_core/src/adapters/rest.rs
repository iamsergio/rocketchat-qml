// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The server port over the Rocket.Chat REST API.

use rc_api::{DdpClient, DdpEvent, DdpEvents, RestClient, ServerVersion};
use secrecy::SecretString;
use tokio::sync::mpsc;

use crate::domain::{Credentials, Message, MessageId, RoomId, RoomKind, User, message_from};
use crate::ports::{ApiError, Connector, Page, Pushed, PushedEvents, ServerApi};

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
            messages: page.messages.into_iter().filter_map(message_from).collect(),
            reached_start: page.reached_start,
        })
    }

    async fn send_message(
        &self,
        credentials: &Credentials,
        room: &RoomId,
        id: &MessageId,
        text: &str,
    ) -> Result<Message, ApiError> {
        let session = self.client.session(api_credentials(credentials));
        let stored = session
            .send_message(room.as_str(), id.as_str(), text)
            .await?;
        message_from(stored)
            .ok_or_else(|| ApiError::Unexpected("the server stored the message as a reply".into()))
    }

    async fn mark_read(&self, credentials: &Credentials, room: &RoomId) -> Result<(), ApiError> {
        let session = self.client.session(api_credentials(credentials));
        Ok(session.mark_read(room.as_str()).await?)
    }

    async fn realtime(&self, credentials: &Credentials) -> Result<PushedEvents, ApiError> {
        let (ddp, events) = DdpClient::connect(&self.server).await?;
        ddp.login(&api_credentials(credentials)).await?;
        ddp.subscribe_my_messages().await?;
        ddp.subscribe_subscription_changes(credentials.user_id.as_str())
            .await?;
        ddp.subscribe_room_activity(credentials.user_id.as_str())
            .await?;
        let (pushed, receiver) = mpsc::unbounded_channel();
        tokio::spawn(forward(ddp, events, pushed));
        Ok(receiver)
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
/// so ending it closes the connection.
async fn forward(ddp: DdpClient, mut events: DdpEvents, pushed: mpsc::UnboundedSender<Pushed>) {
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                if let Some(event) = pushed_from(event)
                    && pushed.send(event).is_err()
                {
                    break;
                }
            }
            () = pushed.closed() => break,
        }
    }
    drop(ddp);
}

/// `None` for a message the timeline doesn't show, as in the history.
fn pushed_from(event: DdpEvent) -> Option<Pushed> {
    Some(match event {
        DdpEvent::Message(message) => Pushed::Message(message_from(message)?),
        DdpEvent::SubscriptionChanged(subscription) => Pushed::Subscription(subscription),
        DdpEvent::SubscriptionRemoved { room_id } => Pushed::SubscriptionRemoved(room_id.into()),
        DdpEvent::RoomActivity {
            room_id,
            last_message,
        } => Pushed::Activity {
            room: room_id.into(),
            at: last_message,
        },
    })
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
                Self::Unexpected(format!("rooms of type {code:?} have no history"))
            }
            rc_api::Error::Decode(source) => Self::Unexpected(chain(&source)),
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
        }
    }

    // RT-001
    #[test]
    fn pushed_messages_are_filtered_like_the_history() {
        let shown = pushed_from(DdpEvent::Message(entry(None)));
        assert!(matches!(shown, Some(Pushed::Message(m)) if m.id.as_str() == "m1"));
        assert_eq!(pushed_from(DdpEvent::Message(entry(Some("t")))), None);
    }

    // ROOM-005
    #[test]
    fn room_activity_is_pushed_by_room_id() {
        let at: jiff::Timestamp = "2026-10-20T14:12:48.587Z".parse().unwrap();
        let activity = pushed_from(DdpEvent::RoomActivity {
            room_id: "r1".into(),
            last_message: at,
        });
        assert_eq!(
            activity,
            Some(Pushed::Activity {
                room: "r1".to_owned().into(),
                at
            })
        );
    }

    // RT-002
    #[test]
    fn a_removed_subscription_is_pushed_by_room_id() {
        let removed = pushed_from(DdpEvent::SubscriptionRemoved {
            room_id: "r1".into(),
        });
        assert_eq!(
            removed,
            Some(Pushed::SubscriptionRemoved("r1".to_owned().into()))
        );
    }
}
