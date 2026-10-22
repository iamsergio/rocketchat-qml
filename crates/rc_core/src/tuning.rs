// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The knobs of the core that tests turn to make it quick.

use crate::backoff::Backoff;
use crate::rooms::QUIET_AFTER;

/// The environment variable that overrides how long a direct message may be quiet (see
/// [`Tuning::from_env`]).
const QUIET_AFTER_VAR: &str = "RC_QUIET_AFTER_SECS";

/// Timings that the core has a default for and tests change.
#[derive(Clone, Debug)]
pub(crate) struct Tuning {
    /// The waits between reconnects.
    pub(crate) backoff: Backoff,
    /// How long a direct message may go without activity before the room list leaves it out.
    pub(crate) quiet_after: jiff::SignedDuration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            backoff: Backoff::default(),
            quiet_after: QUIET_AFTER,
        }
    }
}

impl Tuning {
    /// The defaults, except `quiet_after`, which `RC_QUIET_AFTER_SECS` sets in whole seconds. A
    /// testing aid: it lets a GUI test see a direct message go quiet in seconds, not days. A value
    /// that isn't a number of seconds is ignored, with a warning.
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
        tuning
    }
}
