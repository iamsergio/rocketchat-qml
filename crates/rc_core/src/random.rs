// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Unpredictability without a random number crate, for things that need to differ, not to resist
//! an attacker.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// A different number every call: the standard library seeds a hasher randomly per process, and
/// hashing a counter with it scrambles the counter.
pub(crate) fn bits() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    static HASHER: OnceLock<RandomState> = OnceLock::new();
    HASHER
        .get_or_init(RandomState::new)
        .hash_one(COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// A number in `0.0..1.0`.
pub(crate) fn unit() -> f64 {
    // The top 53 bits are what an `f64` holds exactly.
    #[allow(clippy::cast_precision_loss)]
    {
        (bits() >> 11) as f64 / (1_u64 << 53) as f64
    }
}
