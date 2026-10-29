// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The knobs of the core that tests turn to make it quick.

use crate::backoff::Backoff;
use crate::directory::DEBOUNCE;
use crate::members::DEBOUNCE as MEMBERS_DEBOUNCE;
use crate::rooms::{CHANNEL_BATCH, QUIET_AFTER};
use crate::timeline::PAGE_SIZE;

/// The environment variable that overrides how long a direct message may be quiet (see
/// [`Tuning::from_env`]).
const QUIET_AFTER_VAR: &str = "RC_QUIET_AFTER_SECS";

/// The environment variable that overrides how often the room list shows the news of channels, in
/// whole seconds (see [`Tuning::from_env`]). `0` shows them at once.
const CHANNEL_BATCH_VAR: &str = "RC_CHANNEL_BATCH_SECS";

/// The environment variable that overrides how many threads the thread list asks for at a time
/// (see [`Tuning::from_env`]).
const THREADS_PAGE_VAR: &str = "RC_THREADS_PAGE_SIZE";

/// The environment variable that overrides how long the directory search waits for the typing to
/// stop, in whole milliseconds (see [`Tuning::from_env`]). `0` searches at once.
const DIRECTORY_DEBOUNCE_VAR: &str = "RC_DIRECTORY_DEBOUNCE_MS";

/// The environment variable that overrides how long the member search waits for the typing to
/// stop, in whole milliseconds (see [`Tuning::from_env`]). `0` searches at once.
const MEMBERS_DEBOUNCE_VAR: &str = "RC_MEMBERS_DEBOUNCE_MS";

/// Timings that the core has a default for and tests change.
#[derive(Clone, Debug)]
pub(crate) struct Tuning {
    /// The waits between reconnects.
    pub(crate) backoff: Backoff,
    /// How long a direct message may go without activity before the room list leaves it out.
    pub(crate) quiet_after: jiff::SignedDuration,
    /// How often the room list shows the news of channels. Zero shows them at once.
    pub(crate) channel_batch: jiff::SignedDuration,
    /// How many threads one request of the thread list asks for.
    pub(crate) threads_page_size: usize,
    /// How long the directory search waits after the last change of the text. Zero searches at
    /// once.
    pub(crate) directory_debounce: jiff::SignedDuration,
    /// How long the member search waits after the last change of the query. Zero searches at once.
    pub(crate) members_debounce: jiff::SignedDuration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            backoff: Backoff::default(),
            quiet_after: QUIET_AFTER,
            channel_batch: CHANNEL_BATCH,
            threads_page_size: PAGE_SIZE,
            directory_debounce: DEBOUNCE,
            members_debounce: MEMBERS_DEBOUNCE,
        }
    }
}

impl Tuning {
    /// The defaults, except `quiet_after`, which `RC_QUIET_AFTER_SECS` sets in whole seconds (a
    /// testing aid: it lets a GUI test see a direct message go quiet in seconds, not days), and
    /// `channel_batch`, which `RC_CHANNEL_BATCH_SECS` sets in whole seconds (a testing aid: the
    /// news of channels shows in seconds, or at once with 0), and `threads_page_size`, which
    /// `RC_THREADS_PAGE_SIZE` sets (a testing aid too: the thread list loads more without hundreds
    /// of threads), and `directory_debounce`, which `RC_DIRECTORY_DEBOUNCE_MS` sets in whole
    /// milliseconds (a testing aid too), and `members_debounce`, which `RC_MEMBERS_DEBOUNCE_MS` sets
    /// the same way. A value that isn't a number of the kind is ignored, with
    /// a warning.
    pub(crate) fn from_env() -> Self {
        let mut tuning = Self::default();
        match std::env::var(QUIET_AFTER_VAR) {
            Ok(value) => {
                if let Ok(secs) = value.trim().parse::<u32>() {
                    tuning.quiet_after = jiff::SignedDuration::from_secs(i64::from(secs));
                } else {
                    tracing::warn!(value, "{QUIET_AFTER_VAR} isn't a number of seconds");
                }
            }
            Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                tracing::warn!("{QUIET_AFTER_VAR} isn't a number of seconds");
            }
        }
        match std::env::var(CHANNEL_BATCH_VAR) {
            Ok(value) => {
                if let Ok(secs) = value.trim().parse::<u32>() {
                    tuning.channel_batch = jiff::SignedDuration::from_secs(i64::from(secs));
                } else {
                    tracing::warn!(value, "{CHANNEL_BATCH_VAR} isn't a number of seconds");
                }
            }
            Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                tracing::warn!("{CHANNEL_BATCH_VAR} isn't a number of seconds");
            }
        }
        match std::env::var(THREADS_PAGE_VAR) {
            Ok(value) => match value.trim().parse::<usize>() {
                Ok(size) if size > 0 => tuning.threads_page_size = size,
                _ => tracing::warn!(value, "{THREADS_PAGE_VAR} isn't a positive number"),
            },
            Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                tracing::warn!("{THREADS_PAGE_VAR} isn't a positive number");
            }
        }
        match std::env::var(DIRECTORY_DEBOUNCE_VAR) {
            Ok(value) => {
                if let Ok(millis) = value.trim().parse::<u32>() {
                    tuning.directory_debounce =
                        jiff::SignedDuration::from_millis(i64::from(millis));
                } else {
                    tracing::warn!(
                        value,
                        "{DIRECTORY_DEBOUNCE_VAR} isn't a number of milliseconds"
                    );
                }
            }
            Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                tracing::warn!("{DIRECTORY_DEBOUNCE_VAR} isn't a number of milliseconds");
            }
        }
        match std::env::var(MEMBERS_DEBOUNCE_VAR) {
            Ok(value) => {
                if let Ok(millis) = value.trim().parse::<u32>() {
                    tuning.members_debounce = jiff::SignedDuration::from_millis(i64::from(millis));
                } else {
                    tracing::warn!(
                        value,
                        "{MEMBERS_DEBOUNCE_VAR} isn't a number of milliseconds"
                    );
                }
            }
            Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                tracing::warn!("{MEMBERS_DEBOUNCE_VAR} isn't a number of milliseconds");
            }
        }
        tuning
    }
}
