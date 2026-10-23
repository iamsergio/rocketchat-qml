// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Downloading avatars and attachment images.

use std::sync::Arc;

use crate::avatars::Fetched;
use crate::client::Event;
use crate::domain::{AvatarOwner, Credentials};
use crate::images::{self};
use crate::ports::{ApiError, CacheError, CacheKey, Connector, FileCache, ServerApi};
use crate::session::SessionState;

use super::{Actor, AvatarDone, Done, ImageDone};

/// Downloads an avatar and stores it. Without one of its own, a cached file goes.
async fn fetch_avatar<A: ServerApi>(
    api: &A,
    credentials: &Credentials,
    owner: &AvatarOwner,
    key: CacheKey,
    cache: Arc<dyn FileCache>,
) -> Fetched {
    let image = match api.avatar(credentials, owner).await {
        Ok(Some(image)) => image,
        Ok(None) => return forget_avatar(owner, key, cache).await,
        Err(error @ ApiError::Unreachable(_)) => {
            tracing::debug!(%error, ?owner, "can't download the avatar");
            return Fetched::Unreachable;
        }
        // Not the end of the session: the rooms and messages meet the same refusal if the token
        // is dead, and the avatar route may refuse for other reasons.
        Err(error) => {
            tracing::debug!(%error, ?owner, "no avatar");
            return Fetched::Nothing;
        }
    };
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
            tracing::debug!(%error, ?owner, "avatar not kept");
            Fetched::Nothing
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, ?owner, "can't keep the avatar");
            Fetched::Nothing
        }
        Err(error) => {
            tracing::error!(%error, "storing an avatar panicked");
            Fetched::Nothing
        }
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
) -> images::Fetched {
    let image = match api.file(credentials, path).await {
        Ok(image) => image,
        Err(error @ ApiError::Unreachable(_)) => {
            tracing::debug!(%error, path, "can't download the image");
            return images::Fetched::Unreachable;
        }
        // Not the end of the session: the rooms and messages meet the same refusal if the token
        // is dead.
        Err(error) => {
            tracing::debug!(%error, path, "no image");
            return images::Fetched::Nothing;
        }
    };
    let stored = tokio::task::spawn_blocking(move || {
        cache.put(&key, image.content_type.as_deref(), &image.bytes)
    })
    .await;
    match stored {
        Ok(Ok(stored)) => images::Fetched::Stored(stored.path),
        Ok(Err(error @ CacheError::UnsupportedType(_))) => {
            tracing::debug!(%error, path, "image not kept");
            images::Fetched::Nothing
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, path, "can't keep the image");
            images::Fetched::Nothing
        }
        Err(error) => {
            tracing::error!(%error, "storing an image panicked");
            images::Fetched::Nothing
        }
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
        let updates: Vec<_> = owners
            .into_iter()
            .filter_map(|owner| {
                self.avatars
                    .want(owner, |owner| cache.get(&CacheKey::avatar(&server, owner)))
            })
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
        let updates: Vec<_> = owners
            .into_iter()
            .filter_map(|owner| {
                self.avatars
                    .show_cached(owner, |owner| cache.get(&CacheKey::avatar(&server, owner)))
            })
            .collect();
        if !updates.is_empty() {
            (self.on_event)(Event::Avatars(updates));
        }
    }

    fn start_avatar_downloads(&mut self, server: &str) {
        let Some(session) = &self.session else {
            return;
        };
        while let Some(owner) = self.avatars.next_download() {
            let api = session.api.clone();
            let credentials = session.credentials.clone();
            let key = CacheKey::avatar(server, &owner);
            let cache = Arc::clone(&self.cache);
            let attempt = self.attempt;
            let done = self.done_tx.clone();
            self.avatar_tasks.spawn(async move {
                let outcome = fetch_avatar(&api, &credentials, &owner, key, cache).await;
                // Fails only if the actor is gone.
                let _ = done.send(Done::Avatar(AvatarDone {
                    attempt,
                    owner,
                    outcome,
                }));
            });
        }
    }

    pub(super) fn handle_avatar_done(&mut self, done: AvatarDone) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.attempt != self.attempt {
            tracing::debug!("answer of a stale avatar download dropped");
            return;
        }
        let server = account.server.clone();
        if let Some(update) = self.avatars.finished(done.owner, done.outcome) {
            (self.on_event)(Event::Avatars(vec![update]));
        }
        self.start_avatar_downloads(&server);
    }

    /// Forgets the avatars and tells the frontend, unless it had none. The cache keeps its files.
    pub(super) fn clear_avatars(&mut self) {
        self.avatar_tasks.abort_all();
        let forgotten = self.avatars.reset();
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
        let updates: Vec<_> = paths
            .into_iter()
            .filter_map(|path| {
                self.images
                    .want(path, |path| cache.get(&CacheKey::attachment(&server, path)))
            })
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
        while let Some(path) = self.images.next_download() {
            let api = session.api.clone();
            let credentials = session.credentials.clone();
            let key = CacheKey::attachment(server, &path);
            let cache = Arc::clone(&self.cache);
            let attempt = self.attempt;
            let done = self.done_tx.clone();
            self.image_tasks.spawn(async move {
                let outcome = fetch_image(&api, &credentials, &path, key, cache).await;
                // Fails only if the actor is gone.
                let _ = done.send(Done::Image(ImageDone {
                    attempt,
                    path,
                    outcome,
                }));
            });
        }
    }

    pub(super) fn handle_image_done(&mut self, done: ImageDone) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.attempt != self.attempt {
            tracing::debug!("answer of a stale image download dropped");
            return;
        }
        let server = account.server.clone();
        if let Some(update) = self.images.finished(done.path, done.outcome) {
            (self.on_event)(Event::Images(vec![update]));
        }
        self.start_image_downloads(&server);
    }

    /// Forgets the attachment images and tells the frontend, unless it had none. The cache keeps
    /// its files.
    pub(super) fn clear_images(&mut self) {
        self.image_tasks.abort_all();
        let forgotten = self.images.reset();
        if !forgotten.is_empty() {
            (self.on_event)(Event::Images(forgotten));
        }
    }
}
