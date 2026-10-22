// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::ports::Clock;

/// The longest single sleep. The monotonic clock the runtime sleeps by may stand still while the
/// computer is suspended, so a longer wait is made of steps that read the wall clock again.
const MAX_STEP: Duration = Duration::from_secs(60);

/// The system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::now()
    }

    fn sleep_until(&self, at: jiff::Timestamp) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            loop {
                let left = at.duration_since(jiff::Timestamp::now());
                if !left.is_positive() {
                    return;
                }
                tokio::time::sleep(left.unsigned_abs().min(MAX_STEP)).await;
            }
        })
    }
}
