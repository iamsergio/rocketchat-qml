// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Downloading avatars and attachment images.

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinSet;

use crate::avatars::AvatarUpdate;
use crate::client::Event;
use crate::domain::{AvatarOwner, Credentials};
use crate::downloads::{Change, Downloads, Fetched, Refresh};
use crate::images::ImageUpdate;
use crate::ports::{ApiError, CacheError, CacheKey, Connector, FileCache, Image, ServerApi};
use crate::session::SessionState;

use super::{Actor, Done};

/// A download's answer.
pub(super) struct DownloadDone<K> {
    /// The login attempt the download belongs to: the answer is stale if another has begun since.
    attempt: u64,
    key: K,
    outcome: Fetched,
}

/// One kind of downloaded file: which keys are wanted, queued and shown, and the downloads in
/// flight.
pub(super) struct Media<K> {
    pub(super) downloads: Downloads<K>,
    tasks: JoinSet<()>,
}

impl<K: Clone + Eq + Hash + Send + 'static> Media<K> {
    pub(super) fn new(refresh: Refresh) -> Self {
        Self {
            downloads: Downloads::new(refresh),
            tasks: JoinSet::new(),
        }
    }

    pub(super) fn abort(&mut self) {
        self.tasks.abort_all();
    }

    /// Starts the queued downloads the limit allows. `fetch` makes the download of a key, and
    /// `wrap` its answer a message for the actor.
    fn start<A: Send + 'static, Fut>(
        &mut self,
        attempt: u64,
        done: &UnboundedSender<Done<A>>,
        wrap: fn(DownloadDone<K>) -> Done<A>,
        fetch: impl Fn(K) -> Fut,
    ) where
        Fut: Future<Output = Fetched> + Send + 'static,
    {
        while let Some(key) = self.downloads.next_download() {
            let fetching = fetch(key.clone());
            let done = done.clone();
            self.tasks.spawn(async move {
                let outcome = fetching.await;
                // Fails only if the actor is gone.
                let _ = done.send(wrap(DownloadDone {
                    attempt,
                    key,
                    outcome,
                }));
            });
        }
    }

    /// Stops the downloads and forgets everything. Returns what the frontend should forget.
    fn clear(&mut self) -> Vec<Change<K>> {
        self.abort();
        self.downloads.reset()
    }
}

/// What a download that got no image comes to: the server couldn't be reached, which is worth
/// another try, or it refused. A refusal isn't the end of the session: the rooms and messages meet
/// the same one if the token is dead, and the route may refuse for other reasons.
fn failed(error: &ApiError, subject: &(dyn Debug + Sync), what: &str) -> Fetched {
    if matches!(error, ApiError::Unreachable(_)) {
        tracing::debug!(%error, ?subject, "can't download the {what}");
        Fetched::Unreachable
    } else {
        tracing::debug!(%error, ?subject, "no {what}");
        Fetched::Nothing
    }
}

/// Stores a downloaded image in the cache.
async fn store(
    cache: Arc<dyn FileCache>,
    key: CacheKey,
    image: Image,
    subject: &(dyn Debug + Sync),
    what: &str,
) -> Fetched {
    let stored = tokio::task::spawn_blocking(move || {
        cache.put(&key, image.content_type.as_deref(), &image.bytes)
    })
    .await;
    match stored {
        Ok(Ok(stored)) => Fetched::Stored {
            path: stored.path,
            changed: stored.changed,
        },
        Ok(Err(error @ CacheError::UnsupportedType(_))) => {
            tracing::debug!(%error, ?subject, "{what} not kept");
            Fetched::Nothing
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, ?subject, "can't keep the {what}");
            Fetched::Nothing
        }
        Err(error) => {
            tracing::error!(%error, "storing an {what} panicked");
            Fetched::Nothing
        }
    }
}

/// Downloads an avatar and stores it. Without one of its own, a cached file goes.
async fn fetch_avatar<A: ServerApi>(
    api: &A,
    credentials: &Credentials,
    owner: &AvatarOwner,
    key: CacheKey,
    cache: Arc<dyn FileCache>,
) -> Fetched {
    match api.avatar(credentials, owner).await {
        Ok(Some(image)) => store(cache, key, image, owner, "avatar").await,
        Ok(None) => forget_avatar(owner, key, cache).await,
        Err(error) => failed(&error, owner, "avatar"),
    }
}

/// Removes the cached file of an avatar the server doesn't have.
async fn forget_avatar(owner: &AvatarOwner, key: CacheKey, cache: Arc<dyn FileCache>) -> Fetched {
    match tokio::task::spawn_blocking(move || cache.remove(&key)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(%error, ?owner, "can't remove the avatar"),
        Err(error) => tracing::error!(%error, "removing an avatar panicked"),
    }
    Fetched::Missing
}

/// Downloads an attachment image and stores it.
async fn fetch_image<A: ServerApi>(
    api: &A,
    credentials: &Credentials,
    path: &str,
    key: CacheKey,
    cache: Arc<dyn FileCache>,
) -> Fetched {
    match api.file(credentials, path).await {
        Ok(image) => store(cache, key, image, &path, "image").await,
        Err(error) => failed(&error, &path, "image"),
    }
}

impl<C: Connector> Actor<C> {
    /// Wants the avatars of `owners`: what the cache holds goes to the frontend at once, and each
    /// avatar is downloaded again, once per session, in the background.
    pub(super) fn want_avatars(&mut self, owners: impl IntoIterator<Item = AvatarOwner>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        let server = account.server.clone();
        let cache = &self.cache;
        let updates: Vec<AvatarUpdate> = owners
            .into_iter()
            .filter_map(|owner| {
                self.avatars
                    .downloads
                    .want(owner, |owner| cache.get(&CacheKey::avatar(&server, owner)))
            })
            .map(Into::into)
            .collect();
        if !updates.is_empty() {
            (self.on_event)(Event::Avatars(updates));
        }
        self.start_avatar_downloads(&server);
    }

    /// Shows the avatars the file cache holds for `owners`, and fetches none: for avatars that
    /// nobody looks at yet. [`want_avatars`](Self::want_avatars) does the rest when they are.
    pub(super) fn show_cached_avatars(&mut self, owners: impl IntoIterator<Item = AvatarOwner>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        let server = account.server.clone();
        let cache = &self.cache;
        let updates: Vec<AvatarUpdate> = owners
            .into_iter()
            .filter_map(|owner| {
                self.avatars
                    .downloads
                    .show_cached(owner, |owner| cache.get(&CacheKey::avatar(&server, owner)))
            })
            .map(Into::into)
            .collect();
        if !updates.is_empty() {
            (self.on_event)(Event::Avatars(updates));
        }
    }

    fn start_avatar_downloads(&mut self, server: &str) {
        let Some(session) = &self.session else {
            return;
        };
        let (api, credentials) = (session.api.clone(), session.credentials.clone());
        let (cache, server) = (Arc::clone(&self.cache), server.to_owned());
        self.avatars
            .start(self.attempt, &self.done_tx, Done::Avatar, move |owner| {
                let (api, credentials) = (api.clone(), credentials.clone());
                let (cache, key) = (Arc::clone(&cache), CacheKey::avatar(&server, &owner));
                async move { fetch_avatar(&api, &credentials, &owner, key, cache).await }
            });
    }

    pub(super) fn handle_avatar_done(&mut self, done: DownloadDone<AvatarOwner>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.attempt != self.attempt {
            tracing::debug!("answer of a stale avatar download dropped");
            return;
        }
        let server = account.server.clone();
        if let Some(change) = self.avatars.downloads.finished(done.key, done.outcome) {
            (self.on_event)(Event::Avatars(vec![change.into()]));
        }
        self.start_avatar_downloads(&server);
    }

    /// Forgets the avatars and tells the frontend, unless it had none. The cache keeps its files.
    pub(super) fn clear_avatars(&mut self) {
        let forgotten: Vec<AvatarUpdate> =
            self.avatars.clear().into_iter().map(Into::into).collect();
        if !forgotten.is_empty() {
            (self.on_event)(Event::Avatars(forgotten));
        }
    }

    /// Wants the images at `paths`: what the cache holds goes to the frontend at once, and the
    /// others are downloaded in the background.
    pub(super) fn want_images(&mut self, paths: impl IntoIterator<Item = String>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        let server = account.server.clone();
        let cache = &self.cache;
        let updates: Vec<ImageUpdate> = paths
            .into_iter()
            .filter_map(|path| {
                self.images
                    .downloads
                    .want(path, |path| cache.get(&CacheKey::attachment(&server, path)))
            })
            .map(Into::into)
            .collect();
        if !updates.is_empty() {
            (self.on_event)(Event::Images(updates));
        }
        self.start_image_downloads(&server);
    }

    fn start_image_downloads(&mut self, server: &str) {
        let Some(session) = &self.session else {
            return;
        };
        let (api, credentials) = (session.api.clone(), session.credentials.clone());
        let (cache, server) = (Arc::clone(&self.cache), server.to_owned());
        self.images
            .start(self.attempt, &self.done_tx, Done::Image, move |path| {
                let (api, credentials) = (api.clone(), credentials.clone());
                let (cache, key) = (Arc::clone(&cache), CacheKey::attachment(&server, &path));
                async move { fetch_image(&api, &credentials, &path, key, cache).await }
            });
    }

    pub(super) fn handle_image_done(&mut self, done: DownloadDone<String>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.attempt != self.attempt {
            tracing::debug!("answer of a stale image download dropped");
            return;
        }
        let server = account.server.clone();
        if let Some(change) = self.images.downloads.finished(done.key, done.outcome) {
            (self.on_event)(Event::Images(vec![change.into()]));
        }
        self.start_image_downloads(&server);
    }

    /// Forgets the attachment images and tells the frontend, unless it had none. The cache keeps
    /// its files.
    pub(super) fn clear_images(&mut self) {
        let forgotten: Vec<ImageUpdate> = self.images.clear().into_iter().map(Into::into).collect();
        if !forgotten.is_empty() {
            (self.on_event)(Event::Images(forgotten));
        }
    }
}
