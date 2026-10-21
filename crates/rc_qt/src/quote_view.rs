// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `MessageList` model shows for the quotes of a message: for each one, who wrote the
//! quoted message, what it said (as HTML and as written) and when. Plain Rust, so it can be
//! unit-tested without Qt.

use rc_core::domain::{Message, Quote};

use crate::message_view::time_text;
use crate::rich_html::{Clock, Viewer, render};

/// One quote as QML gets it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuoteView {
    /// The username of the quoted message's author.
    pub author: String,
    /// What the quoted message said, as HTML for rich text.
    pub body_html: String,
    /// What it said as written, which the tests read.
    pub body: String,
    /// The time of day it was sent, empty if the server didn't say.
    pub time: String,
}

/// The quotes of `message`, in the order the author quoted them.
#[must_use]
pub fn quotes(message: &Message, viewer: &Viewer, clock: Clock) -> Vec<QuoteView> {
    message
        .quotes
        .iter()
        .map(|quote| view(quote, viewer, clock))
        .collect()
}

fn view(quote: &Quote, viewer: &Viewer, clock: Clock) -> QuoteView {
    QuoteView {
        author: quote.author.clone(),
        body_html: render(&quote.text, viewer, clock),
        body: quote.text.text.clone(),
        time: quote
            .ts
            .map_or_else(String::new, |ts| time_text(ts, clock.zone)),
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use jiff::tz::TimeZone;
    use rc_core::domain::{Author, Delivery, MessageKind};
    use rc_core::rich::{Block, Inline, RichText};

    use super::*;

    fn message(quotes: Vec<Quote>) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".into(),
                display_name: None,
            },
            ts: Timestamp::UNIX_EPOCH,
            kind: MessageKind::Text("hi".into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes,
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
        }
    }

    fn quote(author: &str, text: RichText, ts: Option<&str>) -> Quote {
        Quote {
            message: Some("q".to_owned().into()),
            link: "http://s/channel/c?msg=q".into(),
            author: author.into(),
            text,
            ts: ts.map(|ts| ts.parse().unwrap()),
        }
    }

    fn clock(zone: &TimeZone) -> Clock<'_> {
        Clock {
            zone,
            now: "2026-10-21T12:00:00Z".parse().unwrap(),
        }
    }

    // ACT-005
    #[test]
    fn a_quote_has_its_author_its_text_in_both_forms_and_its_time() {
        let bold = RichText {
            text: "say **it**".into(),
            blocks: vec![Block::Paragraph(vec![
                Inline::Text("say ".into()),
                Inline::Bold(vec![Inline::Text("it".into())]),
            ])],
        };
        let message = message(vec![quote("bob", bold, Some("2026-10-21T09:42:00Z"))]);
        let zone = TimeZone::get("Europe/Lisbon").unwrap();

        let views = quotes(&message, &Viewer::default(), clock(&zone));

        let [view] = views.as_slice() else {
            panic!("expected one view, got {views:?}");
        };
        assert_eq!(view.author, "bob");
        assert!(
            view.body_html.contains("say <b>it</b>"),
            "{}",
            view.body_html
        );
        assert_eq!(view.body, "say **it**");
        // Lisbon is an hour ahead of UTC in October.
        assert_eq!(view.time, "10:42");
    }

    // ACT-005
    #[test]
    fn the_quotes_keep_their_order_and_a_time_that_is_not_known_shows_nothing() {
        let message = message(vec![
            quote("bob", "one".into(), None),
            quote("carol", "two".into(), Some("2026-10-21T09:42:00Z")),
        ]);
        let zone = TimeZone::UTC;

        let views = quotes(&message, &Viewer::default(), clock(&zone));

        let seen: Vec<_> = views
            .iter()
            .map(|v| (v.author.as_str(), v.time.as_str()))
            .collect();
        assert_eq!(seen, [("bob", ""), ("carol", "09:42")]);
        assert_eq!(
            quotes(&self::message(Vec::new()), &Viewer::default(), clock(&zone)),
            []
        );
    }

    // ACT-005
    #[test]
    fn a_quote_of_a_message_with_no_text_has_an_empty_body() {
        let message = message(vec![quote("bob", RichText::plain(""), None)]);
        let zone = TimeZone::UTC;

        let views = quotes(&message, &Viewer::default(), clock(&zone));

        assert_eq!(
            (views[0].body.as_str(), views[0].body_html.as_str()),
            ("", "")
        );
    }
}
