// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Typed client for what GitHub offers about pull requests and their CI: the user's open pull
//! requests with their check jobs, the ones of the repositories they watch, and the actions on a
//! pull request (rerun, approve, update the branch, merge) and on a repository (its runners). It
//! keeps no application state and runs no `gh` process.
//!
//! Everything starts at [`GithubClient`]: an API address (GitHub's, or a stub's) and a token. The
//! serde wire types are private; callers only see the plain types in this crate.

#![forbid(unsafe_code)]

mod client;
mod error;
mod types;
mod wire;

pub use client::{DEFAULT_API_URL, GithubClient};
pub use error::Error;
pub use types::{
    BranchDeletion, CiState, Job, JobState, MergeOutcome, PrRef, PrState, PullRequest,
    PullRequestStatus, Runner, RunnerJob,
};
