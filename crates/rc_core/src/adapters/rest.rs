// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The server port over the Rocket.Chat REST API.

use rc_api::{RestClient, ServerVersion};
use secrecy::SecretString;

use crate::domain::{Credentials, User};
use crate::ports::{ApiError, Connector, ServerApi};

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

    async fn logout(&self, credentials: &Credentials) -> Result<(), ApiError> {
        let credentials = rc_api::Credentials {
            user_id: credentials.user_id.as_str().to_owned(),
            token: credentials.token.clone(),
        };
        Ok(self.client.session(credentials).logout().await?)
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
            rc_api::Error::Decode(source) => Self::Unexpected(chain(&source)),
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
