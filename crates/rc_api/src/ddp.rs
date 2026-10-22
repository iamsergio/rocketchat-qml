// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The realtime connection: Meteor's DDP over a WebSocket, as Rocket.Chat speaks it.
//!
//! [`DdpClient::connect`] opens the connection and spawns a task that owns it. The task matches
//! answers to the calls waiting for them, forwards stream events, answers the server's pings and
//! pings a server that has gone quiet. When the connection ends, for whatever reason, the calls
//! still waiting fail with [`Error::ConnectionClosed`] and the events end. Reconnecting is up to
//! the caller: a new connection, a new login and new subscriptions.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::{self, Message as Frame};
use tokio_tungstenite::{Connector, WebSocketStream, connect_async_tls_with_config};

use crate::client::base_url;
use crate::error::Error;
use crate::types::{Credentials, DdpEvent};
use crate::wire;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the server may stay silent before it is pinged.
const PING_AFTER: Duration = Duration::from_secs(25);
/// How long the server may stay silent after a ping before the connection counts as dead.
const PONG_TIMEOUT: Duration = Duration::from_secs(10);

/// What the server pushes, in the order it sends it. Ends when the connection does.
pub type DdpEvents = mpsc::UnboundedReceiver<DdpEvent>;

/// A subscription made with a `subscribe_*` method that can be ended again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionId(String);

/// One realtime connection to a server. Cheap to clone; the connection closes when the last clone
/// is dropped.
#[derive(Clone)]
pub struct DdpClient {
    commands: mpsc::UnboundedSender<Command>,
}

impl DdpClient {
    /// Connects to `server`, a base URL as [`RestClient::new`](crate::RestClient::new) takes it:
    /// `https://chat.example.com` is reached at `wss://chat.example.com/websocket`, with TLS
    /// checked against the OS trust store. Must be called within a Tokio runtime, where the
    /// connection's task runs.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidUrl`] if `server` isn't an `http` or `https` URL with a host,
    /// [`Error::WebSocket`] if the connection can't be opened in time, [`Error::NotRocketChat`] if
    /// the server doesn't speak DDP.
    pub async fn connect(server: &str) -> Result<(Self, DdpEvents), Error> {
        let url = websocket_url(server)?;
        let connector = Connector::Rustls(tls_config()?);
        let connecting = connect_async_tls_with_config(url.as_str(), None, true, Some(connector));
        let (ws, _) = timeout(CONNECT_TIMEOUT, connecting)
            .await
            .map_err(|_| timed_out())?
            .map_err(websocket_error)?;
        start(ws).await
    }

    /// Logs the connection in with a token from the REST login (`login` with `resume`).
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is expired or revoked, [`Error::ConnectionClosed`] if
    /// the connection ended, or [`Error::Refused`].
    pub async fn login(&self, credentials: &Credentials) -> Result<(), Error> {
        let params = json!([{ "resume": credentials.token.expose_secret() }]);
        self.call("login", params).await?;
        Ok(())
    }

    /// New and edited messages of every room the user is in, as [`DdpEvent::Message`]
    /// (`stream-room-messages` with `__my_messages__`). Needs a login first.
    ///
    /// # Errors
    ///
    /// [`Error::ConnectionClosed`] if the connection ended, or [`Error::Refused`].
    pub async fn subscribe_my_messages(&self) -> Result<(), Error> {
        let params = json!(["__my_messages__", false]);
        self.subscribe("stream-room-messages", params).await?;
        Ok(())
    }

    /// The user's subscriptions as they are added, changed or removed, as
    /// [`DdpEvent::SubscriptionChanged`] and [`DdpEvent::SubscriptionRemoved`]
    /// (`stream-notify-user` with `<user id>/subscriptions-changed`). Needs a login as `user_id`
    /// first.
    ///
    /// # Errors
    ///
    /// [`Error::ConnectionClosed`] if the connection ended, or [`Error::Refused`].
    pub async fn subscribe_subscription_changes(&self, user_id: &str) -> Result<(), Error> {
        let params = json!([format!("{user_id}/subscriptions-changed"), false]);
        self.subscribe("stream-notify-user", params).await?;
        Ok(())
    }

    /// When the user's rooms have new messages, as [`DdpEvent::RoomChanged`]
    /// (`stream-notify-user` with `<user id>/rooms-changed`). Needs a login as `user_id` first.
    ///
    /// # Errors
    ///
    /// [`Error::ConnectionClosed`] if the connection ended, or [`Error::Refused`].
    pub async fn subscribe_room_activity(&self, user_id: &str) -> Result<(), Error> {
        let params = json!([format!("{user_id}/rooms-changed"), false]);
        self.subscribe("stream-notify-user", params).await?;
        Ok(())
    }

    /// When any user's or room's avatar changes, as [`DdpEvent::AvatarChanged`]
    /// (`stream-notify-logged` with `updateAvatar`). Needs a login.
    ///
    /// # Errors
    ///
    /// [`Error::ConnectionClosed`] if the connection ended, or [`Error::Refused`].
    pub async fn subscribe_avatar_changes(&self) -> Result<(), Error> {
        let params = json!(["updateAvatar", false]);
        self.subscribe("stream-notify-logged", params).await?;
        Ok(())
    }

    /// The messages deleted in one room, as [`DdpEvent::MessageDeleted`] (`stream-notify-room`
    /// with `<room id>/deleteMessage`). The server doesn't push a deletion on the stream of the
    /// user's messages. Needs a login first. End it with [`unsubscribe`](Self::unsubscribe).
    ///
    /// # Errors
    ///
    /// [`Error::ConnectionClosed`] if the connection ended, or [`Error::Refused`].
    pub async fn subscribe_room_deletions(&self, room_id: &str) -> Result<SubscriptionId, Error> {
        let params = json!([format!("{room_id}/deleteMessage"), false]);
        self.subscribe("stream-notify-room", params).await
    }

    /// What the users do in one room: type, in the room or in one of its threads, as
    /// [`DdpEvent::UserActivity`] (`stream-notify-room` with `<room id>/user-activity`). Needs a
    /// login first. End it with [`unsubscribe`](Self::unsubscribe).
    ///
    /// # Errors
    ///
    /// [`Error::ConnectionClosed`] if the connection ended, or [`Error::Refused`].
    pub async fn subscribe_user_activity(&self, room_id: &str) -> Result<SubscriptionId, Error> {
        let params = json!([format!("{room_id}/user-activity"), false]);
        self.subscribe("stream-notify-room", params).await
    }

    /// Ends a subscription: the server sends no more of its events. It doesn't wait for the
    /// server's answer, and an event already on its way may still arrive. Does nothing if the
    /// connection has ended, since its subscriptions have ended with it.
    pub fn unsubscribe(&self, subscription: &SubscriptionId) {
        let _ = self.send(Command::Unsubscribe {
            id: subscription.0.clone(),
        });
    }

    async fn call(&self, method: &'static str, params: Value) -> Result<Value, Error> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Call {
            method,
            params,
            reply,
        })?;
        answer.await.map_err(|_| Error::ConnectionClosed)?
    }

    async fn subscribe(&self, name: &'static str, params: Value) -> Result<SubscriptionId, Error> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Subscribe {
            name,
            params,
            reply,
        })?;
        answer.await.map_err(|_| Error::ConnectionClosed)?
    }

    fn send(&self, command: Command) -> Result<(), Error> {
        self.commands
            .send(command)
            .map_err(|_| Error::ConnectionClosed)
    }
}

/// `http(s)://host/prefix/` becomes `ws(s)://host/prefix/websocket`.
fn websocket_url(server: &str) -> Result<reqwest::Url, Error> {
    let mut url = base_url(server)?
        .join("websocket")
        .map_err(|e| Error::InvalidUrl(e.to_string()))?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme)
        .map_err(|()| Error::InvalidUrl(server.to_owned()))?;
    Ok(url)
}

/// The same TLS checks as the REST client: the OS verifier, with the crypto provider reqwest uses.
fn tls_config() -> Result<Arc<rustls::ClientConfig>, Error> {
    let tls_error = |e: rustls::Error| websocket_error(tungstenite::Error::Tls(e.into()));
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let verifier = rustls_platform_verifier::Verifier::new(provider.clone()).map_err(tls_error)?;
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(tls_error)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn websocket_error(error: tungstenite::Error) -> Error {
    Error::WebSocket(Box::new(error))
}

fn timed_out() -> Error {
    websocket_error(tungstenite::Error::Io(io::ErrorKind::TimedOut.into()))
}

/// Does the DDP handshake on an open WebSocket, then hands it to a task of its own.
async fn start<S>(mut ws: WebSocketStream<S>) -> Result<(DdpClient, DdpEvents), Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let hello = json!({ "msg": "connect", "version": "1", "support": ["1"] });
    send(&mut ws, &hello).await?;
    timeout(CONNECT_TIMEOUT, handshake(&mut ws))
        .await
        .map_err(|_| timed_out())??;
    let (commands, receiver) = mpsc::unbounded_channel();
    let (events, events_receiver) = mpsc::unbounded_channel();
    let connection = Connection {
        ws,
        events,
        pending: HashMap::new(),
        unsubscribed: HashSet::new(),
        next_id: 0,
    };
    tokio::spawn(connection.run(receiver));
    Ok((DdpClient { commands }, events_receiver))
}

/// Waits for `connected`. Whatever comes before it, such as `{"server_id": "0"}`, is skipped.
async fn handshake<S>(ws: &mut WebSocketStream<S>) -> Result<(), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let frame = ws
            .next()
            .await
            .ok_or(Error::ConnectionClosed)?
            .map_err(websocket_error)?;
        if matches!(frame, Frame::Close(_)) {
            return Err(Error::ConnectionClosed);
        }
        let Frame::Text(text) = frame else { continue };
        match serde_json::from_str::<Incoming>(&text) {
            Ok(Incoming::Connected) => return Ok(()),
            Ok(Incoming::Failed) => return Err(Error::NotRocketChat),
            _ => {}
        }
    }
}

async fn send<S>(ws: &mut WebSocketStream<S>, value: &Value) -> Result<(), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    ws.send(Frame::text(value.to_string()))
        .await
        .map_err(websocket_error)
}

enum Command {
    Call {
        method: &'static str,
        params: Value,
        reply: oneshot::Sender<Result<Value, Error>>,
    },
    Subscribe {
        name: &'static str,
        params: Value,
        reply: oneshot::Sender<Result<SubscriptionId, Error>>,
    },
    Unsubscribe {
        id: String,
    },
}

/// A call or subscription waiting for its answer, by id. Dropping it fails the caller with
/// [`Error::ConnectionClosed`].
enum Pending {
    Call(oneshot::Sender<Result<Value, Error>>),
    Subscribe(oneshot::Sender<Result<SubscriptionId, Error>>),
}

/// The task's state. It lives as long as the WebSocket.
struct Connection<S> {
    ws: WebSocketStream<S>,
    events: mpsc::UnboundedSender<DdpEvent>,
    pending: HashMap<String, Pending>,
    /// The subscriptions ended by the client, whose `nosub` is still to come.
    unsubscribed: HashSet<String>,
    next_id: u64,
}

impl<S> Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut quiet_until = Instant::now() + PING_AFTER;
        let mut pinged = false;
        let result = loop {
            tokio::select! {
                frame = self.ws.next() => match frame {
                    // Tungstenite answers the close, but the stream only ends once the closing
                    // handshake is through, which a peer that has gone may never finish.
                    Some(Ok(Frame::Close(_))) | None => break Ok("the server closed it"),
                    Some(Ok(frame)) => {
                        quiet_until = Instant::now() + PING_AFTER;
                        pinged = false;
                        if let Err(error) = self.receive(frame).await {
                            break Err(error);
                        }
                    }
                    Some(Err(error)) => break Err(websocket_error(error)),
                },
                command = commands.recv() => {
                    let Some(command) = command else {
                        // Nobody can use the connection any more.
                        let _ = self.ws.close(None).await;
                        break Ok("the client was dropped");
                    };
                    if let Err(error) = self.command(command).await {
                        break Err(error);
                    }
                }
                () = sleep_until(quiet_until) => {
                    if pinged {
                        break Err(timed_out());
                    }
                    if let Err(error) = send(&mut self.ws, &json!({ "msg": "ping" })).await {
                        break Err(error);
                    }
                    quiet_until = Instant::now() + PONG_TIMEOUT;
                    pinged = true;
                }
            }
        };
        match result {
            Ok(reason) => tracing::info!(reason, "realtime connection closed"),
            Err(error) => tracing::warn!(%error, "realtime connection lost"),
        }
        // Dropping `self` fails the pending calls and ends the events.
    }

    async fn command(&mut self, command: Command) -> Result<(), Error> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let (frame, pending) = match command {
            Command::Call {
                method,
                params,
                reply,
            } => {
                tracing::debug!(method, id, "DDP call");
                let frame =
                    json!({ "msg": "method", "id": id, "method": method, "params": params });
                (frame, Pending::Call(reply))
            }
            Command::Subscribe {
                name,
                params,
                reply,
            } => {
                tracing::debug!(name, id, "DDP subscription");
                let frame = json!({ "msg": "sub", "id": id, "name": name, "params": params });
                (frame, Pending::Subscribe(reply))
            }
            Command::Unsubscribe { id } => {
                tracing::debug!(id, "DDP unsubscription");
                self.unsubscribed.insert(id.clone());
                return send(&mut self.ws, &json!({ "msg": "unsub", "id": id })).await;
            }
        };
        self.pending.insert(id, pending);
        send(&mut self.ws, &frame).await
    }

    async fn receive(&mut self, frame: Frame) -> Result<(), Error> {
        // WebSocket pings are answered by tungstenite.
        let Frame::Text(text) = frame else {
            return Ok(());
        };
        let incoming = match serde_json::from_str::<Incoming>(&text) {
            Ok(incoming) => incoming,
            Err(error) => {
                tracing::debug!(%error, "unknown DDP frame");
                return Ok(());
            }
        };
        match incoming {
            Incoming::Ping { id } => {
                let pong = match id {
                    Some(id) => json!({ "msg": "pong", "id": id }),
                    None => json!({ "msg": "pong" }),
                };
                send(&mut self.ws, &pong).await?;
            }
            Incoming::Result { id, result, error } => {
                if let Some(Pending::Call(reply)) = self.pending.remove(&id) {
                    let _ = reply.send(match error {
                        Some(error) => Err(error.into()),
                        None => Ok(result.unwrap_or(Value::Null)),
                    });
                } else {
                    tracing::debug!(id, "DDP result for no call");
                }
            }
            Incoming::Ready { subs } => {
                for id in subs {
                    if let Some(Pending::Subscribe(reply)) = self.pending.remove(&id) {
                        let _ = reply.send(Ok(SubscriptionId(id)));
                    }
                }
            }
            Incoming::Nosub { id, error } => {
                if let Some(Pending::Subscribe(reply)) = self.pending.remove(&id) {
                    let error = error.map_or(Error::ConnectionClosed, Error::from);
                    let _ = reply.send(Err(error));
                } else if self.unsubscribed.remove(&id) {
                    tracing::debug!(id, "DDP subscription ended");
                } else {
                    // A subscription that was ready: no more events will come for it.
                    tracing::warn!(id, "the server ended a subscription");
                }
            }
            Incoming::Changed { collection, fields } => {
                match wire::parse_stream_event(&collection, fields) {
                    Ok(Some(event)) => {
                        // Nobody may be reading the events, which is fine.
                        let _ = self.events.send(event);
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(%error, collection, "unexpected stream event"),
                }
            }
            Incoming::Connected | Incoming::Failed | Incoming::Other => {}
        }
        Ok(())
    }
}

/// The frames the server sends that matter here. `added`, `updated` and the like are `Other`.
#[derive(Deserialize)]
#[serde(tag = "msg", rename_all = "lowercase")]
enum Incoming {
    Connected,
    Failed,
    Ping {
        #[serde(default)]
        id: Option<String>,
    },
    Result {
        id: String,
        #[serde(default)]
        result: Option<Value>,
        #[serde(default)]
        error: Option<DdpError>,
    },
    Ready {
        #[serde(default)]
        subs: Vec<String>,
    },
    Nosub {
        id: String,
        #[serde(default)]
        error: Option<DdpError>,
    },
    Changed {
        collection: String,
        #[serde(default)]
        fields: Value,
    },
    #[serde(other)]
    Other,
}

/// A Meteor error. `error` is a number (`401`) or a string (`error-not-allowed`).
#[derive(Deserialize)]
struct DdpError {
    error: Value,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl From<DdpError> for Error {
    fn from(error: DdpError) -> Self {
        let code = match error.error {
            Value::String(code) => code,
            other => other.to_string(),
        };
        if code == "401" || code == "403" {
            return Error::Unauthorized;
        }
        Error::Refused {
            message: error.reason.or(error.message).unwrap_or_default(),
            code,
        }
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use tokio::io::{DuplexStream, duplex};
    use tokio::task::JoinHandle;
    use tokio_tungstenite::tungstenite::protocol::Role;

    use super::*;

    type Started = Result<(DdpClient, DdpEvents), Error>;

    /// The server's side of the connection, scripted by the test: the WebSocket analogue of
    /// wiremock.
    struct Peer {
        ws: WebSocketStream<DuplexStream>,
    }

    impl Peer {
        /// The next DDP frame the client sent. Panics if the connection ends first.
        async fn recv(&mut self) -> Value {
            loop {
                let frame = self.ws.next().await.unwrap().unwrap();
                if let Frame::Text(text) = frame {
                    return serde_json::from_str(&text).unwrap();
                }
            }
        }

        async fn send(&mut self, value: Value) {
            self.send_text(&value.to_string()).await;
        }

        async fn send_text(&mut self, text: &str) {
            self.ws.send(Frame::text(text)).await.unwrap();
        }

        /// Reads the client's `connect` and lets the handshake through, as Rocket.Chat does.
        async fn accept(&mut self) {
            let hello = self.recv().await;
            assert_eq!(hello["msg"], "connect");
            assert_eq!(hello["version"], "1");
            self.send(json!({ "server_id": "0" })).await;
            self.send(json!({ "msg": "connected", "session": "s" }))
                .await;
        }

        /// Reads the client's frame for `msg` and returns its id.
        async fn recv_id(&mut self, msg: &str) -> String {
            let frame = self.recv().await;
            assert_eq!(frame["msg"], msg, "{frame}");
            frame["id"].as_str().unwrap().to_owned()
        }
    }

    /// The client's `start` on one end of an in-memory pipe, and the peer on the other.
    async fn pipe() -> (JoinHandle<Started>, Peer) {
        let (client, server) = duplex(64 * 1024);
        let client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
        let server = WebSocketStream::from_raw_socket(server, Role::Server, None).await;
        (tokio::spawn(start(client)), Peer { ws: server })
    }

    async fn connected() -> (DdpClient, DdpEvents, Peer) {
        let (started, mut peer) = pipe().await;
        peer.accept().await;
        let (client, events) = started.await.unwrap().unwrap_or_else(|e| panic!("{e}"));
        (client, events, peer)
    }

    fn credentials(token: &str) -> Credentials {
        Credentials {
            user_id: "u1".to_owned(),
            token: SecretString::from(token),
        }
    }

    fn meteor_error(code: &Value, reason: &str) -> Value {
        json!({
            "isClientSafe": true, "error": code, "reason": reason,
            "message": format!("{reason} [{code}]"), "errorType": "Meteor.Error",
        })
    }

    fn message_frame(id: &str, text: &str) -> Value {
        json!({
            "msg": "changed", "collection": "stream-room-messages", "id": "id",
            "fields": { "eventName": "__my_messages__", "args": [{
                "_id": id, "rid": "r1", "msg": text, "ts": { "$date": 1_792_511_428_626_i64 },
                "u": { "_id": "u2", "username": "admin", "name": "Administrator" },
            }, { "roomParticipant": true }] },
        })
    }

    // AUTH-003
    #[test]
    fn websocket_url_follows_the_scheme_and_keeps_the_path() {
        let url = |server| websocket_url(server).unwrap().to_string();
        assert_eq!(
            url("http://127.0.0.1:3000"),
            "ws://127.0.0.1:3000/websocket"
        );
        assert_eq!(
            url("https://chat.example.com/"),
            "wss://chat.example.com/websocket"
        );
        assert_eq!(
            url("https://example.com/chat"),
            "wss://example.com/chat/websocket"
        );
        assert!(matches!(
            websocket_url("ftp://example.com"),
            Err(Error::InvalidUrl(_))
        ));
        assert!(matches!(websocket_url("nope"), Err(Error::InvalidUrl(_))));
    }

    // AUTH-003
    #[tokio::test]
    async fn handshake_skips_the_server_id_frame() {
        // `connected()` sends `{"server_id":"0"}` before `connected`, as Rocket.Chat does.
        let (_client, _events, _peer) = connected().await;
    }

    // AUTH-003
    #[tokio::test]
    async fn a_server_that_refuses_the_version_is_not_rocket_chat() {
        let (started, mut peer) = pipe().await;
        peer.recv().await;
        peer.send(json!({ "msg": "failed", "version": "pre1" }))
            .await;
        assert!(matches!(started.await.unwrap(), Err(Error::NotRocketChat)));
    }

    // AUTH-003
    #[tokio::test]
    async fn a_server_that_closes_during_the_handshake_is_a_closed_connection() {
        let (started, mut peer) = pipe().await;
        peer.recv().await;
        peer.ws.close(None).await.unwrap();
        let result = started.await.unwrap();
        assert!(
            matches!(result, Err(Error::ConnectionClosed)),
            "{:?}",
            result.err()
        );
    }

    // AUTH-003
    #[tokio::test(start_paused = true)]
    async fn a_server_that_never_answers_the_handshake_times_out() {
        let (started, _peer) = pipe().await;
        let Err(Error::WebSocket(error)) = started.await.unwrap() else {
            panic!("expected a WebSocket error");
        };
        assert!(
            matches!(*error, tungstenite::Error::Io(ref e) if e.kind() == io::ErrorKind::TimedOut),
            "{error:?}"
        );
    }

    // AUTH-004
    #[tokio::test]
    async fn login_sends_the_resume_token() {
        let (client, _events, mut peer) = connected().await;
        let login = tokio::spawn(async move { client.login(&credentials("tok")).await });

        let frame = peer.recv().await;
        assert_eq!(frame["msg"], "method");
        assert_eq!(frame["method"], "login");
        assert_eq!(frame["params"], json!([{ "resume": "tok" }]));
        let id = frame["id"].as_str().unwrap();
        peer.send(json!({ "msg": "result", "id": id, "result": {
            "id": "u1", "token": "tok", "tokenExpires": { "$date": 1_800_287_426_052_i64 },
            "type": "resume",
        } }))
        .await;
        // A `login` result is followed by the user's record and `updated`; neither matters here.
        peer.send(json!({ "msg": "updated", "methods": [id] }))
            .await;

        login.await.unwrap().unwrap();
    }

    // AUTH-004
    #[tokio::test]
    async fn meteor_errors_map_to_unauthorized_or_refused() {
        let (client, _events, mut peer) = connected().await;
        let cases = [
            (json!(401), "User not found", "Unauthorized"),
            (
                json!(403),
                "You've been logged out by the server",
                "Unauthorized",
            ),
            (json!(404), "Method 'x' not found", "404"),
            (
                json!("error-not-allowed"),
                "Not allowed",
                "error-not-allowed",
            ),
        ];
        for (code, reason, expected) in cases {
            let caller = client.clone();
            let call = tokio::spawn(async move { caller.call("login", json!([])).await });
            let id = peer.recv_id("method").await;
            peer.send(json!({ "msg": "result", "id": id, "error": meteor_error(&code, reason) }))
                .await;
            match (call.await.unwrap(), expected) {
                (Err(Error::Unauthorized), "Unauthorized") => {}
                (Err(Error::Refused { code, message }), expected) => {
                    assert_eq!(code, expected);
                    assert_eq!(message, reason);
                }
                (other, _) => panic!("{other:?}"),
            }
        }
    }

    // AUTH-004
    #[tokio::test]
    async fn results_are_matched_to_calls_by_id() {
        let (client, _events, mut peer) = connected().await;
        let (first, second) = (client.clone(), client.clone());
        let first = tokio::spawn(async move { first.call("a", json!([])).await });
        let id_a = peer.recv_id("method").await;
        let second = tokio::spawn(async move { second.call("b", json!([])).await });
        let id_b = peer.recv_id("method").await;
        assert_ne!(id_a, id_b);

        // Answered out of order.
        peer.send(json!({ "msg": "result", "id": id_b, "result": "b" }))
            .await;
        peer.send(json!({ "msg": "result", "id": id_a, "result": "a" }))
            .await;
        assert_eq!(second.await.unwrap().unwrap(), json!("b"));
        assert_eq!(first.await.unwrap().unwrap(), json!("a"));
    }

    // RT-001
    #[tokio::test]
    async fn a_subscription_is_ready_when_the_server_says_so() {
        let (client, _events, mut peer) = connected().await;
        let subscribing = tokio::spawn(async move { client.subscribe_my_messages().await });

        let frame = peer.recv().await;
        assert_eq!(frame["msg"], "sub");
        assert_eq!(frame["name"], "stream-room-messages");
        assert_eq!(frame["params"], json!(["__my_messages__", false]));
        peer.send(json!({ "msg": "ready", "subs": [frame["id"]] }))
            .await;

        subscribing.await.unwrap().unwrap();
    }

    // RT-002
    #[tokio::test]
    async fn subscription_changes_are_subscribed_for_the_user() {
        let (client, _events, mut peer) = connected().await;
        let subscribing =
            tokio::spawn(async move { client.subscribe_subscription_changes("u1").await });

        let frame = peer.recv().await;
        assert_eq!(frame["name"], "stream-notify-user");
        assert_eq!(frame["params"], json!(["u1/subscriptions-changed", false]));
        peer.send(json!({ "msg": "ready", "subs": [frame["id"]] }))
            .await;

        subscribing.await.unwrap().unwrap();
    }

    // ROOM-005
    #[tokio::test]
    async fn room_activity_is_subscribed_for_the_user() {
        let (client, _events, mut peer) = connected().await;
        let subscribing = tokio::spawn(async move { client.subscribe_room_activity("u1").await });

        let frame = peer.recv().await;
        assert_eq!(frame["name"], "stream-notify-user");
        assert_eq!(frame["params"], json!(["u1/rooms-changed", false]));
        peer.send(json!({ "msg": "ready", "subs": [frame["id"]] }))
            .await;

        subscribing.await.unwrap().unwrap();
    }

    // ROOM-005
    #[tokio::test]
    async fn a_room_change_with_a_last_message_arrives_with_it() {
        let (_client, mut events, mut peer) = connected().await;
        peer.send(
            json!({ "msg": "changed", "collection": "stream-notify-user", "id": "id",
            "fields": { "eventName": "u1/rooms-changed", "args": ["updated",
                { "_id": "r1", "lm": { "$date": 1_792_517_519_831_i64 } }] } }),
        )
        .await;

        let Some(DdpEvent::RoomChanged {
            room_id,
            last_message: Some(last_message),
            ..
        }) = events.recv().await
        else {
            panic!("expected a room change with a last message");
        };
        assert_eq!(room_id, "r1");
        assert_eq!(last_message.as_millisecond(), 1_792_517_519_831);
    }

    // RT-001
    #[tokio::test]
    async fn a_refused_subscription_is_an_error() {
        let (client, _events, mut peer) = connected().await;
        let subscribing = tokio::spawn(async move { client.subscribe_my_messages().await });

        let id = peer.recv_id("sub").await;
        peer.send(json!({ "msg": "nosub", "id": id,
            "error": meteor_error(&json!(404), "Subscription not found") }))
            .await;

        let result = subscribing.await.unwrap();
        assert!(
            matches!(&result, Err(Error::Refused { code, .. }) if code == "404"),
            "{result:?}"
        );
    }

    // RT-003
    #[tokio::test]
    async fn server_pings_are_answered_with_pongs() {
        let (_client, _events, mut peer) = connected().await;

        peer.send(json!({ "msg": "ping", "id": "p1" })).await;
        assert_eq!(peer.recv().await, json!({ "msg": "pong", "id": "p1" }));
        peer.send(json!({ "msg": "ping" })).await;
        assert_eq!(peer.recv().await, json!({ "msg": "pong" }));
    }

    // RT-001
    #[tokio::test]
    async fn stream_frames_become_events_in_order() {
        let (_client, mut events, mut peer) = connected().await;

        peer.send(message_frame("m1", "first")).await;
        peer.send(
            json!({ "msg": "changed", "collection": "stream-notify-user", "id": "id",
            "fields": { "eventName": "u1/subscriptions-changed", "args": ["removed", {
                "_id": "s1", "rid": "r1", "t": "c", "name": "general",
            }] } }),
        )
        .await;
        peer.send(message_frame("m2", "second")).await;

        let Some(DdpEvent::Message(first)) = events.recv().await else {
            panic!("expected a message");
        };
        assert_eq!((first.id.as_str(), first.text.as_str()), ("m1", "first"));
        assert_eq!(first.room_id, "r1");
        assert_eq!(first.author.username, "admin");
        assert_eq!(first.ts.as_millisecond(), 1_792_511_428_626);
        assert_eq!(
            events.recv().await,
            Some(DdpEvent::SubscriptionRemoved {
                room_id: "r1".to_owned()
            })
        );
        let Some(DdpEvent::Message(second)) = events.recv().await else {
            panic!("expected a message");
        };
        assert_eq!(second.id, "m2");
    }

    // RICH-006
    #[tokio::test]
    async fn room_deletions_are_subscribed_for_the_room() {
        let (client, _events, mut peer) = connected().await;
        let subscribing = tokio::spawn(async move { client.subscribe_room_deletions("r1").await });

        let frame = peer.recv().await;
        assert_eq!(frame["msg"], "sub");
        assert_eq!(frame["name"], "stream-notify-room");
        assert_eq!(frame["params"], json!(["r1/deleteMessage", false]));
        peer.send(json!({ "msg": "ready", "subs": [frame["id"]] }))
            .await;

        let subscription = subscribing.await.unwrap().unwrap();
        assert_eq!(subscription.0, frame["id"].as_str().unwrap());
    }

    // RICH-006
    #[tokio::test]
    async fn a_deletion_arrives_as_an_event() {
        let (_client, mut events, mut peer) = connected().await;
        peer.send(
            json!({ "msg": "changed", "collection": "stream-notify-room", "id": "id",
            "fields": { "eventName": "r1/deleteMessage", "args": [{ "_id": "m1" }] } }),
        )
        .await;
        assert_eq!(
            events.recv().await,
            Some(DdpEvent::MessageDeleted {
                room_id: "r1".to_owned(),
                message_id: "m1".to_owned(),
            })
        );
    }

    // ACT-006
    #[tokio::test]
    async fn user_activity_is_subscribed_for_the_room() {
        let (client, _events, mut peer) = connected().await;
        let subscribing = tokio::spawn(async move { client.subscribe_user_activity("r1").await });

        let frame = peer.recv().await;
        assert_eq!(frame["msg"], "sub");
        assert_eq!(frame["name"], "stream-notify-room");
        assert_eq!(frame["params"], json!(["r1/user-activity", false]));
        peer.send(json!({ "msg": "ready", "subs": [frame["id"]] }))
            .await;

        let subscription = subscribing.await.unwrap().unwrap();
        assert_eq!(subscription.0, frame["id"].as_str().unwrap());
    }

    // ACT-006
    #[tokio::test]
    async fn a_user_activity_arrives_as_an_event() {
        let (_client, mut events, mut peer) = connected().await;
        peer.send(
            json!({ "msg": "changed", "collection": "stream-notify-room", "id": "id",
            "fields": { "eventName": "r1/user-activity",
                "args": ["bob", ["user-typing"], { "tmid": "t1" }] } }),
        )
        .await;
        assert_eq!(
            events.recv().await,
            Some(DdpEvent::UserActivity {
                room_id: "r1".to_owned(),
                username: "bob".to_owned(),
                typing: true,
                thread_id: Some("t1".to_owned()),
            })
        );
    }

    // RICH-006
    #[tokio::test]
    async fn unsubscribing_sends_unsub_and_the_connection_goes_on() {
        let (client, mut events, mut peer) = connected().await;
        let subscriber = client.clone();
        let subscribing =
            tokio::spawn(async move { subscriber.subscribe_room_deletions("r1").await });
        let id = peer.recv_id("sub").await;
        peer.send(json!({ "msg": "ready", "subs": [id] })).await;
        let subscription = subscribing.await.unwrap().unwrap();

        client.unsubscribe(&subscription);
        let frame = peer.recv().await;
        assert_eq!(frame, json!({ "msg": "unsub", "id": id }));
        // The server's `nosub` for it is expected, not an event, and nothing else is disturbed.
        peer.send(json!({ "msg": "nosub", "id": id })).await;
        peer.send(message_frame("m1", "after")).await;
        let Some(DdpEvent::Message(message)) = events.recv().await else {
            panic!("expected a message");
        };
        assert_eq!(message.id, "m1");
    }

    // RICH-006
    #[tokio::test]
    async fn unsubscribing_on_a_closed_connection_does_nothing() {
        let (client, mut events, mut peer) = connected().await;
        let subscriber = client.clone();
        let subscribing =
            tokio::spawn(async move { subscriber.subscribe_room_deletions("r1").await });
        let id = peer.recv_id("sub").await;
        peer.send(json!({ "msg": "ready", "subs": [id] })).await;
        let subscription = subscribing.await.unwrap().unwrap();

        peer.ws.close(None).await.unwrap();
        assert_eq!(events.recv().await, None);
        client.unsubscribe(&subscription);
    }

    // RICH-004
    #[tokio::test]
    async fn avatar_changes_are_subscribed_on_the_logged_stream() {
        let (client, _events, mut peer) = connected().await;
        let subscribing = tokio::spawn(async move { client.subscribe_avatar_changes().await });

        let frame = peer.recv().await;
        assert_eq!(frame["name"], "stream-notify-logged");
        assert_eq!(frame["params"], json!(["updateAvatar", false]));
        peer.send(json!({ "msg": "ready", "subs": [frame["id"]] }))
            .await;

        subscribing.await.unwrap().unwrap();
    }

    // RICH-004
    #[tokio::test]
    async fn an_avatar_change_names_a_user_or_a_room() {
        use crate::types::AvatarOwner;
        let (_client, mut events, mut peer) = connected().await;
        for args in [
            json!({ "username": "alice", "etag": "e1" }),
            json!({ "rid": "r1", "etag": "e2" }),
            json!({ "etag": "e3" }),
        ] {
            peer.send(
                json!({ "msg": "changed", "collection": "stream-notify-logged", "id": "id",
                "fields": { "eventName": "updateAvatar", "args": [args] } }),
            )
            .await;
        }
        peer.send(message_frame("m1", "after")).await;

        assert_eq!(
            events.recv().await,
            Some(DdpEvent::AvatarChanged(AvatarOwner::User {
                username: "alice".into()
            }))
        );
        assert_eq!(
            events.recv().await,
            Some(DdpEvent::AvatarChanged(AvatarOwner::Room {
                room_id: "r1".into()
            }))
        );
        // One that names nobody says nothing.
        assert!(matches!(events.recv().await, Some(DdpEvent::Message(_))));
    }

    // RT-001
    #[tokio::test]
    async fn frames_that_are_not_understood_are_ignored() {
        let (_client, mut events, mut peer) = connected().await;

        peer.send_text("not json").await;
        peer.send_text(r#"{"no":"msg"}"#).await;
        peer.ws.send(Frame::binary(vec![1, 2, 3])).await.unwrap();
        peer.send(json!({ "msg": "added", "collection": "users", "id": "u1", "fields": {} }))
            .await;
        peer.send(
            json!({ "msg": "changed", "collection": "stream-other", "id": "id",
            "fields": { "eventName": "x", "args": [1] } }),
        )
        .await;
        // Known streams, but not events this crate follows or can read.
        peer.send(
            json!({ "msg": "changed", "collection": "stream-notify-user", "id": "id",
            "fields": { "eventName": "u1/notification", "args": [{ "title": "x" }] } }),
        )
        .await;
        peer.send(
            json!({ "msg": "changed", "collection": "stream-room-messages", "id": "id",
            "fields": { "eventName": "__my_messages__", "args": [] } }),
        )
        .await;
        peer.send(message_frame("m1", "still here")).await;

        let Some(DdpEvent::Message(message)) = events.recv().await else {
            panic!("expected a message");
        };
        assert_eq!(message.id, "m1");
    }

    // RT-003
    #[tokio::test(start_paused = true)]
    async fn a_quiet_server_is_pinged_and_a_pong_keeps_the_connection() {
        let (client, mut events, mut peer) = connected().await;

        // Twice: the pong starts a new quiet period.
        for _ in 0..2 {
            let started = Instant::now();
            let ping = peer.recv().await;
            assert_eq!(ping["msg"], "ping");
            assert_eq!(started.elapsed(), PING_AFTER);
            peer.send(json!({ "msg": "pong" })).await;
        }

        // Still usable.
        let caller = client.clone();
        let call = tokio::spawn(async move { caller.call("m", json!([])).await });
        let id = peer.recv_id("method").await;
        peer.send(json!({ "msg": "result", "id": id, "result": 1 }))
            .await;
        assert_eq!(call.await.unwrap().unwrap(), json!(1));
        assert!(
            events
                .try_recv()
                .is_err_and(|e| e == mpsc::error::TryRecvError::Empty)
        );
    }

    // RT-003
    #[tokio::test(start_paused = true)]
    async fn a_server_that_stays_silent_after_a_ping_is_dropped() {
        let (client, mut events, mut peer) = connected().await;
        let caller = client.clone();
        let call = tokio::spawn(async move { caller.call("m", json!([])).await });
        peer.recv_id("method").await;

        assert_eq!(peer.recv().await["msg"], "ping");
        let started = Instant::now();
        // No pong. The client gives up, which ends the events and fails the call.
        assert!(events.recv().await.is_none());
        assert_eq!(started.elapsed(), PONG_TIMEOUT);
        assert!(matches!(call.await.unwrap(), Err(Error::ConnectionClosed)));
        // And the connection is gone for later calls too.
        assert!(matches!(
            client.call("m", json!([])).await,
            Err(Error::ConnectionClosed)
        ));
    }

    // RT-003
    #[tokio::test]
    async fn the_server_closing_ends_the_events_and_fails_pending_calls() {
        let (client, mut events, mut peer) = connected().await;
        let caller = client.clone();
        let call = tokio::spawn(async move { caller.call("m", json!([])).await });
        peer.recv_id("method").await;

        peer.ws.close(None).await.unwrap();

        assert!(matches!(call.await.unwrap(), Err(Error::ConnectionClosed)));
        assert!(events.recv().await.is_none());
    }

    // RT-003
    #[tokio::test]
    async fn a_broken_pipe_ends_the_events_and_fails_pending_calls() {
        let (client, mut events, mut peer) = connected().await;
        let caller = client.clone();
        let call = tokio::spawn(async move { caller.call("m", json!([])).await });
        peer.recv_id("method").await;

        // No close handshake: the transport just goes away.
        drop(peer);

        assert!(matches!(call.await.unwrap(), Err(Error::ConnectionClosed)));
        assert!(events.recv().await.is_none());
    }

    // RT-003
    #[tokio::test]
    async fn dropping_the_last_client_closes_the_socket() {
        let (client, mut events, mut peer) = connected().await;
        let clone = client.clone();

        drop(client);
        // A clone keeps it open.
        let pong = tokio::spawn(async move {
            peer.send(json!({ "msg": "ping" })).await;
            assert_eq!(peer.recv().await["msg"], "pong");
            peer
        });
        let mut peer = pong.await.unwrap();

        drop(clone);
        loop {
            match peer.ws.next().await {
                Some(Ok(Frame::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(e)) => panic!("{e}"),
            }
        }
        assert!(events.recv().await.is_none());
    }

    // AUTH-003
    #[tokio::test]
    async fn connecting_to_a_closed_port_fails() {
        // Bind to find a free port, then close it again.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let result = DdpClient::connect(&server).await;
        assert!(
            matches!(result, Err(Error::WebSocket(_))),
            "{:?}",
            result.err()
        );
    }

    // AUTH-003
    #[tokio::test]
    async fn connecting_to_a_web_server_without_websockets_fails() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let page = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi";
            socket.write_all(page.as_bytes()).await.unwrap();
            let _ = socket.shutdown().await;
        });

        let result = DdpClient::connect(&server).await;
        assert!(
            matches!(result, Err(Error::WebSocket(_))),
            "{:?}",
            result.err()
        );
    }
}
