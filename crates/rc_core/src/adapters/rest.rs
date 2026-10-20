// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The server port over the Rocket.Chat REST API.

use rc_api::{RestClient, ServerVersion};
use secrecy::SecretString;

use crate::domain::{Credentials, RoomId, RoomKind, User, message_from};
use crate::ports::{ApiError, Connector, Page, ServerApi};

pub struct RestConnector;

impl Connector for RestConnector {
    type Api = RestApi;

    fn connect(&self, server: &str) -> Result<RestApi, ApiError> {
        Ok(RestApi {
            client: RestClient::new(server)?,
        })
    }
}

#[derive(Clone)]
pub struct RestApi {
    client: RestClient,
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

    async fn logout(&self, credentials: &Credentials) -> Result<(), ApiError> {
        Ok(self
            .client
            .session(api_credentials(credentials))
            .logout()
            .await?)
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
