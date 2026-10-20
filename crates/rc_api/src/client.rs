// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Method, StatusCode, Url};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::error::Error;
use crate::types::{
    Credentials, CustomEmoji, HistoryPage, LoginResult, Message, RoomType, ServerInfo,
    Subscription, User,
};
use crate::wire;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 10;

const USER_ID_HEADER: HeaderName = HeaderName::from_static("x-user-id");
const AUTH_TOKEN_HEADER: HeaderName = HeaderName::from_static("x-auth-token");

/// Talks to one Rocket.Chat server without being logged in. Cheap to clone.
#[derive(Clone)]
pub struct RestClient {
    http: reqwest::Client,
    base: Url,
}

impl RestClient {
    /// `server` is the server's base URL, e.g. `https://chat.example.com` or
    /// `http://127.0.0.1:3000`, possibly with a path prefix (`https://example.com/chat`). The
    /// scheme is required. TLS is checked against the OS trust store.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidUrl`] if `server` isn't an `http` or `https` URL with a host.
    pub fn new(server: &str) -> Result<Self, Error> {
        let base = base_url(server)?;
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(redirect_policy())
            .build()
            .map_err(Error::Unreachable)?;
        Ok(Self { http, base })
    }

    /// The base URL, normalized: it always ends with `/`.
    #[must_use]
    pub fn server_url(&self) -> &Url {
        &self.base
    }

    /// Asks the server for its version (`GET /api/info`, which needs no login).
    ///
    /// # Errors
    ///
    /// [`Error::Unreachable`] if the server can't be reached, [`Error::NotRocketChat`] if it
    /// answers with something that isn't Rocket.Chat's info, [`Error::Server`] on a 5xx.
    pub async fn info(&self) -> Result<ServerInfo, Error> {
        let (status, body) = self.send(Method::GET, "api/info", None, None).await?;
        if status.is_server_error() {
            return Err(server_error(status, &body));
        }
        if !status.is_success() {
            return Err(Error::NotRocketChat);
        }
        wire::parse_info(&body).ok_or(Error::NotRocketChat)
    }

    /// Logs in with a username (or email) and password (`POST /api/v1/login`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidCredentials`] if the server refuses them, or any of the other errors.
    pub async fn login(
        &self,
        username: &str,
        password: &SecretString,
    ) -> Result<LoginResult, Error> {
        let payload = json!({ "user": username, "password": password.expose_secret() });
        let (status, body) = self
            .send(Method::POST, "api/v1/login", None, Some(&payload))
            .await?;
        check(status, &body, || Error::InvalidCredentials)?;
        wire::parse_login(&body).map_err(Error::Decode)
    }

    /// For resuming a stored session, and for what [`login`](Self::login) returned.
    #[must_use]
    pub fn session(&self, credentials: Credentials) -> RestSession {
        RestSession {
            client: self.clone(),
            credentials,
        }
    }

    /// Sends one request and returns the status and the whole body, whatever the status is.
    async fn send(
        &self,
        method: Method,
        path: &str,
        credentials: Option<&Credentials>,
        payload: Option<&serde_json::Value>,
    ) -> Result<(StatusCode, Vec<u8>), Error> {
        let url = self
            .base
            .join(path)
            .map_err(|e| Error::InvalidUrl(format!("{path}: {e}")))?;
        let mut request = self.http.request(method.clone(), url);
        if let Some(credentials) = credentials {
            request = request
                .header(USER_ID_HEADER, sensitive(&credentials.user_id)?)
                .header(
                    AUTH_TOKEN_HEADER,
                    sensitive(credentials.token.expose_secret())?,
                );
        }
        if let Some(payload) = payload {
            request = request.json(payload);
        }
        let response = request.send().await.map_err(Error::Unreachable)?;
        let status = response.status();
        tracing::debug!(%method, path, status = status.as_u16(), "REST request");
        let body = response.bytes().await.map_err(Error::Unreachable)?;
        Ok((status, body.to_vec()))
    }
}

/// A logged-in view of the server. Cheap to clone.
#[derive(Clone)]
pub struct RestSession {
    client: RestClient,
    credentials: Credentials,
}

impl RestSession {
    #[must_use]
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// The logged-in user (`GET /api/v1/me`). Also what to call to find out whether a stored
    /// session still works.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, or any of the other errors.
    pub async fn me(&self) -> Result<User, Error> {
        let body = self.call(Method::GET, "api/v1/me").await?;
        wire::parse_user(&body).map_err(Error::Decode)
    }

    /// The rooms the user has joined (`GET /api/v1/subscriptions.get`), each with the time of its
    /// last message ([`Subscription::last_message`]), which comes from a second request
    /// (`GET /api/v1/rooms.get`): the subscriptions don't carry it.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, or any of the other errors.
    pub async fn subscriptions(&self) -> Result<Vec<Subscription>, Error> {
        let (subscriptions, rooms) = tokio::try_join!(
            self.call(Method::GET, "api/v1/subscriptions.get"),
            self.call(Method::GET, "api/v1/rooms.get"),
        )?;
        let mut subscriptions = wire::parse_subscriptions(&subscriptions).map_err(Error::Decode)?;
        let activity = wire::parse_room_activity(&rooms).map_err(Error::Decode)?;
        for subscription in &mut subscriptions {
            subscription.last_message = activity.get(&subscription.room_id).copied();
        }
        Ok(subscriptions)
    }

    /// The server's custom emoji (`GET /api/v1/emoji-custom.list`).
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, or any of the other errors.
    pub async fn custom_emoji(&self) -> Result<Vec<CustomEmoji>, Error> {
        let body = self.call(Method::GET, "api/v1/emoji-custom.list").await?;
        wire::parse_custom_emoji(&body).map_err(Error::Decode)
    }

    /// A page of a room's history, newest message first (`channels.history`, `groups.history` or
    /// `im.history`, by room type). Without `latest` it starts at the newest message; with it, at
    /// the newest one sent at or before that instant. That is inclusive, so the message at
    /// `latest` comes back again: pass the time of the oldest message already held, and the
    /// messages that share its millisecond can't be skipped, which is for the caller to
    /// de-duplicate. The page has at most `count` messages; [`HistoryPage::reached_start`] says
    /// when there are no more.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedRoomType`] for a room type that has no history call,
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist or isn't visible to the user, or any of the other errors.
    pub async fn history(
        &self,
        room_id: &str,
        room_type: &RoomType,
        latest: Option<jiff::Timestamp>,
        count: usize,
    ) -> Result<HistoryPage, Error> {
        let endpoint = match room_type {
            RoomType::Channel => "channels.history",
            RoomType::Private => "groups.history",
            RoomType::Direct => "im.history",
            RoomType::Other(code) => return Err(Error::UnsupportedRoomType(code.clone())),
        };
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("roomId", room_id);
            pairs.append_pair("count", &count.to_string());
            if let Some(latest) = latest {
                pairs.append_pair("latest", &latest.to_string());
                pairs.append_pair("inclusive", "true");
            }
        }
        let path = format!("api/v1/{endpoint}?{}", escaped.query().unwrap_or_default());
        let body = self.call(Method::GET, &path).await?;
        wire::parse_history(&body, count).map_err(Error::Decode)
    }

    /// Posts a text message under an id the caller picked (`POST /api/v1/chat.sendMessage`), and
    /// returns it as the server stored it, with the server's time. The id is what makes a retry
    /// safe: the server stores a message once per id.
    ///
    /// Sending an id the server has already stored is refused with a 400 whose text is an
    /// internal error (`Cannot read properties of undefined (reading 'starred')`), not a code to
    /// recognise. So after any 400 the message is looked up by id, and if it is there, in this
    /// room and from this user, the first attempt got through and its message is the answer.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses the message (no such room, not allowed to post...), or any of the other errors.
    pub async fn send_message(
        &self,
        room_id: &str,
        id: &str,
        text: &str,
    ) -> Result<Message, Error> {
        let payload = json!({ "message": { "_id": id, "rid": room_id, "msg": text } });
        let sent = self
            .call_json(Method::POST, "api/v1/chat.sendMessage", &payload)
            .await
            .and_then(|body| wire::parse_message(&body).map_err(Error::Decode));
        match sent {
            Err(error @ Error::Server { status: 400, .. }) => match self.stored_message(id).await {
                Some(stored)
                    if stored.room_id == room_id
                        && stored.author.id == self.credentials.user_id =>
                {
                    Ok(stored)
                }
                _ => Err(error),
            },
            other => other,
        }
    }

    /// The message with this id, or `None` if there is none or it can't be fetched.
    async fn stored_message(&self, id: &str) -> Option<Message> {
        let mut escaped = Url::parse("http://unused/").ok()?;
        escaped.query_pairs_mut().append_pair("msgId", id);
        let path = format!("api/v1/chat.getMessage?{}", escaped.query()?);
        let body = self.call(Method::GET, &path).await.ok()?;
        wire::parse_message(&body).ok()
    }

    /// Marks the room read for the user (`POST /api/v1/subscriptions.read`): the server zeroes the
    /// unread and mention counts of the subscription, and pushes the change like any other.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the user
    /// has no subscription to the room, or any of the other errors.
    pub async fn mark_read(&self, room_id: &str) -> Result<(), Error> {
        let payload = json!({ "rid": room_id });
        self.call_json(Method::POST, "api/v1/subscriptions.read", &payload)
            .await?;
        Ok(())
    }

    /// Ends the session on the server (`POST /api/v1/logout`), which revokes the token. Servers
    /// answer with an empty body or with a JSON one; both are fine.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token was already invalid, or any of the other errors.
    pub async fn logout(&self) -> Result<(), Error> {
        self.call(Method::POST, "api/v1/logout").await?;
        Ok(())
    }

    async fn call(&self, method: Method, path: &str) -> Result<Vec<u8>, Error> {
        self.request(method, path, None).await
    }

    async fn call_json(
        &self,
        method: Method,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<Vec<u8>, Error> {
        self.request(method, path, Some(payload)).await
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        payload: Option<&serde_json::Value>,
    ) -> Result<Vec<u8>, Error> {
        let (status, body) = self
            .client
            .send(method, path, Some(&self.credentials), payload)
            .await?;
        check(status, &body, || Error::Unauthorized)?;
        Ok(body)
    }
}

/// `server` as an `http` or `https` base URL that ends with `/`, without query or fragment.
pub(crate) fn base_url(server: &str) -> Result<Url, Error> {
    let mut base = Url::parse(server).map_err(|e| Error::InvalidUrl(format!("{server}: {e}")))?;
    if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() {
        return Err(Error::InvalidUrl(server.to_owned()));
    }
    base.set_query(None);
    base.set_fragment(None);
    // Url::join replaces the last path segment unless the path ends with a slash.
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    Ok(base)
}

/// reqwest only strips the standard `Authorization`-like headers on a redirect, not our
/// `X-User-Id` and `X-Auth-Token`, so we don't follow redirects that leave the server: only the
/// same origin, or an http to https upgrade on the same host.
fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let next = attempt.url();
        let follow = attempt.previous().len() < MAX_REDIRECTS
            && attempt.previous().last().is_some_and(|prev| {
                prev.host_str() == next.host_str()
                    && ((prev.scheme() == next.scheme()
                        && prev.port_or_known_default() == next.port_or_known_default())
                        || (prev.scheme() == "http" && next.scheme() == "https"))
            });
        if follow {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

/// Maps non-2xx statuses to errors: 401 to `unauthorized()`, the rest to [`Error::Server`].
fn check(
    status: StatusCode,
    body: &[u8],
    unauthorized: impl FnOnce() -> Error,
) -> Result<(), Error> {
    if status.is_success() {
        Ok(())
    } else if status == StatusCode::UNAUTHORIZED {
        Err(unauthorized())
    } else {
        Err(server_error(status, body))
    }
}

fn server_error(status: StatusCode, body: &[u8]) -> Error {
    let message = serde_json::from_slice::<wire::ErrorBody>(body)
        .ok()
        .and_then(wire::ErrorBody::into_message)
        .or_else(|| status.canonical_reason().map(str::to_owned))
        .unwrap_or_default();
    Error::Server {
        status: status.as_u16(),
        message,
    }
}

/// A header value that `reqwest` keeps out of its `Debug` output. Values that can't be a header
/// can't be valid credentials either.
fn sensitive(value: &str) -> Result<HeaderValue, Error> {
    let mut value = HeaderValue::from_str(value).map_err(|_| Error::Unauthorized)?;
    value.set_sensitive(true);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(server: &str) -> String {
        RestClient::new(server).unwrap().server_url().to_string()
    }

    // AUTH-003
    #[test]
    fn accepts_http_and_https_urls() {
        assert_eq!(url("http://127.0.0.1:3000"), "http://127.0.0.1:3000/");
        assert_eq!(
            url("https://chat.example.com/"),
            "https://chat.example.com/"
        );
    }

    // AUTH-003
    #[test]
    fn keeps_the_path_prefix() {
        assert_eq!(url("https://host/chat"), "https://host/chat/");
        let client = RestClient::new("https://host/chat").unwrap();
        let login = client.server_url().join("api/v1/login").unwrap();
        assert_eq!(login.as_str(), "https://host/chat/api/v1/login");
    }

    // AUTH-003
    #[test]
    fn strips_query_and_fragment() {
        assert_eq!(url("https://host/chat?a=1#frag"), "https://host/chat/");
    }

    // AUTH-003
    #[test]
    fn rejects_bad_urls() {
        for server in ["127.0.0.1:3000", "ftp://host", "", "host", "http://"] {
            assert!(
                matches!(RestClient::new(server), Err(Error::InvalidUrl(_))),
                "{server:?}"
            );
        }
    }
}
