// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the GitHub service needs from the outside world: the API, the `gh` command line tool and
//! the place the login is kept. Adapters are in `crate::adapters`, fakes in the tests.

use std::future::Future;

use secrecy::SecretString;

use super::domain::{GithubError, PrKey, PullRequest, Runner};
use crate::ports::StoreError;

/// Opens a [`GithubApi`] for a token. Sync: it only builds, no I/O. The address of the API is the
/// adapter's, so that the GUI tests can point it at a stub.
pub trait GithubConnector: Send + Sync + 'static {
    type Api: GithubApi;

    /// # Errors
    ///
    /// [`GithubError::Other`] if the client can't be set up (a bad API address).
    fn connect(&self, token: &SecretString) -> Result<Self::Api, GithubError>;
}

/// GitHub, as one user. Cheap to clone.
pub trait GithubApi: Clone + Send + Sync + 'static {
    /// The login of the token's user: it tells a good token from a bad one.
    fn viewer(&self) -> impl Future<Output = Result<String, GithubError>> + Send;

    /// The user's open pull requests, drafts included, in any order.
    fn my_pull_requests(
        &self,
    ) -> impl Future<Output = Result<Vec<PullRequest>, GithubError>> + Send;

    /// The `owner/name` of the repositories the user watches.
    fn watched_repos(&self) -> impl Future<Output = Result<Vec<String>, GithubError>> + Send;

    /// Other authors' open pull requests in `repos`, created on or after `since`, in any order.
    fn watched_pull_requests(
        &self,
        repos: &[String],
        since: jiff::civil::Date,
    ) -> impl Future<Output = Result<Vec<PullRequest>, GithubError>> + Send;

    /// The self-hosted runners of a repository. Needs admin rights on it.
    fn runners(&self, repo: &str) -> impl Future<Output = Result<Vec<Runner>, GithubError>> + Send;

    /// Merges with the rebase method; unless `force` the checks must have passed. Returns once
    /// the merge is done, which can take minutes.
    fn merge(
        &self,
        pr: &PrKey,
        force: bool,
    ) -> impl Future<Output = Result<(), GithubError>> + Send;

    /// Rebases the branch on its base.
    fn update_branch(&self, pr: &PrKey) -> impl Future<Output = Result<(), GithubError>> + Send;

    /// Reruns the failed jobs; the cancelled ones only with `include_cancelled`.
    fn rerun_failed(
        &self,
        pr: &PrKey,
        include_cancelled: bool,
    ) -> impl Future<Output = Result<(), GithubError>> + Send;

    fn rerun_job(
        &self,
        repo: &str,
        job_id: u64,
    ) -> impl Future<Output = Result<(), GithubError>> + Send;

    /// Approves the runs that wait for it.
    fn approve_runs(&self, pr: &PrKey) -> impl Future<Output = Result<(), GithubError>> + Send;
}

/// Why `gh auth token` gave no token.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GhError {
    #[error("the GitHub CLI (gh) isn't installed")]
    NotInstalled,
    #[error("the GitHub CLI isn't logged in")]
    NotLoggedIn,
    #[error("the GitHub CLI failed: {0}")]
    Failed(String),
}

/// The GitHub CLI. Blocks: the service calls it off the async threads.
pub trait GhCli: Send + Sync + 'static {
    /// The token of the CLI's login (`gh auth token`), read anew at every call: it is never kept.
    ///
    /// # Errors
    ///
    /// [`GhError`] if there is no `gh`, it has no login, or it fails.
    fn token(&self) -> Result<SecretString, GhError>;
}

/// How the user connected GitHub, as kept between runs.
#[derive(Clone, Debug)]
pub enum GithubAuth {
    /// The GitHub CLI's login. The token itself stays in `gh`'s hands and is read at each start.
    GhCli,
    /// A personal access token the user pasted.
    Token(SecretString),
}

impl PartialEq for GithubAuth {
    fn eq(&self, other: &Self) -> bool {
        use secrecy::ExposeSecret;
        match (self, other) {
            (Self::GhCli, Self::GhCli) => true,
            (Self::Token(a), Self::Token(b)) => a.expose_secret() == b.expose_secret(),
            _ => false,
        }
    }
}

/// Where the user's choice, and a pasted token, survive restarts. The calls block: the service
/// runs them off the async threads.
pub trait GithubStore: Send + Sync + 'static {
    /// What was kept, if anything.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store can't be read or the entry is unreadable.
    fn load(&self) -> Result<Option<GithubAuth>, StoreError>;

    /// Keeps the choice, replacing the one kept.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store can't be written.
    fn save(&self, auth: &GithubAuth) -> Result<(), StoreError>;

    /// Forgets the choice and the token. Succeeds if nothing was kept.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store can't be written.
    fn clear(&self) -> Result<(), StoreError>;
}

impl GithubStore for Box<dyn GithubStore> {
    fn load(&self) -> Result<Option<GithubAuth>, StoreError> {
        (**self).load()
    }

    fn save(&self, auth: &GithubAuth) -> Result<(), StoreError> {
        (**self).save(auth)
    }

    fn clear(&self) -> Result<(), StoreError> {
        (**self).clear()
    }
}

/// The ports of the service, bundled.
pub struct GithubPorts<C, G, S, K> {
    pub connector: C,
    pub gh: G,
    pub store: S,
    pub clock: K,
}
