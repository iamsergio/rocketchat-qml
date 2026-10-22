// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The core task: what every account uses (the connector, the file cache, the clock and the store
//! worker), and the account actors it runs, one per account.

use std::collections::HashMap;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::Instrument;

use crate::account;
use crate::backoff::Backoff;
use crate::client::{Command, Event};
use crate::domain::{AccountId, StoredAccount};
use crate::ports::{Clock, Connector, FileCache, SecretStore};
use crate::store_worker::{StoreHandle, run_store_worker};
use crate::users::Users;

/// The services shared by the accounts. An actor takes clones of them when it is built.
pub(crate) struct Shared<C: Connector> {
    pub(crate) connector: Arc<C>,
    pub(crate) cache: Arc<dyn FileCache>,
    /// The time of day.
    pub(crate) clock: Arc<dyn Clock>,
    /// Queues calls for the store worker, which ends once every clone is dropped.
    pub(crate) store: StoreHandle,
    /// Who each account is, to refuse a user who is another account's already.
    pub(crate) users: Arc<Users>,
}

/// The command senders of the account actors, shared by the [`Client`](crate::Client), which sends
/// the accounts' commands straight to the actors, and the core task, which adds the accounts it
/// makes itself and clears the map when it stops.
///
/// Sending straight to the actor, with no hop through the core task, keeps a command the frontend
/// sent earlier ahead of anything a background task answers later: the actor handles commands
/// before answers.
///
/// A lock is fine here: it is held only to look up, insert or remove a sender, never across an
/// await or a callback. Sending on an unbounded channel doesn't block.
#[derive(Default)]
pub(crate) struct ActorSenders(Mutex<HashMap<AccountId, UnboundedSender<Command>>>);

impl ActorSenders {
    pub(crate) fn insert(&self, id: AccountId, sender: UnboundedSender<Command>) {
        self.lock().insert(id, sender);
    }

    /// Sends `command` to the account's actor. Returns false if there is no such account or its
    /// actor is gone.
    pub(crate) fn send(&self, id: &AccountId, command: Command) -> bool {
        self.lock()
            .get(id)
            .is_some_and(|sender| sender.send(command).is_ok())
    }

    /// Takes the sender out. Dropping it, once the last command is sent, ends the actor.
    pub(crate) fn take(&self, id: &AccountId) -> Option<UnboundedSender<Command>> {
        self.lock().remove(id)
    }

    /// Drops every sender, which ends every actor.
    pub(crate) fn clear(&self) {
        // Dropped after the lock is released.
        let senders = std::mem::take(&mut *self.lock());
        drop(senders);
    }

    /// A poisoned lock is recovered: each update is one map call that leaves it consistent.
    fn lock(&self) -> MutexGuard<'_, HashMap<AccountId, UnboundedSender<Command>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a [`Client`](crate::Client) tells the core task. Commands for an account don't pass
/// through it: they go straight to the actor (see [`ActorSenders`]).
pub(crate) enum CoreCommand {
    /// Runs an actor for a logged-out account with this id, taking its commands from the receiver.
    /// The client has put the sender in the [`ActorSenders`] already.
    Add(AccountId, UnboundedReceiver<Command>),
    /// The client has told the account's actor to log out and dropped its sender: updates the
    /// order of the accounts. `Removed` comes when the actor has ended.
    Remove(AccountId),
}

/// What the core task shares with the [`Client`](crate::Client): where the accounts' commands go,
/// and the commands for the core itself.
pub(crate) struct Link {
    pub(crate) senders: Arc<ActorSenders>,
    pub(crate) commands: UnboundedReceiver<CoreCommand>,
}

/// The services the core is given, which every account uses.
pub(crate) struct Ports<C, S> {
    pub(crate) connector: C,
    pub(crate) store: S,
    pub(crate) cache: Arc<dyn FileCache>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Calls the frontend's callback, tagged with the account.
type OnEvent = Arc<dyn Fn(AccountId, Event) + Send + Sync>;

/// Starts the store worker and the core task on the runtime, and returns the receiver that is
/// signalled when both have drained and are about to end.
///
/// The core task loads the stored accounts, adds each (see [`Core::add`]) and then serves the
/// commands (see [`Core::run`]). When the commands end it stops the actors and returns the
/// deadline ([`DRAIN_TIMEOUT`](crate::account::DRAIN_TIMEOUT) after they began stopping) by which
/// the token revocations must be done. The spawned task then gives the store worker, whose queue
/// closes once the core task has dropped its [`Shared`], until that same deadline to empty it. The
/// worker keeps going while the revocations are awaited, so the whole drain takes one
/// `DRAIN_TIMEOUT` at most.
pub(crate) fn spawn<C: Connector, S: SecretStore>(
    runtime: &Runtime,
    ports: Ports<C, S>,
    on_event: impl Fn(AccountId, Event) + Send + Sync + 'static,
    backoff: Backoff,
    link: Link,
) -> std_mpsc::Receiver<()> {
    let (finished_tx, finished) = std_mpsc::channel();
    let (store_tx, store_rx) = mpsc::unbounded_channel();
    let store_worker = runtime.spawn(run_store_worker(Arc::new(ports.store), store_rx));
    let core = Core {
        shared: Shared {
            connector: Arc::new(ports.connector),
            cache: ports.cache,
            clock: ports.clock,
            store: StoreHandle(store_tx),
            users: Arc::default(),
        },
        on_event: Arc::new(on_event),
        backoff,
        senders: link.senders,
        accounts: Vec::new(),
        actors: JoinSet::new(),
    };
    runtime.spawn(async move {
        let deadline = core.run(link.commands).await;
        if tokio::time::timeout_at(deadline, store_worker)
            .await
            .is_err()
        {
            tracing::warn!("pending logout work didn't finish in time");
        }
        // Fails only if the client is gone already.
        let _ = finished_tx.send(());
    });
    finished
}

/// Owns the services and the account actors, and does what the frontend asks of the accounts.
///
/// There is always at least one account: the stored ones at startup, else a new logged-out one,
/// and a new one when the last is removed.
///
/// Events of an account arrive in order, as its actor emits them, between two that the core
/// emits itself: `Added` comes first, since it is emitted before the actor is spawned, and
/// `Removed` comes last, since it is emitted once the actor has ended.
struct Core<C: Connector> {
    shared: Shared<C>,
    on_event: OnEvent,
    backoff: Backoff,
    /// The senders of the actors' commands, shared with the client.
    senders: Arc<ActorSenders>,
    /// The accounts in the order they were added, until they are removed.
    accounts: Vec<AccountId>,
    /// The actors, each yielding its account and the deadline by which its revocations are done.
    /// Includes the ones of accounts that were removed and are logging out.
    actors: JoinSet<(AccountId, Instant)>,
}

impl<C: Connector> Core<C> {
    /// Adds the stored accounts, serves the commands until they end, and then stops. Returns the
    /// deadline by which the store worker must be done.
    async fn run(mut self, mut commands: UnboundedReceiver<CoreCommand>) -> Instant {
        let stored = self.shared.store.load().await.unwrap_or_else(|error| {
            tracing::warn!(%error, "can't read the stored accounts");
            Vec::new()
        });
        if stored.is_empty() {
            self.add_new();
        }
        for account in stored {
            self.shared
                .users
                .claim(&account.id, &account.server, &account.user_id);
            let (sender, receiver) = mpsc::unbounded_channel();
            self.senders.insert(account.id.clone(), sender);
            self.add(account.id.clone(), Some(account), receiver);
        }
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break,
                },
                Some(ended) = self.actors.join_next() => self.ended(ended),
            }
        }
        self.stop().await
    }

    fn handle(&mut self, command: CoreCommand) {
        match command {
            CoreCommand::Add(id, commands) => self.add(id, None, commands),
            CoreCommand::Remove(id) => self.accounts.retain(|account| *account != id),
        }
    }

    /// Adds a logged-out account with an id of our own.
    fn add_new(&mut self) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let id = AccountId::generate();
        self.senders.insert(id.clone(), sender);
        self.add(id, None, receiver);
    }

    /// Emits `Added` and runs an actor for the account, which resumes `stored` if it holds a token.
    fn add(
        &mut self,
        id: AccountId,
        stored: Option<StoredAccount>,
        commands: UnboundedReceiver<Command>,
    ) {
        (self.on_event)(id.clone(), Event::Added);
        let on_event = Arc::clone(&self.on_event);
        let tagged = id.clone();
        let run = account::run(
            &self.shared,
            id.clone(),
            stored,
            move |event| on_event(tagged.clone(), event),
            self.backoff.clone(),
            commands,
        );
        let span = tracing::info_span!("account", %id);
        self.actors.spawn({
            let id = id.clone();
            async move { (id, run.instrument(span).await) }
        });
        self.accounts.push(id);
    }

    /// An actor ended, because its account was removed: emits `Removed`, and adds a new account
    /// if none is left. The account leaves the order here too, as the actor may end before the
    /// core reads the client's `Remove`.
    fn ended(&mut self, ended: Result<(AccountId, Instant), tokio::task::JoinError>) {
        match ended {
            Ok((id, _)) => {
                self.accounts.retain(|account| *account != id);
                (self.on_event)(id, Event::Removed);
            }
            Err(error) => tracing::error!(%error, "an account actor failed"),
        }
        if self.accounts.is_empty() {
            self.add_new();
        }
    }

    /// Ends every actor, waits for them, and drops the services. Returns the latest of the
    /// deadlines the actors give the store worker. No `Removed` events come.
    async fn stop(mut self) -> Instant {
        self.accounts.clear();
        self.senders.clear();
        let mut deadline = Instant::now();
        while let Some(ended) = self.actors.join_next().await {
            match ended {
                Ok((_, ends)) => deadline = deadline.max(ends),
                Err(error) => tracing::error!(%error, "an account actor failed"),
            }
        }
        deadline
    }
}
