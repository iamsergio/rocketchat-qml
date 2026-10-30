// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::path::Path;
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Method, StatusCode, Url};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use tokio::io::AsyncReadExt as _;

use crate::error::Error;
use crate::types::{
    Credentials, CustomEmoji, DirectoryChannel, DirectoryPage, DirectoryUser, Download,
    HistoryPage, LoginResult, MemberPage, Message, MessageChanges, MessageSettings, RoomInfo,
    RoomType, ServerInfo, Subscription, ThreadPage, ThreadsKind, ThreadsPage, User,
};
use crate::wire;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// How long an idle connection is kept for the next request. Shorter than the 5 s after which
/// Node's HTTP server, which Rocket.Chat runs on, closes one: a request sent on a connection the
/// server is closing fails with an incomplete message.
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_REDIRECTS: usize = 10;
/// How long an upload may take as a whole. The client's request timeout would end a big file on a
/// slow line; a stalled one still ends, by the connection's own failure.
const UPLOAD_TIMEOUT: Duration = Duration::from_hours(1);
/// How much of a file is read and sent at a time.
const UPLOAD_CHUNK: usize = 64 * 1024;

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
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
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

    /// The server's own URL, its `Site_Url` setting without a trailing slash (`GET
    /// /api/v1/settings.public`, which needs no login). `None` if the server has none or it is
    /// not a text. The links of quotes must begin with it, or the server makes no quote of them.
    ///
    /// # Errors
    ///
    /// [`Error::Unreachable`] if the server can't be reached, [`Error::Server`] if it refuses or
    /// answers with an error status, [`Error::Decode`] if the answer isn't Rocket.Chat's.
    pub async fn site_url(&self) -> Result<Option<String>, Error> {
        let path = "api/v1/settings.public?_id=Site_Url";
        let (status, body) = self.send(Method::GET, path, None, None).await?;
        if !status.is_success() {
            return Err(server_error(status, &body));
        }
        wire::parse_site_url(&body).map_err(Error::Decode)
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
        let (status, _, body) = self
            .send_to(method, url, credentials, false, payload)
            .await?;
        Ok((status, body))
    }

    /// [`send`](Self::send) to a URL, which also returns the `Content-Type` of the answer. With
    /// `cookie` the login also goes as the cookies the web client has, for the routes that read it
    /// from there.
    async fn send_to(
        &self,
        method: Method,
        url: Url,
        credentials: Option<&Credentials>,
        cookie: bool,
        payload: Option<&serde_json::Value>,
    ) -> Result<(StatusCode, Option<String>, Vec<u8>), Error> {
        let path = url.path().to_owned();
        let path = path.as_str();
        let mut request = self.http.request(method.clone(), url);
        if let Some(credentials) = credentials {
            request = request
                .header(USER_ID_HEADER, sensitive(&credentials.user_id)?)
                .header(
                    AUTH_TOKEN_HEADER,
                    sensitive(credentials.token.expose_secret())?,
                );
            if cookie {
                let login = format!(
                    "rc_uid={}; rc_token={}",
                    credentials.user_id,
                    credentials.token.expose_secret()
                );
                request = request.header(reqwest::header::COOKIE, sensitive(&login)?);
            }
        }
        if let Some(payload) = payload {
            request = request.json(payload);
        }
        let response = request.send().await.map_err(Error::Unreachable)?;
        let status = response.status();
        tracing::debug!(%method, path, status = status.as_u16(), "REST request");
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.bytes().await.map_err(Error::Unreachable)?;
        Ok((status, content_type, body.to_vec()))
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
        let mut rooms = wire::parse_rooms(&rooms).map_err(Error::Decode)?;
        for subscription in &mut subscriptions {
            if let Some(room) = rooms.remove(&subscription.room_id) {
                subscription.last_message = room.lm;
                subscription.avatar_etag = room.avatar_etag;
                subscription.topic = room.topic.unwrap_or_default();
            }
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

    /// What the server allows users to do with their own messages, and whether it takes uploads
    /// (`GET /api/v1/settings.public` for the four message settings and the two upload ones). The
    /// settings are public, but the call carries the login headers like the others.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, or any of the other errors.
    pub async fn message_settings(&self) -> Result<MessageSettings, Error> {
        let path = "api/v1/settings.public?_id=Message_AllowEditing,\
            Message_AllowEditing_BlockEditInMinutes,Message_AllowDeleting,\
            Message_AllowDeleting_BlockDeleteInMinutes,FileUpload_Enabled,FileUpload_MaxFileSize";
        let body = self.call(Method::GET, path).await?;
        wire::parse_message_settings(&body).map_err(Error::Decode)
    }

    /// The etag of the user's own avatar (`GET /api/v1/users.info`), which says that they have
    /// one: `None` for a user with the avatar the server generates. The server needs a moment to
    /// show a new avatar here.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if there is
    /// no such user, or any of the other errors.
    pub async fn user_avatar_etag(&self, username: &str) -> Result<Option<String>, Error> {
        let path = format!("api/v1/users.info?{}", query("username", username)?);
        let body = self.call(Method::GET, &path).await?;
        wire::parse_user_avatar_etag(&body).map_err(Error::Decode)
    }

    /// The etag of the room's own avatar (`GET /api/v1/rooms.info`): `None` for a room with the
    /// avatar the server generates.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist or isn't visible to the user, or any of the other errors.
    pub async fn room_avatar_etag(&self, room_id: &str) -> Result<Option<String>, Error> {
        let path = format!("api/v1/rooms.info?{}", query("roomId", room_id)?);
        let body = self.call(Method::GET, &path).await?;
        wire::parse_room_avatar_etag(&body).map_err(Error::Decode)
    }

    /// The avatar of a user (`GET /avatar/<username>`), an image the server generates from the
    /// initials when the user has none. Not under `/api/v1`, and answered with the image itself.
    /// The route reads the login from the cookies, not the headers, and without them answers with
    /// the generated avatar even for a user who has uploaded one, so the login goes as both.
    /// The server answers for any name, so there is no 404 for an unknown user.
    ///
    /// # Errors
    ///
    /// [`Error::Server`] on any non-2xx answer, including a redirect that leaves the server (not
    /// followed: the login headers must not go there), or any of the other errors.
    pub async fn user_avatar(&self, username: &str) -> Result<Download, Error> {
        self.download(&["avatar", username]).await
    }

    /// The avatar of a channel or group (`GET /avatar/room/<room id>`). For a direct message the
    /// server draws an initial taken from the room id, so ask for the other user's avatar
    /// ([`user_avatar`](Self::user_avatar)) instead.
    ///
    /// # Errors
    ///
    /// As [`user_avatar`](Self::user_avatar).
    pub async fn room_avatar(&self, room_id: &str) -> Result<Download, Error> {
        self.download(&["avatar", "room", room_id]).await
    }

    /// A file uploaded to a room (`GET /file-upload/...`), given the path the message carries:
    /// an attachment's link or image. The server wants the login headers for it.
    ///
    /// Only paths below `/file-upload/` are accepted, and they are taken as the server's own, so
    /// the login headers can't be sent anywhere else. Each segment is percent-decoded and then
    /// escaped again by the request.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidUrl`] for any other path (a full URL, another place on the server, `..`,
    /// a query), or as [`user_avatar`](Self::user_avatar).
    pub async fn file(&self, path: &str) -> Result<Download, Error> {
        let segments = file_segments(path).ok_or_else(|| Error::InvalidUrl(path.to_owned()))?;
        let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
        self.download(&segments).await
    }

    /// The image of a custom emoji (`GET /emoji-custom/<name>.<extension>`), as the emoji list
    /// gives its name and extension. The route needs no login, but the headers go along like for
    /// the other downloads, so a server that locks its files behind them still answers. For a name
    /// it doesn't have, a server answers a placeholder image rather than refusing.
    ///
    /// # Errors
    ///
    /// As [`user_avatar`](Self::user_avatar).
    pub async fn emoji_image(&self, name: &str, extension: &str) -> Result<Download, Error> {
        self.download(&["emoji-custom", &format!("{name}.{extension}")])
            .await
    }

    /// Gets the file at `segments` below the server's base URL. Each segment is escaped, so a name
    /// can't climb out of its place in the path.
    async fn download(&self, segments: &[&str]) -> Result<Download, Error> {
        let mut url = self.client.base.clone();
        url.path_segments_mut()
            .map_err(|()| Error::InvalidUrl(self.client.base.to_string()))?
            .pop_if_empty()
            .extend(segments);
        let (status, content_type, body) = self
            .client
            .send_to(Method::GET, url, Some(&self.credentials), true, None)
            .await?;
        if !status.is_success() {
            return Err(server_error(status, &body));
        }
        Ok(Download {
            bytes: body,
            content_type: content_type.map(|value| media_type(&value)),
        })
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

    /// The messages of a room that match `text` (`GET /api/v1/chat.search`), newest first, `count`
    /// of them after the first `offset`. The server searches whole words, ignoring case, any of
    /// them, and takes `/regex/i` too; it doesn't rank, whatever the `score` it sends. Replies in
    /// a thread and system messages are among them. An empty `text` is refused.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses (a private group the user is not in, an invalid regular expression), or any of the
    /// other errors.
    pub async fn search_messages(
        &self,
        room_id: &str,
        text: &str,
        count: usize,
        offset: usize,
    ) -> Result<Vec<Message>, Error> {
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("roomId", room_id);
            pairs.append_pair("searchText", text);
            pairs.append_pair("count", &count.to_string());
            pairs.append_pair("offset", &offset.to_string());
        }
        let path = format!("api/v1/chat.search?{}", escaped.query().unwrap_or_default());
        let body = self.call(Method::GET, &path).await?;
        wire::parse_search(&body).map_err(Error::Decode)
    }

    /// What happened to a room's messages since `since` (`GET /api/v1/chat.syncMessages`): the
    /// ones sent or changed, whole, and the ids of the ones deleted. It works for every room type.
    /// The server decides by its own clock, so a caller that takes `since` from its clock should
    /// leave a margin; applying a change twice does no harm.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist or isn't visible to the user, or any of the other errors.
    pub async fn sync_messages(
        &self,
        room_id: &str,
        since: jiff::Timestamp,
    ) -> Result<MessageChanges, Error> {
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("roomId", room_id);
            pairs.append_pair("lastUpdate", &since.to_string());
        }
        let path = format!(
            "api/v1/chat.syncMessages?{}",
            escaped.query().unwrap_or_default()
        );
        let body = self.call(Method::GET, &path).await?;
        wire::parse_changes(&body).map_err(Error::Decode)
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
        self.post_message(room_id, id, &payload).await
    }

    /// Posts a text message as a reply in the thread of `thread_id` (`tmid`), and in the channel
    /// too if `also_to_channel` (`tshow`). Otherwise as [`send_message`](Self::send_message). The
    /// server raises the parent's reply count and last reply time.
    ///
    /// # Errors
    ///
    /// As [`send_message`](Self::send_message). The server doesn't check that `thread_id` is a
    /// message of the room, so that is for the caller to see to.
    pub async fn send_thread_reply(
        &self,
        room_id: &str,
        id: &str,
        text: &str,
        thread_id: &str,
        also_to_channel: bool,
    ) -> Result<Message, Error> {
        let payload = json!({ "message": {
            "_id": id, "rid": room_id, "msg": text, "tmid": thread_id, "tshow": also_to_channel,
        } });
        self.post_message(room_id, id, &payload).await
    }

    /// The request of [`send_message`](Self::send_message) and its safety net after a 400.
    async fn post_message(
        &self,
        room_id: &str,
        id: &str,
        payload: &serde_json::Value,
    ) -> Result<Message, Error> {
        let sent = self
            .call_json(Method::POST, "api/v1/chat.sendMessage", payload)
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

    /// Uploads the file at `path` to the room, named `file_name` (`POST /api/v1/rooms.media/<rid>`).
    /// The file is not a message yet: [`confirm_upload`](Self::confirm_upload) posts it, and
    /// returns the message. The file is streamed from disk, never held in memory, and sent as
    /// `application/octet-stream`: the server finds out the type itself. `progress` is called
    /// with the number of bytes handed to the connection so far, as it grows, and reaches the
    /// file's length when the last chunk is. Returns the id of the uploaded file.
    ///
    /// The server refuses a type it blacklists (400, `File type is not accepted`), a room the
    /// user isn't in (`unauthorized`) and an empty request.
    ///
    /// # Errors
    ///
    /// [`Error::File`] if the file can't be opened or read, [`Error::Unauthorized`] if the token is
    /// expired or revoked, [`Error::Server`] if the server refuses the file, or any of the other
    /// errors.
    pub async fn upload_file(
        &self,
        room_id: &str,
        path: &Path,
        file_name: &str,
        progress: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<String, Error> {
        let file = tokio::fs::File::open(path).await.map_err(Error::File)?;
        let length = file.metadata().await.map_err(Error::File)?.len();
        let chunks = futures_util::stream::unfold(
            (file, 0_u64, progress),
            |(mut file, sent, progress)| async move {
                let mut chunk = vec![0; UPLOAD_CHUNK];
                match file.read(&mut chunk).await {
                    Ok(0) => None,
                    Ok(read) => {
                        chunk.truncate(read);
                        let sent = sent + read as u64;
                        progress(sent);
                        Some((Ok(chunk), (file, sent, progress)))
                    }
                    Err(error) => Some((Err(error), (file, sent, progress))),
                }
            },
        );
        let part = reqwest::multipart::Part::stream_with_length(
            reqwest::Body::wrap_stream(chunks),
            length,
        )
        .file_name(file_name.to_owned())
        .mime_str("application/octet-stream")
        .map_err(Error::Unreachable)?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let url = self
            .client
            .base
            .join(&format!("api/v1/rooms.media/{}", segment(room_id)?))
            .map_err(|e| Error::InvalidUrl(e.to_string()))?;
        let request = self
            .client
            .http
            .post(url)
            .timeout(UPLOAD_TIMEOUT)
            .header(USER_ID_HEADER, sensitive(&self.credentials.user_id)?)
            .header(
                AUTH_TOKEN_HEADER,
                sensitive(self.credentials.token.expose_secret())?,
            )
            .multipart(form);
        let response = request.send().await.map_err(Error::Unreachable)?;
        let status = response.status();
        let body = response.bytes().await.map_err(Error::Unreachable)?;
        check(status, &body, || Error::Unauthorized)?;
        wire::parse_uploaded(&body).map_err(Error::Decode)
    }

    /// Posts an uploaded file as a message (`POST /api/v1/rooms.mediaConfirm/<rid>/<fileId>`), with
    /// `text` as its text (may be empty) and, with `thread_id`, as a reply in that thread (never
    /// also shown in the channel: the server refuses `tshow` here). The server picks the message's
    /// id. Confirming a file again answers with the message it made the first time, so a confirm
    /// may be repeated when its answer was lost.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses (`invalid-file` for an id it doesn't know), or any of the other errors.
    pub async fn confirm_upload(
        &self,
        room_id: &str,
        file_id: &str,
        text: &str,
        thread_id: Option<&str>,
    ) -> Result<Message, Error> {
        let mut payload = json!({ "msg": text });
        if let Some(thread_id) = thread_id {
            payload["tmid"] = json!(thread_id);
        }
        let path = format!(
            "api/v1/rooms.mediaConfirm/{}/{}",
            segment(room_id)?,
            segment(file_id)?
        );
        let body = self.call_json(Method::POST, &path, &payload).await?;
        wire::parse_message(&body).map_err(Error::Decode)
    }

    /// A page of the replies of a thread, newest first (`GET /api/v1/chat.getThreadMessages`),
    /// starting `offset` replies from the newest. The parent is not among them. The page has at
    /// most `count` replies; [`ThreadPage::reached_start`] says when there are no more. A reply
    /// that arrives meanwhile shifts the later pages by one, which is for the caller to
    /// de-duplicate.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the thread
    /// doesn't exist or isn't visible to the user, or any of the other errors.
    pub async fn thread_messages(
        &self,
        thread_id: &str,
        count: usize,
        offset: usize,
    ) -> Result<ThreadPage, Error> {
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("tmid", thread_id);
            pairs.append_pair("count", &count.to_string());
            pairs.append_pair("offset", &offset.to_string());
            // Without it the server sends the oldest reply first.
            pairs.append_pair("sort", r#"{"ts":-1}"#);
        }
        let path = format!(
            "api/v1/chat.getThreadMessages?{}",
            escaped.query().unwrap_or_default()
        );
        let body = self.call(Method::GET, &path).await?;
        wire::parse_thread_messages(&body, offset).map_err(Error::Decode)
    }

    /// A page of the threads of a room (`GET /api/v1/chat.getThreadsList`), as their parent
    /// messages, the one with the most recent reply first, starting `offset` threads from the
    /// first. The page has at most `count` threads; [`ThreadsPage::reached_end`] says when there
    /// are no more. A thread that gets a reply meanwhile moves to the front and shifts the later
    /// pages, which is for the caller to de-duplicate.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist or isn't visible to the user, or any of the other errors.
    pub async fn threads_list(
        &self,
        room_id: &str,
        kind: ThreadsKind,
        count: usize,
        offset: usize,
    ) -> Result<ThreadsPage, Error> {
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("rid", room_id);
            // The server refuses `all`: every thread is what it lists without a type.
            match kind {
                ThreadsKind::All => {}
                ThreadsKind::Following => {
                    pairs.append_pair("type", "following");
                }
                ThreadsKind::Unread => {
                    pairs.append_pair("type", "unread");
                }
            }
            pairs.append_pair("count", &count.to_string());
            pairs.append_pair("offset", &offset.to_string());
        }
        let path = format!(
            "api/v1/chat.getThreadsList?{}",
            escaped.query().unwrap_or_default()
        );
        let body = self.call(Method::GET, &path).await?;
        wire::parse_threads_list(&body, offset).map_err(Error::Decode)
    }

    /// Replaces the text of a message (`POST /api/v1/chat.update`) and returns the message as the
    /// server stored it, with `editedAt` set.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses the edit (not the user's message, editing not allowed, past the time limit...), or
    /// any of the other errors.
    pub async fn edit_message(
        &self,
        room_id: &str,
        id: &str,
        text: &str,
    ) -> Result<Message, Error> {
        let payload = json!({ "roomId": room_id, "msgId": id, "text": text });
        let body = self
            .call_json(Method::POST, "api/v1/chat.update", &payload)
            .await?;
        wire::parse_message(&body).map_err(Error::Decode)
    }

    /// Deletes a message (`POST /api/v1/chat.delete`).
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses the deletion (not the user's message, deleting not allowed, past the time limit...),
    /// or any of the other errors.
    pub async fn delete_message(&self, room_id: &str, id: &str) -> Result<(), Error> {
        let payload = json!({ "roomId": room_id, "msgId": id });
        self.call_json(Method::POST, "api/v1/chat.delete", &payload)
            .await?;
        Ok(())
    }

    /// Adds the user's reaction to a message, or takes it back (`POST /api/v1/chat.react`).
    /// `emoji` is the shortcode with its colons (`:smile:`). The request always says which of the
    /// two is wanted, never the server's toggle, so repeating it changes nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses the reaction (unknown emoji, no such message, no access to the room...), or any of
    /// the other errors.
    pub async fn react(&self, id: &str, emoji: &str, should_react: bool) -> Result<(), Error> {
        let payload = json!({ "messageId": id, "emoji": emoji, "shouldReact": should_react });
        self.call_json(Method::POST, "api/v1/chat.react", &payload)
            .await?;
        Ok(())
    }

    /// The message with this id as the server stores it (`GET /api/v1/chat.getMessage`), whatever
    /// room it is in, as long as the user may read that room.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if there is
    /// no such message or the user may not read it, or any of the other errors.
    pub async fn message(&self, id: &str) -> Result<Message, Error> {
        let path = format!("api/v1/chat.getMessage?{}", query("msgId", id)?);
        let body = self.call(Method::GET, &path).await?;
        wire::parse_message(&body).map_err(Error::Decode)
    }

    /// The message with this id, or `None` if there is none or it can't be fetched.
    async fn stored_message(&self, id: &str) -> Option<Message> {
        self.message(id).await.ok()
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

    /// Marks the room as unread for the user (`POST /api/v1/subscriptions.unread`): the server
    /// sets the subscription's unread count to 1 and its alert flag, and pushes the change like
    /// any other. A room without messages is refused.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses (no messages to mark unread...), or any of the other errors.
    pub async fn mark_unread(&self, room_id: &str) -> Result<(), Error> {
        let payload = json!({ "roomId": room_id });
        self.call_json(Method::POST, "api/v1/subscriptions.unread", &payload)
            .await?;
        Ok(())
    }

    /// Marks the room as a favorite of the user, or takes it back (`POST /api/v1/rooms.favorite`).
    /// The request says which of the two is wanted, so repeating it changes nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the user
    /// has no subscription to the room, or any of the other errors.
    pub async fn set_favorite(&self, room_id: &str, favorite: bool) -> Result<(), Error> {
        let payload = json!({ "roomId": room_id, "favorite": favorite });
        self.call_json(Method::POST, "api/v1/rooms.favorite", &payload)
            .await?;
        Ok(())
    }

    /// Leaves a channel or a private group (`POST /api/v1/channels.leave` or `groups.leave`,
    /// depending on `kind`). The server removes the subscription and pushes that.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked,
    /// [`Error::UnsupportedRoomType`] for a direct message or any other kind of room,
    /// [`Error::Server`] if the server refuses (the user is the room's last owner, is not in it
    /// any more...), or any of the other errors.
    pub async fn leave(&self, room_id: &str, kind: &RoomType) -> Result<(), Error> {
        let path = match kind {
            RoomType::Channel => "api/v1/channels.leave",
            RoomType::Private => "api/v1/groups.leave",
            RoomType::Direct => return Err(Error::UnsupportedRoomType("d".into())),
            RoomType::Other(t) => return Err(Error::UnsupportedRoomType(t.clone())),
        };
        let payload = json!({ "roomId": room_id });
        self.call_json(Method::POST, path, &payload).await?;
        Ok(())
    }

    /// The users of the server's directory whose name or username match `text` (`GET
    /// /api/v1/directory` with `type=users`), at most `count`. `text` and `type` are parameters of
    /// their own: the older `query` parameter, a JSON object that holds them, is ignored by
    /// Rocket.Chat 8 and the server then lists everybody. The page's `total` is how many match.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, or any of the other errors.
    pub async fn directory_users(
        &self,
        text: &str,
        count: usize,
    ) -> Result<DirectoryPage<DirectoryUser>, Error> {
        let body = self.directory("users", text, count, None).await?;
        wire::parse_directory_users(&body).map_err(Error::Decode)
    }

    /// The public channels of the server's directory whose name matches `text`, the ones with the
    /// most members first, at most `count` (see [`directory_users`](Self::directory_users)). The
    /// directory doesn't say whether the user is in a channel, and lists no private group.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, or any of the other errors.
    pub async fn directory_channels(
        &self,
        text: &str,
        count: usize,
    ) -> Result<DirectoryPage<DirectoryChannel>, Error> {
        let sort = json!({ "usersCount": -1 }).to_string();
        let body = self.directory("channels", text, count, Some(&sort)).await?;
        wire::parse_directory_channels(&body).map_err(Error::Decode)
    }

    /// The members of a channel or private group whose username or name contain `filter`, case
    /// insensitively (`GET /api/v1/rooms.membersOrderedByRole`), at most `count`, owners first,
    /// then moderators, then by name. An empty `filter` lists them all. The page's `total` is how
    /// many match. The server refuses a direct message (`error-room-type-not-supported`).
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses, or any of the other errors.
    pub async fn room_members(
        &self,
        room_id: &str,
        filter: &str,
        count: usize,
    ) -> Result<MemberPage, Error> {
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("roomId", room_id);
            pairs.append_pair("filter", filter);
            pairs.append_pair("count", &count.to_string());
        }
        let path = format!(
            "api/v1/rooms.membersOrderedByRole?{}",
            escaped.query().unwrap_or_default()
        );
        let body = self.call(Method::GET, &path).await?;
        wire::parse_room_members(&body).map_err(Error::Decode)
    }

    async fn directory(
        &self,
        kind: &str,
        text: &str,
        count: usize,
        sort: Option<&str>,
    ) -> Result<Vec<u8>, Error> {
        // Only a URL to escape the query with; the host is never used.
        let mut escaped =
            Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = escaped.query_pairs_mut();
            pairs.append_pair("text", text);
            pairs.append_pair("type", kind);
            pairs.append_pair("workspace", "local");
            pairs.append_pair("count", &count.to_string());
            pairs.append_pair("offset", "0");
            if let Some(sort) = sort {
                pairs.append_pair("sort", sort);
            }
        }
        let path = format!("api/v1/directory?{}", escaped.query().unwrap_or_default());
        self.call(Method::GET, &path).await
    }

    /// Opens the direct message with `username` (`POST /api/v1/im.create`), which the server
    /// creates the first time and finds after that, and shows again in the user's rooms if they
    /// had closed it. Returns the room's id. Careful: the server doesn't check that the user
    /// exists, and makes an empty direct message with a name nobody has, so only ask for
    /// usernames that were found.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the server
    /// refuses, or any of the other errors.
    pub async fn create_direct(&self, username: &str) -> Result<String, Error> {
        let payload = json!({ "username": username });
        let body = self
            .call_json(Method::POST, "api/v1/im.create", &payload)
            .await?;
        wire::parse_direct_created(&body).map_err(Error::Decode)
    }

    /// Joins a public channel (`POST /api/v1/channels.join`). Joining one the user is in already
    /// succeeds. The server then pushes the new subscription.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist or can't be joined, or any of the other errors.
    pub async fn join_channel(&self, room_id: &str) -> Result<(), Error> {
        let payload = json!({ "roomId": room_id });
        self.call_json(Method::POST, "api/v1/channels.join", &payload)
            .await?;
        Ok(())
    }

    /// A room the user can see (`GET /api/v1/rooms.info`): any public channel, in or not.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist or isn't visible to the user, or any of the other errors.
    pub async fn room_info(&self, room_id: &str) -> Result<RoomInfo, Error> {
        let path = format!("api/v1/rooms.info?{}", query("roomId", room_id)?);
        let body = self.call(Method::GET, &path).await?;
        wire::parse_room_info(&body).map_err(Error::Decode)
    }

    /// The user's subscription to one room (`GET /api/v1/subscriptions.getOne`), `None` if the
    /// user isn't in it, filled in with the room's last message time, avatar and topic like
    /// [`subscriptions`](Self::subscriptions) does (from `rooms.info`).
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::Server`] if the room
    /// doesn't exist, or any of the other errors.
    pub async fn subscription(&self, room_id: &str) -> Result<Option<Subscription>, Error> {
        let path = format!("api/v1/subscriptions.getOne?{}", query("roomId", room_id)?);
        let body = self.call(Method::GET, &path).await?;
        let Some(mut subscription) = wire::parse_subscription(&body).map_err(Error::Decode)? else {
            return Ok(None);
        };
        let info = self.room_info(room_id).await?;
        subscription.last_message = info.last_message;
        subscription.avatar_etag = info.avatar_etag;
        subscription.topic = info.topic;
        Ok(Some(subscription))
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

/// An id as one path segment, escaped.
fn segment(id: &str) -> Result<String, Error> {
    let mut url = Url::parse("http://x/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
    url.path_segments_mut()
        .map_err(|()| Error::InvalidUrl(id.to_owned()))?
        .push(id);
    Ok(url.path().trim_start_matches('/').to_owned())
}

/// `name=value` as a query string, escaped.
fn query(name: &str, value: &str) -> Result<String, Error> {
    // Only a URL to escape the query with; the host is never used.
    let mut escaped = Url::parse("http://unused/").map_err(|e| Error::InvalidUrl(e.to_string()))?;
    escaped.query_pairs_mut().append_pair(name, value);
    Ok(escaped.query().unwrap_or_default().to_owned())
}

/// The decoded segments of a server path below `/file-upload/`, or `None` if it is anything else:
/// no leading `/file-upload/`, an empty, `.` or `..` segment (also when percent-encoded), a
/// segment that decodes to something that isn't UTF-8 or holds a `/`, or a query or fragment.
fn file_segments(path: &str) -> Option<Vec<String>> {
    let rest = path.strip_prefix("/file-upload/")?;
    if rest.contains(['?', '#', '\\']) {
        return None;
    }
    let mut segments = vec!["file-upload".to_owned()];
    for raw in rest.split('/') {
        let segment = percent_decode(raw)?;
        if matches!(segment.as_str(), "" | "." | "..") || segment.contains('/') {
            return None;
        }
        segments.push(segment);
    }
    Some(segments)
}

/// `%XX` escapes decoded. `None` for a broken escape or a result that isn't UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The media type of a `Content-Type` value, without parameters, in lower case.
fn media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
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

    // RICH-005
    #[test]
    fn file_paths_are_split_and_decoded() {
        assert_eq!(
            file_segments("/file-upload/6ad7f2c6/my%20big%C3%A9.png"),
            Some(vec![
                "file-upload".to_owned(),
                "6ad7f2c6".to_owned(),
                "my bigé.png".to_owned()
            ])
        );
    }

    // RICH-005
    #[test]
    fn only_paths_below_file_upload_are_files() {
        for path in [
            "",
            "/file-upload/",
            "/file-upload",
            "/avatar/alice",
            "file-upload/a/b.png",
            "https://evil.example/file-upload/a/b.png",
            "//evil.example/file-upload/a/b.png",
            "/file-upload/../api/v1/me",
            "/file-upload/%2e%2e/api",
            "/file-upload/a//b.png",
            "/file-upload/a%2Fb.png",
            "/file-upload/a/b.png?token=1",
            "/file-upload/a/b.png#frag",
            "/file-upload/a%/b.png",
            "/file-upload/a/%FF.png",
        ] {
            assert_eq!(file_segments(path), None, "{path:?}");
        }
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
