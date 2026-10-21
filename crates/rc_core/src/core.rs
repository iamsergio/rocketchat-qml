// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What every account uses, kept apart from the account actors: the connector, the file cache and
//! the store worker.

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver};

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

/// Starts the store worker and the actor on the runtime, and returns the receiver that is
/// signalled when both have drained and are about to end.
///
/// A task owns the store worker and the [`Shared`]. It loads the stored accounts, picks the one to
/// run (see [`pick_account`]) and runs its actor. When the actor ends it returns the deadline
/// ([`DRAIN_TIMEOUT`](crate::account::DRAIN_TIMEOUT) after it began stopping) by which the token
/// revocations must be done. The task then drops the `Shared`, which closes the worker's queue,
/// and gives the worker until that same deadline to empty it. The worker keeps going while the
/// revocations are awaited, so the whole drain takes one `DRAIN_TIMEOUT` at most.
pub(crate) fn spawn<C: Connector, S: SecretStore>(
    runtime: &Runtime,
    connector: C,
    store: S,
    cache: impl FileCache,
    on_event: impl Fn(Event) + Send + Sync + 'static,
    backoff: Backoff,
    commands: UnboundedReceiver<Command>,
) -> std_mpsc::Receiver<()> {
    let (finished_tx, finished) = std_mpsc::channel();
    let (store_tx, store_rx) = mpsc::unbounded_channel();
    let store_worker = runtime.spawn(run_store_worker(Arc::new(store), store_rx));
    let shared = Shared {
        connector: Arc::new(connector),
        cache: Arc::new(cache),
        store: StoreHandle(store_tx),
    };
    runtime.spawn(async move {
        let (id, stored) = pick_account(&shared).await;
        let deadline = account::run(&shared, id, stored, on_event, backoff, commands).await;
        drop(shared);
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

/// The account to run: the first stored one that has a token, to resume, or else a new one with
/// nothing to resume.
async fn pick_account<C: Connector>(shared: &Shared<C>) -> (AccountId, Option<StoredAccount>) {
    let accounts = shared.store.load().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "can't read the stored accounts");
        Vec::new()
    });
    if accounts.len() > 1 {
        tracing::info!(
            stored = accounts.len(),
            "more than one account is stored, only the first one is used for now"
        );
    }
    match accounts.into_iter().find(|a| a.token.is_some()) {
        Some(account) => (account.id.clone(), Some(account)),
        None => (AccountId::generate(), None),
    }
}
