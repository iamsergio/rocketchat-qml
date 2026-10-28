// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use jiff::Timestamp;

use crate::types::CiState;

/// What can go wrong talking to GitHub.
///
/// Any `reqwest` failure that isn't an HTTP status (connect, DNS, TLS, timeout, a broken body,
/// client setup) is reported as [`Error::Unreachable`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a valid API URL: {0}")]
    InvalidUrl(String),
    #[error("can't reach GitHub")]
    Unreachable(#[source] reqwest::Error),
    /// 401: the token is wrong, expired or revoked.
    #[error("GitHub refused the token")]
    Unauthorized,
    /// 403 or 429 because the rate limit is used up. `reset` is when the primary limit renews,
    /// `retry_after` how long a secondary limit asks to wait; GitHub sends one of them or neither.
    #[error("GitHub's rate limit is reached")]
    RateLimited {
        reset: Option<Timestamp>,
        retry_after: Option<Duration>,
    },
    /// Any other 403: the token may not do this (e.g. runners need admin rights on the repository).
    #[error("not allowed: {0}")]
    Forbidden(String),
    /// Any other non-2xx answer. `message` comes from GitHub's error body when there is one.
    #[error("GitHub answered {status}: {message}")]
    Api { status: u16, message: String },
    /// GraphQL answers 200 with an `errors` array when a query is refused. The messages are joined
    /// with `; `.
    #[error("GitHub refused the query: {0}")]
    GraphQl(String),
    #[error("unexpected answer from GitHub")]
    Decode(#[source] serde_json::Error),
    /// The URL isn't `https://<host>/<owner>/<repo>/pull/<number>`.
    #[error("not a pull request URL: {0}")]
    NotAPullRequestUrl(String),
    /// The pull request doesn't exist, or the token can't see it.
    #[error("no such pull request: {0}")]
    PullRequestNotFound(String),
    /// Merging needs an open pull request. `state` is `closed` or `merged`.
    #[error("the pull request is already {0}")]
    NotOpen(String),
    /// Merging without `force` needs the checks to have passed.
    #[error("not merging: the checks of the pull request are {0:?}")]
    ChecksNotPassed(CiState),
    /// The merge ran and failed (a conflict, a changed head, a rule). `status` is the one the
    /// merge endpoint reports, `message` its explanation.
    #[error("the merge {status}: {message}")]
    MergeFailed { status: String, message: String },
    /// The merge was still going after the time `merge` waits.
    #[error("the merge is still pending")]
    MergeTimeout,
    #[error("no failed jobs to rerun")]
    NothingToRerun,
    /// Some of several requests failed; the others were done. `first` is the first failure.
    #[error("{failed} of {total} requests failed")]
    Partial {
        failed: usize,
        total: usize,
        #[source]
        first: Box<Error>,
    },
}
