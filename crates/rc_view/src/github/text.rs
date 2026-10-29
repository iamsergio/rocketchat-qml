// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The words and numbers of the GitHub screens that aren't a view of their own: ages, durations
//! and the reasons a call failed.

use jiff::Timestamp;
use rc_core::github::{ConnectFailure, GithubError};

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

/// How long ago `then` was, in one unit: "40 s", "11 min", "1 h", "5 d", "3 w", "8 mo", "2 y". The
/// unit changes at its first full second, minute, hour, day, week (7 days), month (30 days) and
/// year (365 days), and the number is rounded down. A time in the future (the clocks differ by a
/// little) is "0 s".
#[must_use]
pub fn age(now: Timestamp, then: Timestamp) -> String {
    let seconds = (now.as_second() - then.as_second()).max(0);
    let (n, unit) = match seconds {
        s if s < MINUTE => (s, "s"),
        s if s < HOUR => (s / MINUTE, "min"),
        s if s < DAY => (s / HOUR, "h"),
        s if s < WEEK => (s / DAY, "d"),
        s if s < MONTH => (s / WEEK, "w"),
        s if s < YEAR => (s / MONTH, "mo"),
        s => (s / YEAR, "y"),
    };
    format!("{n} {unit}")
}

/// How long a finished job took: "48 s", "9 m 12 s", "9 m", "1 h 5 m", "2 h". A zero part after
/// the first is left out.
#[must_use]
pub fn duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < MINUTE {
        format!("{seconds} s")
    } else if seconds < HOUR {
        let (m, s) = (seconds / MINUTE, seconds % MINUTE);
        if s == 0 {
            format!("{m} m")
        } else {
            format!("{m} m {s} s")
        }
    } else {
        hours_and_minutes(seconds)
    }
}

/// How long a running job has been going: "30 s so far", "6 m so far", "1 h 5 m so far". Whole
/// minutes from the first one, because a running figure that counts seconds is always stale.
#[must_use]
pub fn so_far(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < MINUTE {
        format!("{seconds} s so far")
    } else if seconds < HOUR {
        format!("{} m so far", seconds / MINUTE)
    } else {
        format!("{} so far", hours_and_minutes(seconds))
    }
}

fn hours_and_minutes(seconds: i64) -> String {
    let (h, m) = (seconds / HOUR, seconds % HOUR / MINUTE);
    if m == 0 {
        format!("{h} h")
    } else {
        format!("{h} h {m} m")
    }
}

/// Why a command or a refresh failed, as a sentence the user reads.
#[must_use]
pub fn error_text(error: &GithubError) -> String {
    match error {
        GithubError::Unauthorized => "GitHub refused the token".to_owned(),
        GithubError::RateLimited { .. } => "GitHub's rate limit is reached".to_owned(),
        GithubError::Unreachable => "Can't reach GitHub".to_owned(),
        GithubError::Forbidden(message) => format!("Not allowed: {message}"),
        GithubError::NotOpen(state) => format!("The pull request is already {state}"),
        GithubError::ChecksNotPassed(_) => "The checks haven't passed".to_owned(),
        GithubError::MergeFailed(message) => format!("The merge failed: {message}"),
        GithubError::MergeTimeout => "The merge is still pending".to_owned(),
        GithubError::NothingToRerun => "There are no failed jobs to rerun".to_owned(),
        GithubError::Partial { failed, total } => format!("{failed} of {total} requests failed"),
        GithubError::NotFound => "The pull request doesn't exist any more".to_owned(),
        GithubError::Other(message) => message.clone(),
    }
}

/// What the connect page says about a failed connection.
#[must_use]
pub fn connect_failure_text(failure: &ConnectFailure) -> String {
    match failure {
        ConnectFailure::BadToken => "GitHub refused the token".to_owned(),
        ConnectFailure::GhMissing => "gh isn't installed".to_owned(),
        ConnectFailure::GhNotLoggedIn => "gh isn't logged in. Run gh auth login".to_owned(),
        ConnectFailure::RateLimited => "GitHub's rate limit is reached".to_owned(),
        ConnectFailure::Unreachable => "Can't reach GitHub".to_owned(),
        ConnectFailure::Other(message) => message.clone(),
    }
}

/// "1 check failed", "2 checks failed".
#[must_use]
pub fn checks_failed(count: usize) -> String {
    if count == 1 {
        "1 check failed".to_owned()
    } else {
        format!("{count} checks failed")
    }
}
