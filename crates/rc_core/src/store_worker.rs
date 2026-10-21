// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The worker that runs the blocking secret-store calls, in the order they were queued.

use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

use crate::domain::StoredSession;
use crate::ports::{SecretStore, StoreError};

pub(crate) enum StoreOp {
    Load(oneshot::Sender<Result<Option<StoredSession>, StoreError>>),
    Save(StoredSession),
    Clear,
}

/// Queues secret-store calls for the store worker. The worker ends when the last clone is dropped
/// and its queue is empty.
#[derive(Clone)]
pub(crate) struct StoreHandle(pub(crate) UnboundedSender<StoreOp>);

impl StoreHandle {
    pub(crate) async fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        let (answer, answered) = oneshot::channel();
        let gone = || StoreError::Unavailable("the store worker is gone".to_owned());
        self.0.send(StoreOp::Load(answer)).map_err(|_| gone())?;
        answered.await.map_err(|_| gone())?
    }

    pub(crate) fn save(&self, session: StoredSession) {
        self.queue(StoreOp::Save(session));
    }

    pub(crate) fn clear(&self) {
        self.queue(StoreOp::Clear);
    }

    fn queue(&self, op: StoreOp) {
        if self.0.send(op).is_err() {
            tracing::warn!("the store worker is gone, the secret store wasn't updated");
        }
    }
}

/// Runs the store calls one at a time, in the order they were queued, off the async threads: they
/// block, and a keyring can wait for the user to unlock it. Ends when the queue is closed and
/// empty.
pub(crate) async fn run_store_worker<S: SecretStore>(
    store: Arc<S>,
    mut ops: UnboundedReceiver<StoreOp>,
) {
    while let Some(op) = ops.recv().await {
        let store = Arc::clone(&store);
        let result = tokio::task::spawn_blocking(move || match op {
            StoreOp::Load(answer) => {
                // The asker may have given up.
                let _ = answer.send(store.load());
                Ok(())
            }
            StoreOp::Save(session) => store.save(&session),
            StoreOp::Clear => store.clear(),
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "secret store update failed"),
            Err(error) => tracing::error!(%error, "secret store call panicked"),
        }
    }
}
