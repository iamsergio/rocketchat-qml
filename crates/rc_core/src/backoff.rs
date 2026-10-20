// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How long to wait before trying the server again after the realtime connection was lost.

use std::time::Duration;

use crate::random;

/// The wait after the first failure.
const BASE: Duration = Duration::from_secs(1);
/// The longest wait: a server that is down for an hour is asked once a minute.
const CAP: Duration = Duration::from_secs(60);
/// How long a connection must stay up to count as working: a server that accepts and drops at once
/// must not get the first step's wait every time.
const STABLE: Duration = Duration::from_secs(60);

/// The wait doubles with every failure in a row, up to a cap, and is then cut randomly to between
/// half and all of it. Without the cut, all the clients of a server that restarted would come
/// back in the same second.
#[derive(Clone, Debug)]
pub struct Backoff {
    base: Duration,
    cap: Duration,
    stable: Duration,
    /// Failures since the last success.
    failures: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(BASE, CAP, STABLE)
    }
}

impl Backoff {
    /// Waits `base` after the first failure and never more than `cap`. A connection that stayed up
    /// for `stable` wipes the failures out (see [`lasted`](Self::lasted)).
    #[must_use]
    pub const fn new(base: Duration, cap: Duration, stable: Duration) -> Self {
        Self {
            base,
            cap,
            stable,
            failures: 0,
        }
    }

    /// The wait before the next try, which counts as a failure.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.delay(random::unit());
        self.failures = self.failures.saturating_add(1);
        delay
    }

    /// Whether a connection that was up for `uptime` was up long enough to forget the failures.
    #[must_use]
    pub fn lasted(&self, uptime: Duration) -> bool {
        uptime >= self.stable
    }

    /// Forgets the failures: the connection worked.
    pub fn reset(&mut self) {
        self.failures = 0;
    }

    /// The wait for the current failure count. `jitter` is in `0.0..=1.0`: 0 gives half of the
    /// step, 1 all of it.
    fn delay(&self, jitter: f64) -> Duration {
        // Past 2^20 the cap has been reached for any sane base.
        let factor = 1_u32 << self.failures.min(20);
        let step = self
            .base
            .checked_mul(factor)
            .map_or(self.cap, |step| step.min(self.cap));
        let jitter = if jitter.is_finite() {
            jitter.clamp(0.0, 1.0)
        } else {
            1.0
        };
        step.mul_f64(0.5 + 0.5 * jitter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn secs(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    // RT-003
    #[test]
    fn the_step_doubles_up_to_the_cap() {
        let mut backoff = Backoff::new(secs(1), secs(60), secs(60));
        let mut steps = Vec::new();
        for _ in 0..9 {
            steps.push(backoff.delay(1.0));
            backoff.failures += 1;
        }
        assert_eq!(
            steps,
            [1, 2, 4, 8, 16, 32, 60, 60, 60].map(Duration::from_secs)
        );
    }

    // RT-003
    #[test]
    fn jitter_cuts_the_step_to_between_half_and_all_of_it() {
        let mut backoff = Backoff::new(secs(4), secs(60), secs(60));
        assert_eq!(backoff.delay(0.0), secs(2));
        assert_eq!(backoff.delay(0.5), secs(3));
        assert_eq!(backoff.delay(1.0), secs(4));
        // Out of range counts as the nearest end.
        assert_eq!(backoff.delay(-3.0), secs(2));
        assert_eq!(backoff.delay(7.0), secs(4));
        assert_eq!(backoff.delay(f64::NAN), secs(4));
        backoff.failures = 2;
        assert_eq!(backoff.delay(0.0), secs(8));
    }

    // RT-003
    #[test]
    fn a_reset_starts_over() {
        let mut backoff = Backoff::new(secs(1), secs(60), secs(60));
        for _ in 0..4 {
            backoff.next_delay();
        }
        assert_eq!(backoff.delay(1.0), secs(16));
        backoff.reset();
        assert_eq!(backoff.delay(1.0), secs(1));
    }

    // RT-003
    #[test]
    fn only_a_long_uptime_counts_as_stable() {
        let backoff = Backoff::new(secs(1), secs(60), secs(30));
        assert!(!backoff.lasted(secs(29)));
        assert!(backoff.lasted(secs(30)));
    }

    // RT-003
    #[test]
    fn many_failures_neither_overflow_nor_pass_the_cap() {
        let mut backoff = Backoff::new(secs(1), secs(60), secs(60));
        backoff.failures = u32::MAX;
        assert_eq!(backoff.delay(1.0), secs(60));
        // A base whose doubling doesn't fit in a `Duration`.
        let mut huge = Backoff::new(Duration::MAX, secs(60), secs(60));
        huge.failures = 3;
        assert_eq!(huge.delay(1.0), secs(60));
        assert!(backoff.next_delay() <= secs(60));
    }

    // RT-003
    #[test]
    fn the_random_delays_stay_in_range_and_differ() {
        let delays: Vec<_> = (0..50)
            .map(|_| Backoff::new(secs(10), secs(10), secs(60)).next_delay())
            .collect();
        assert!(delays.iter().all(|d| *d >= secs(5) && *d <= secs(10)));
        assert!(delays.iter().any(|d| *d != delays[0]));
    }
}
