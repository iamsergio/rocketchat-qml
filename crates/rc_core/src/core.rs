// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The core task: what every account uses (the connector, the file cache and the store worker), and
//! the account actors it runs, one per account.

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::Instrument;

use crate::account;
use crate::backoff::Backoff;
use crate::client::{Command, Event};
use crate::domain::{AccountId, StoredAccount};
use crate::ports::{Connector, FileCache, SecretStore};
use crate::store_worker::{StoreHandle, run_store_worker};

/// The services shared by the accounts. An actor takes clones of them when it is built.
pub(crate) struct Shared<C: Connector> {
    pub(crate) connector: Arc<C>,
    pub(crate) cache: Arc<dyn FileCache>,
    /// Queues calls for the store worker, which ends once every clone is dropped.
    pub(crate) store: StoreHandle,
}

/// What a [`Client`](crate::Client) tells the core task.
pub(crate) enum CoreCommand {
    /// Adds a logged-out account with this id.
    Add(AccountId),
    /// Logs the account out, forgets it and ends it.
    Remove(AccountId),
    /// A command for one account's actor.
    Account(AccountId, Command),
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
    connector: C,
    store: S,
    cache: impl FileCache,
    on_event: impl Fn(AccountId, Event) + Send + Sync + 'static,
    backoff: Backoff,
    commands: UnboundedReceiver<CoreCommand>,
) -> std_mpsc::Receiver<()> {
    let (finished_tx, finished) = std_mpsc::channel();
    let (store_tx, store_rx) = mpsc::unbounded_channel();
    let store_worker = runtime.spawn(run_store_worker(Arc::new(store), store_rx));
    let core = Core {
        shared: Shared {
            connector: Arc::new(connector),
            cache: Arc::new(cache),
            store: StoreHandle(store_tx),
        },
        on_event: Arc::new(on_event),
        backoff,
        accounts: Vec::new(),
        actors: JoinSet::new(),
    };
    runtime.spawn(async move {
        let deadline = core.run(commands).await;
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
    /// The accounts in the order they were added, with the way to reach their actors. Removing
    /// one drops its sender, which is what ends the actor.
    accounts: Vec<(AccountId, UnboundedSender<Command>)>,
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
            self.add(AccountId::generate(), None);
        }
        for account in stored {
            self.add(account.id.clone(), Some(account));
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
            CoreCommand::Add(id) => self.add(id, None),
            CoreCommand::Remove(id) => self.remove(&id),
            CoreCommand::Account(id, command) => {
                if let Some((_, actor)) = self.accounts.iter().find(|(account, _)| *account == id) {
                    // Fails only if the actor is gone.
                    let _ = actor.send(command);
                } else {
                    tracing::debug!(%id, "command for an unknown account dropped");
                }
            }
        }
    }

    /// Emits `Added` and runs an actor for the account, which resumes `stored` if it holds a token.
    fn add(&mut self, id: AccountId, stored: Option<StoredAccount>) {
        (self.on_event)(id.clone(), Event::Added);
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let on_event = Arc::clone(&self.on_event);
        let tagged = id.clone();
        let run = account::run(
            &self.shared,
            id.clone(),
            stored,
            move |event| on_event(tagged.clone(), event),
            self.backoff.clone(),
            commands_rx,
        );
        let span = tracing::info_span!("account", %id);
        self.actors.spawn({
            let id = id.clone();
            async move { (id, run.instrument(span).await) }
        });
        self.accounts.push((id, commands));
    }

    /// Tells the actor to log out and drops it, so that it stops. `Removed` follows when it has.
    fn remove(&mut self, id: &AccountId) {
        let Some(at) = self.accounts.iter().position(|(account, _)| account == id) else {
            tracing::debug!(%id, "removal of an unknown account dropped");
            return;
        };
        let (_, actor) = self.accounts.remove(at);
        // Fails only if the actor is gone.
        let _ = actor.send(Command::Logout);
    }

    /// An actor ended, because its account was removed: emits `Removed`, and adds a new account
    /// if none is left.
    fn ended(&mut self, ended: Result<(AccountId, Instant), tokio::task::JoinError>) {
        match ended {
            Ok((id, _)) => (self.on_event)(id, Event::Removed),
            Err(error) => tracing::error!(%error, "an account actor failed"),
        }
        if self.accounts.is_empty() {
            self.add(AccountId::generate(), None);
        }
    }

    /// Ends every actor, waits for them, and drops the services. Returns the latest of the
    /// deadlines the actors give the store worker. No `Removed` events come.
    async fn stop(mut self) -> Instant {
        self.accounts.clear();
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
