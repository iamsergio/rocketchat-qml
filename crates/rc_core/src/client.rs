// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The handle frontends drive the core through.

use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use secrecy::SecretString;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::account;
use crate::adapters::RestConnector;
use crate::avatars::AvatarUpdate;
use crate::backoff::Backoff;
use crate::domain::{MessageId, RoomId};
use crate::emoji::{self, CustomEmojis};
use crate::error::Error;
use crate::images::ImageUpdate;
use crate::policy::{ActionError, MessageAction, MessagePolicy};
use crate::ports::{Connector, FileCache, SecretStore};
use crate::rooms::{RoomAction, RoomActionError, RoomChange};
use crate::session::SessionState;
use crate::timeline::{ThreadStatus, TimelineChange, TimelineStatus};

#[cfg(test)]
mod tests;

/// Worker threads of the async runtime: networking only, the GUI has its own thread.
const WORKER_THREADS: usize = 2;
/// How long dropping a [`Client`] waits for the actor to finish draining.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);
/// How long dropping a [`Client`] waits for the runtime's threads after that.
const RUNTIME_WAIT: Duration = Duration::from_secs(1);

/// What the core tells the frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    Session(SessionState),
    /// Changes to the room list, to apply in order. After `LoggedIn` it carries the rooms; after
    /// `LoggedOut` it empties the list, unless the list was empty already. Reconnecting brings
    /// what changed meanwhile as inserts, updates, moves and removals, never as a reset.
    Rooms(Vec<RoomChange>),
    /// Changes to the message list of the open room, to apply in order. Opening a room starts with
    /// a reset, to what is held of it already. Logging out resets to nothing, if a room was open.
    Timeline(Vec<TimelineChange>),
    /// What the frontend shows about the message list. Comes after the changes it goes with.
    TimelineStatus(TimelineStatus),
    /// Changes to the message list of the open thread, to apply in order: the parent first, then
    /// its replies, oldest first. Opening a thread starts with a reset to nothing, as does
    /// closing it, and so does anything that closes it: another room, a logout, the room or the
    /// parent going away.
    ThreadTimeline(Vec<TimelineChange>),
    /// What the frontend shows about the thread's message list. Comes after the changes it goes
    /// with.
    ThreadStatus(ThreadStatus),
    /// The state of the realtime connection of a logged-in session, when it changes. A session
    /// starts `Connected`, and so does the frontend until told otherwise: nothing is sent at login,
    /// and a logout or an expiry resets it without an event.
    Connection(Connection),
    /// The custom emoji of the server, whenever the list changes. Logging out empties it, if it
    /// held any.
    CustomEmoji(CustomEmojis),
    /// The files to show as avatars, for the users and rooms that changed: the first one a user or
    /// room gets, or a new one when the server's differs. After a logout, the ones that go.
    Avatars(Vec<AvatarUpdate>),
    /// The files to show as attachment images, for the images that got one. After a logout, the
    /// ones that go.
    Images(Vec<ImageUpdate>),
    /// What the server lets users do with their own messages, whenever it changes. Logging out
    /// resets it to the default (everything allowed), if it differed.
    MessagePolicy(MessagePolicy),
    /// How an edit, a deletion or a reaction the frontend asked for ended. A refusal that needs no
    /// request (not the user's message, past the time limit...) is answered at once. Nothing comes
    /// if the session ends first.
    MessageAction {
        id: MessageId,
        action: MessageAction,
        outcome: Result<(), ActionError>,
    },
    /// How a command on a room (a favorite, mark as read or unread, leaving) ended. A refusal that
    /// needs no request is answered at once. Nothing comes if the session ends first.
    RoomAction {
        room: RoomId,
        action: RoomAction,
        outcome: Result<(), RoomActionError>,
    },
}

/// Whether the session is live. The held rooms and messages stay on show while it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Connected,
    /// The connection was lost, or couldn't be opened; the next try comes after `retry_in`.
    Waiting {
        retry_in: Duration,
    },
    /// A try is under way.
    Reconnecting,
}

pub(crate) enum Command {
    Login {
        server: String,
        username: String,
        password: SecretString,
    },
    Logout,
    OpenRoom(RoomId),
    LoadOlder,
    OpenThread(MessageId),
    CloseThread,
    LoadOlderThread,
    SendMessage {
        room: RoomId,
        text: String,
        quote: Option<MessageId>,
        thread: Option<MessageId>,
        also_to_channel: bool,
    },
    RetryMessage(MessageId),
    EditMessage {
        id: MessageId,
        text: String,
    },
    DeleteMessage(MessageId),
    React {
        id: MessageId,
        /// The shortcode with its colons.
        emoji: String,
        react: bool,
    },
    RoomAction {
        room: RoomId,
        action: RoomAction,
    },
}

/// The core, as seen from a frontend: commands go in, events come out through the callback given
/// at creation. Commands never block.
///
/// Dropping the client stops the core. It first lets a pending logout finish (the token revocation
/// and clearing the stored session), which blocks the dropping thread for two seconds at most, and
/// a little longer in the worst case. Don't drop it from async code; the GUI thread owns it.
pub struct Client {
    // Ending the commands is what stops the actor.
    commands: Option<UnboundedSender<Command>>,
    /// Signalled by the actor when it has drained and is about to end.
    finished: std_mpsc::Receiver<()>,
    runtime: Option<Runtime>,
}

impl Client {
    /// A client for real servers. See [`with_ports`](Self::with_ports) for `on_event`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn start(
        store: impl SecretStore,
        cache: impl FileCache,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_ports(RestConnector, store, cache, on_event)
    }

    /// A client that reaches servers through `connector`, remembers the session in `store` and
    /// keeps downloaded files in `cache`; tests pass fakes. If the store holds a session, the
    /// client tries to resume it.
    ///
    /// `on_event` is called once per state change, in order, from a runtime thread. The first event
    /// always comes: `Resuming` if a stored session is being resumed, `LoggedOut` otherwise.
    /// Until then the state is unknown. It must return quickly and must not touch GUI objects: a Qt
    /// frontend hops to its GUI thread inside it, e.g. with `CxxQtThread::queue`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn with_ports<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
        cache: impl FileCache,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_backoff(connector, store, cache, on_event, Backoff::default())
    }

    /// [`with_ports`](Self::with_ports) with the waits between reconnects set, so that tests don't
    /// wait for seconds.
    fn with_backoff<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
        cache: impl FileCache,
        on_event: impl Fn(Event) + Send + Sync + 'static,
        backoff: Backoff,
    ) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .enable_all()
            .build()
            .map_err(Error::Runtime)?;
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let finished = account::spawn(
            &runtime,
            connector,
            store,
            cache,
            on_event,
            backoff,
            commands_rx,
        );
        Ok(Self {
            commands: Some(commands),
            finished,
            runtime: Some(runtime),
        })
    }

    /// Starts logging in. `server` may lack the scheme, https is assumed. The outcome arrives as
    /// events. Ignored unless logged out.
    pub fn login(&self, server: &str, username: &str, password: SecretString) {
        self.send(Command::Login {
            server: server.to_owned(),
            username: username.to_owned(),
            password,
        });
    }

    /// Cancels a login in progress, or logs out and forgets the stored session. Ignored when logged
    /// out.
    pub fn logout(&self) {
        self.send(Command::Logout);
    }

    /// Shows the room's messages: what is held of them at once, then the newest ones as they
    /// arrive. Replaces the room that was open. The room is marked read, on the server and in the
    /// room list, and stays read while it is open. Ignored unless logged in, if the room isn't in the
    /// room list, or if it is the open room already (unless its last load failed: then it tries
    /// again).
    pub fn open_room(&self, room: RoomId) {
        self.send(Command::OpenRoom(room));
    }

    /// Fetches the messages before the oldest one of the open room. Ignored when no room is open,
    /// when a fetch is in flight, or when the start of the room is reached.
    pub fn load_older(&self) {
        self.send(Command::LoadOlder);
    }

    /// Shows the replies of a message of the open room, in a thread beside the room: a reset to
    /// nothing at once, then the parent and its newest replies as they arrive. Replaces the
    /// thread that was open. The parent must be a sent text message that the open room holds and
    /// that isn't a reply itself; anything else is logged at debug and ignored. So is a thread
    /// that is open already, unless its last load failed: then it tries again. Ignored unless
    /// logged in. The thread closes with the room (see [`close_thread`](Self::close_thread)).
    pub fn open_thread(&self, parent: MessageId) {
        self.send(Command::OpenThread(parent));
    }

    /// Closes the open thread, and tells the frontend with a reset to nothing. It closes by itself
    /// when another room is opened, the room is closed or leaves the list, the parent is deleted
    /// and on logout. Ignored when no thread is open.
    pub fn close_thread(&self) {
        self.send(Command::CloseThread);
    }

    /// Fetches the replies before the oldest one the open thread holds. Ignored when no thread is
    /// open, when a fetch is in flight, or when the start of the thread is reached.
    pub fn load_older_thread(&self) {
        self.send(Command::LoadOlderThread);
    }

    /// Sends a text message to the room. It shows in the room's messages at once, as pending (see
    /// [`Delivery`]), and becomes sent when the server confirms it, or failed if it can't. It
    /// goes out whichever room is open by then. Ignored unless logged in, if the room isn't in the
    /// room list, or if the text is blank.
    ///
    /// With `quote`, the message quotes that one, which shows at once too. It must be a sent text
    /// message that the open room holds, else the message goes out without a quote. The server
    /// makes the quote from a link in the text, but only when the link begins with its own
    /// `Site_Url`, so that is what the links are built from (it is read at login; the address of the
    /// login stands in for a server that has none).
    ///
    /// With `thread`, the message is a reply in that thread, which must be the open one (else the
    /// message is not sent). It shows in the thread's messages at once, as pending, and also in
    /// the room's if `also_to_channel`, which asks the server to show it there too. Without a
    /// thread, `also_to_channel` does nothing.
    pub fn send_message(
        &self,
        room: RoomId,
        text: &str,
        quote: Option<MessageId>,
        thread: Option<MessageId>,
        also_to_channel: bool,
    ) {
        self.send(Command::SendMessage {
            room,
            text: text.to_owned(),
            quote,
            thread,
            also_to_channel,
        });
    }

    /// Sends a message of the open room or thread that failed again, under the same id, so that it
    /// can't appear twice if the first attempt got through after all. Ignored unless it is a
    /// failed message of the open room or thread.
    pub fn retry_message(&self, id: MessageId) {
        self.send(Command::RetryMessage(id));
    }

    /// Replaces the text of a message of the user's in the open room. The answer comes as
    /// [`Event::MessageAction`]. The text is trimmed; a blank one is refused, and one that is the
    /// message's text already succeeds without asking the server. The message keeps its quotes.
    /// Refused at once too if the message isn't in the open room, isn't the user's, isn't sent,
    /// isn't text, or the server's settings don't allow editing it (any more). Ignored unless
    /// logged in.
    pub fn edit_message(&self, id: MessageId, text: &str) {
        self.send(Command::EditMessage {
            id,
            text: text.to_owned(),
        });
    }

    /// Deletes a message of the user's in the open room. The answer comes as
    /// [`Event::MessageAction`], and the message leaves the room's messages with it. Refused at
    /// once under the same conditions as [`edit_message`](Self::edit_message). Ignored unless
    /// logged in.
    pub fn delete_message(&self, id: MessageId) {
        self.send(Command::DeleteMessage(id));
    }

    /// Adds the user's reaction with `emoji` to a message of the open room, or takes it back. The
    /// emoji is a shortcode, with or without colons. The answer comes as [`Event::MessageAction`],
    /// and the reaction shows in the message with it, without waiting for the server's push.
    /// Asking for what is so already succeeds. Refused at once, without a request, if the message
    /// isn't in the open room, isn't sent or isn't text, or the emoji is blank. Ignored unless
    /// logged in.
    pub fn react(&self, id: MessageId, emoji: &str, react: bool) {
        self.send(Command::React {
            id,
            emoji: emoji::shortcode(emoji),
            react,
        });
    }

    /// Marks the room as one of the user's favorites, or takes it back. The answer comes as
    /// [`Event::RoomAction`], and the room moves in the list with it (favorites come first),
    /// without waiting for the server's push. Asking for what is so already succeeds. Refused at
    /// once, without a request, if the room isn't in the list. Ignored unless logged in.
    pub fn set_favorite(&self, room: RoomId, favorite: bool) {
        let action = if favorite {
            RoomAction::Favorite
        } else {
            RoomAction::Unfavorite
        };
        self.send(Command::RoomAction { room, action });
    }

    /// Marks the room read, on the server and in the room list, as opening it does. The answer
    /// comes as [`Event::RoomAction`]. Refused at once, without a request, if the room isn't in
    /// the list. Ignored unless logged in.
    pub fn mark_read(&self, room: RoomId) {
        self.send(Command::RoomAction {
            room,
            action: RoomAction::MarkRead,
        });
    }

    /// Marks the room unread: the server counts one unread message and raises the alert, and the
    /// room list shows that at once. The answer comes as [`Event::RoomAction`]. Refused at once,
    /// without a request, if the room isn't in the list or is the open room, which stays read
    /// while it is open. Ignored unless logged in.
    pub fn mark_unread(&self, room: RoomId) {
        self.send(Command::RoomAction {
            room,
            action: RoomAction::MarkUnread,
        });
    }

    /// Leaves a channel or a private group. The answer comes as [`Event::RoomAction`], and the
    /// room leaves the list with it, and is closed if it was open. The server refuses the last
    /// owner of a room, and that is the answer. Refused at once, without a request, if the room
    /// isn't in the list or is a direct message. Ignored unless logged in.
    pub fn leave_room(&self, room: RoomId) {
        self.send(Command::RoomAction {
            room,
            action: RoomAction::Leave,
        });
    }

    fn send(&self, command: Command) {
        let sent = self
            .commands
            .as_ref()
            .is_some_and(|c| c.send(command).is_ok());
        if !sent {
            tracing::error!("the session actor is gone, command dropped");
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.commands.take();
        if self.finished.recv_timeout(SHUTDOWN_WAIT).is_err() {
            tracing::warn!("the core didn't finish in time, pending work is lost");
        }
        // A store call stuck on a locked keyring must not hang the exit.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(RUNTIME_WAIT);
        }
    }
}
