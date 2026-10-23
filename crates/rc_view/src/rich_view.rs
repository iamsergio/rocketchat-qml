// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What rich text shows, whatever markup a frontend writes it in: which links are made, the text of
//! a timestamp, who a mention is for, the box of a task and the value of a colour. `rich_html`
//! (Qt's rich text) and `rc_slint`'s markdown both take them from here, so the two agree.

use jiff::Timestamp;
use rc_core::rich::Mention;

use crate::viewer::{Clock, Viewer};

/// Whether a link to `url` is worth making: other schemes (`file:`, `javascript:`, ...) are not
/// something a message should be able to open.
#[must_use]
pub fn is_safe_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

/// The box before a task list item, with the space after it.
#[must_use]
pub fn task_marker(done: bool) -> &'static str {
    if done { "\u{2611} " } else { "\u{2610} " }
}

/// The name a mention shows after the `@`, and whether it is one of the viewer's: `@all` and
/// `@here` always are.
#[must_use]
pub fn mention_name<'a>(mention: &'a Mention, viewer: &Viewer) -> (&'a str, bool) {
    match mention {
        Mention::All => ("all", true),
        Mention::Here => ("here", true),
        Mention::User(name) => (
            name.as_str(),
            viewer
                .username
                .as_deref()
                .is_some_and(|me| me.eq_ignore_ascii_case(name)),
        ),
    }
}

/// A colour as the author's `#rrggbb`, or `#rrggbbaa` if it is not opaque.
#[must_use]
pub fn color_hex(r: u8, g: u8, b: u8, a: u8) -> String {
    if a == u8::MAX {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// A timestamp the way the author's format letter asks (`t` time, `T` with seconds, `d` date, `D`
/// long date, `f` and `F` date and time, `R` relative), in the user's time zone. The author's own
/// text if it isn't a time or the format is unknown.
#[must_use]
pub fn time_text(time: Option<Timestamp>, format: &str, fallback: &str, clock: Clock) -> String {
    let Some(time) = time else {
        return fallback.to_owned();
    };
    let zoned = time.to_zoned(clock.zone.clone());
    let pattern = match format {
        "t" => "%H:%M",
        "T" => "%H:%M:%S",
        "d" => "%Y-%m-%d",
        "D" => "%-d %B %Y",
        "f" => "%-d %B %Y %H:%M",
        "F" => "%A, %-d %B %Y %H:%M",
        "R" => return relative(time, clock.now),
        _ => return fallback.to_owned(),
    };
    zoned.strftime(pattern).to_string()
}

/// `3 hours ago`, `in 2 days`: the largest unit that fits at least once.
fn relative(time: Timestamp, now: Timestamp) -> String {
    let seconds = now.as_second() - time.as_second();
    let amount = seconds.unsigned_abs();
    let units = [(86_400, "day"), (3_600, "hour"), (60, "minute")];
    let (count, unit) = units
        .iter()
        .find(|(size, _)| amount >= *size)
        .map_or((amount, "second"), |(size, name)| (amount / size, *name));
    let plural = if count == 1 { "" } else { "s" };
    if seconds >= 0 {
        format!("{count} {unit}{plural} ago")
    } else {
        format!("in {count} {unit}{plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RICH-001
    #[test]
    fn a_relative_time_uses_the_largest_unit_and_looks_both_ways() {
        let now: Timestamp = "2026-10-20T12:00:00Z".parse().unwrap();
        let at = |text: &str| relative(text.parse().unwrap(), now);
        assert_eq!(at("2026-10-20T11:59:30Z"), "30 seconds ago");
        assert_eq!(at("2026-10-20T11:59:00Z"), "1 minute ago");
        assert_eq!(at("2026-10-20T09:00:00Z"), "3 hours ago");
        assert_eq!(at("2026-10-18T12:00:00Z"), "2 days ago");
        assert_eq!(at("2026-10-20T14:00:00Z"), "in 2 hours");
        assert_eq!(at("2026-10-20T12:00:00Z"), "0 seconds ago");
    }
}
