// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `MessageList` model shows for a message: names, times, day separators and the wording
//! of system messages. Plain Rust, so it can be unit-tested without Qt. The time zone and the
//! current time are parameters, so tests don't depend on where or when they run.

use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use rc_core::domain::{Author, Delivery, Message, MessageKind, SystemEvent};

use crate::rich_html::{Clock, Viewer, render};

/// Who to show as the author: the display name, else the username.
#[must_use]
pub fn author_name(author: &Author) -> &str {
    author
        .display_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or(&author.username)
}

/// The text of an ordinary message; nothing for a system message, which has `system_text`.
#[must_use]
pub fn body(message: &Message) -> &str {
    match &message.kind {
        MessageKind::Text(text) => &text.text,
        MessageKind::System(_) => "",
    }
}

/// The formatted body of an ordinary message, as HTML for rich text; nothing for a system message.
#[must_use]
pub fn html(message: &Message, viewer: &Viewer, clock: Clock) -> String {
    match &message.kind {
        MessageKind::Text(text) => render(text, viewer, clock),
        MessageKind::System(_) => String::new(),
    }
}

/// Whether the message calls the viewer, by name or with `@all` or `@here`. Not one the viewer
/// wrote themselves.
#[must_use]
pub fn mentions_me(message: &Message, viewer: &Viewer) -> bool {
    viewer
        .username
        .as_deref()
        .is_some_and(|me| !message.author.username.eq_ignore_ascii_case(me) && message.mentions(me))
}

/// How QML sees the delivery state of a message: a string, as for the kind of a room.
#[must_use]
pub fn delivery_name(delivery: Delivery) -> &'static str {
    match delivery {
        Delivery::Sent => "sent",
        Delivery::Pending => "pending",
        Delivery::Failed => "failed",
    }
}

/// What a system message says, in English; empty for an ordinary message. Built here so that QML
/// has no wording logic.
#[must_use]
pub fn system_text(message: &Message) -> String {
    let MessageKind::System(event) = &message.kind else {
        return String::new();
    };
    let who = author_name(&message.author);
    match event {
        SystemEvent::Joined => format!("{who} joined"),
        SystemEvent::Left => format!("{who} left"),
        SystemEvent::Added { username } => format!("{who} added {username}"),
        SystemEvent::Removed { username } => format!("{who} removed {username}"),
        SystemEvent::TopicChanged { topic } if topic.is_empty() => {
            format!("{who} cleared the topic")
        }
        SystemEvent::TopicChanged { topic } => format!("{who} changed the topic to: {topic}"),
        // No wording for it yet: show that something happened, and what.
        SystemEvent::Other(code) => format!("{who} ({code})"),
    }
}

/// The time of day in `zone`, as `HH:MM`.
#[must_use]
pub fn time_text(ts: Timestamp, zone: &TimeZone) -> String {
    ts.to_zoned(zone.clone()).strftime("%H:%M").to_string()
}

/// Whether `message` is the first of its day in `zone`, given the message before it (`None` for
/// the first of all).
#[must_use]
pub fn first_of_day(previous: Option<&Message>, message: &Message, zone: &TimeZone) -> bool {
    previous.is_none_or(|previous| {
        previous.ts.to_zoned(zone.clone()).date() != message.ts.to_zoned(zone.clone()).date()
    })
}

/// The heading of a day: `Today`, `Yesterday`, else for example `Monday, 19 October 2026`. Days are
/// those of `zone`, and `now` decides what today is.
#[must_use]
pub fn day_label(ts: Timestamp, zone: &TimeZone, now: Timestamp) -> String {
    let day = ts.to_zoned(zone.clone());
    let today = now.to_zoned(zone.clone());
    if day.date() == today.date() {
        return "Today".to_owned();
    }
    if today
        .date()
        .yesterday()
        .is_ok_and(|yesterday| yesterday == day.date())
    {
        return "Yesterday".to_owned();
    }
    long_date(&day)
}

fn long_date(day: &Zoned) -> String {
    day.strftime("%A, %-d %B %Y").to_string()
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Delivery, MessageKind};
    use rc_core::rich::{Block, Inline, Mention, RichText};

    use super::*;

    fn author(username: &str, display_name: Option<&str>) -> Author {
        Author {
            id: "u".to_owned().into(),
            username: username.to_owned(),
            display_name: display_name.map(str::to_owned),
        }
    }

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    fn message(ts: &str, kind: MessageKind) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: author("alice", Some("Alice A")),
            ts: at(ts),
            kind,
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            edited: false,
            reactions: Vec::new(),
        }
    }

    fn text(ts: &str) -> Message {
        message(ts, MessageKind::Text("hi".into()))
    }

    fn system(event: SystemEvent) -> Message {
        message("2026-10-20T10:00:00Z", MessageKind::System(event))
    }

    fn zone(name: &str) -> TimeZone {
        TimeZone::get(name).unwrap()
    }

    // MSG-003
    #[test]
    fn the_author_is_the_display_name_else_the_username() {
        assert_eq!(author_name(&author("alice", Some("Alice A"))), "Alice A");
        assert_eq!(author_name(&author("alice", None)), "alice");
        assert_eq!(author_name(&author("alice", Some(""))), "alice");
    }

    // MSG-001
    #[test]
    fn the_body_is_the_text_and_empty_for_system_messages() {
        assert_eq!(body(&text("2026-10-20T10:00:00Z")), "hi");
        assert_eq!(body(&system(SystemEvent::Joined)), "");
        assert_eq!(system_text(&text("2026-10-20T10:00:00Z")), "");
    }

    fn viewer(username: &str) -> Viewer {
        Viewer {
            username: Some(username.to_owned()),
            ..Viewer::default()
        }
    }

    fn mentioning(who: &str, mention: Mention) -> Message {
        let mut message = text("2026-10-20T10:00:00Z");
        message.author = author(who, None);
        message.kind = MessageKind::Text(RichText {
            text: String::new(),
            blocks: vec![Block::Paragraph(vec![Inline::Mention(mention)])],
        });
        message
    }

    // RICH-001
    #[test]
    fn the_html_is_the_formatted_body_and_empty_for_system_messages() {
        let zone = zone("UTC");
        let clock = Clock {
            zone: &zone,
            now: at("2026-10-20T10:00:00Z"),
        };
        assert_eq!(
            html(&text("2026-10-20T10:00:00Z"), &Viewer::default(), clock),
            "<p style=\"margin-top:0px; margin-bottom:0px\">hi</p>"
        );
        assert_eq!(
            html(&system(SystemEvent::Joined), &Viewer::default(), clock),
            ""
        );
    }

    // RICH-002
    #[test]
    fn a_message_mentions_the_viewer_by_name_or_to_everybody() {
        let bob = mentioning("alice", Mention::User("bob".into()));
        assert!(mentions_me(&bob, &viewer("bob")));
        assert!(!mentions_me(&bob, &viewer("carol")));
        assert!(!mentions_me(&bob, &Viewer::default()));
        for everybody in [Mention::All, Mention::Here] {
            assert!(mentions_me(&mentioning("alice", everybody), &viewer("bob")));
        }
    }

    // RICH-002
    #[test]
    fn what_the_viewer_wrote_does_not_mention_them() {
        let own = mentioning("bob", Mention::All);
        assert!(!mentions_me(&own, &viewer("Bob")));
        assert!(!mentions_me(&text("2026-10-20T10:00:00Z"), &viewer("bob")));
        assert!(!mentions_me(&system(SystemEvent::Joined), &viewer("bob")));
    }

    // SEND-001, SEND-002
    #[test]
    fn each_delivery_state_has_its_own_name() {
        let states = [Delivery::Sent, Delivery::Pending, Delivery::Failed];
        assert_eq!(states.map(delivery_name), ["sent", "pending", "failed"]);
    }

    // MSG-005
    #[test]
    fn system_messages_say_what_happened() {
        let said = |event| system_text(&system(event));
        assert_eq!(said(SystemEvent::Joined), "Alice A joined");
        assert_eq!(said(SystemEvent::Left), "Alice A left");
        assert_eq!(
            said(SystemEvent::Added {
                username: "bob".into()
            }),
            "Alice A added bob"
        );
        assert_eq!(
            said(SystemEvent::Removed {
                username: "bob".into()
            }),
            "Alice A removed bob"
        );
        assert_eq!(
            said(SystemEvent::TopicChanged {
                topic: "Lunch".into()
            }),
            "Alice A changed the topic to: Lunch"
        );
        assert_eq!(
            said(SystemEvent::TopicChanged {
                topic: String::new()
            }),
            "Alice A cleared the topic"
        );
        assert_eq!(said(SystemEvent::Other("r".into())), "Alice A (r)");
    }

    // MSG-003
    #[test]
    fn the_time_is_local_with_two_digits() {
        let ts = at("2026-10-20T07:05:09Z");
        assert_eq!(time_text(ts, &TimeZone::UTC), "07:05");
        assert_eq!(time_text(ts, &zone("Europe/Lisbon")), "08:05");
        assert_eq!(time_text(ts, &zone("America/New_York")), "03:05");
        assert_eq!(time_text(ts, &zone("Asia/Kolkata")), "12:35");
    }

    // MSG-004
    #[test]
    fn the_first_message_of_a_day_starts_it() {
        let first = text("2026-10-20T10:00:00Z");
        let same = text("2026-10-20T23:59:59Z");
        let next = text("2026-10-21T00:00:00Z");
        assert!(first_of_day(None, &first, &TimeZone::UTC));
        assert!(!first_of_day(Some(&first), &same, &TimeZone::UTC));
        assert!(first_of_day(Some(&same), &next, &TimeZone::UTC));
    }

    // MSG-004
    #[test]
    fn days_are_those_of_the_time_zone() {
        // 23:30 and 00:30 UTC are two days in UTC but one evening in New York (UTC-4) and one
        // night in Lisbon (UTC+1 until the clocks go back on the 25th).
        let before = text("2026-10-20T23:30:00Z");
        let after = text("2026-10-21T00:30:00Z");
        assert!(first_of_day(Some(&before), &after, &TimeZone::UTC));
        assert!(!first_of_day(
            Some(&before),
            &after,
            &zone("America/New_York")
        ));
        assert!(!first_of_day(Some(&before), &after, &zone("Europe/Lisbon")));
        // Lisbon midnight is 23:00 UTC.
        let evening = text("2026-10-20T22:30:00Z");
        assert!(first_of_day(
            Some(&evening),
            &before,
            &zone("Europe/Lisbon")
        ));
    }

    // MSG-004
    #[test]
    fn labels_say_today_yesterday_or_the_date() {
        let now = at("2026-10-20T12:00:00Z");
        let label = |ts: &str| day_label(at(ts), &TimeZone::UTC, now);
        assert_eq!(label("2026-10-20T00:00:00Z"), "Today");
        assert_eq!(label("2026-10-20T23:59:59Z"), "Today");
        assert_eq!(label("2026-10-19T23:59:59Z"), "Yesterday");
        assert_eq!(label("2026-10-19T00:00:00Z"), "Yesterday");
        assert_eq!(label("2026-10-18T12:00:00Z"), "Sunday, 18 October 2026");
        assert_eq!(label("2025-01-05T12:00:00Z"), "Sunday, 5 January 2025");
    }

    // MSG-004
    #[test]
    fn a_message_from_tomorrow_has_a_date_label() {
        let now = at("2026-10-20T12:00:00Z");
        assert_eq!(
            day_label(at("2026-10-21T12:00:00Z"), &TimeZone::UTC, now),
            "Wednesday, 21 October 2026"
        );
    }

    // MSG-004
    #[test]
    fn today_depends_on_the_time_zone() {
        // 2026-10-20 23:30 UTC is already the 21st in Lisbon (UTC+1).
        let now = at("2026-10-20T23:30:00Z");
        let ts = at("2026-10-20T12:00:00Z");
        assert_eq!(day_label(ts, &TimeZone::UTC, now), "Today");
        assert_eq!(day_label(ts, &zone("Europe/Lisbon"), now), "Yesterday");
    }
}
