// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Channels show their news in batches, on the fake clock's schedule.

use std::sync::mpsc::RecvTimeoutError;

use super::*;

/// How long the harness holds the news of channels, in seconds.
const BATCH: i64 = 25 * 60;

fn channel_subscription(id: &str, name: &str, secs: i64) -> rc_api::Subscription {
    let mut sub = subscription(id, name, rc_api::RoomType::Channel);
    sub.last_message = Some(at(secs));
    sub
}

fn channel(id: &str, name: &str, secs: i64) -> Room {
    Room {
        last_activity: at(secs),
        ..room(id, name, RoomKind::Channel)
    }
}

/// A harness that holds news for [`BATCH`], logged in with the channels `alpha` (10 s) and `beta`
/// (30 s), listed as `beta`, `alpha`, and the private group `secret` (20 s) between them.
fn logged_in_batching(fake: &Fake) -> Harness {
    fake.script_login(None, Ok(()));
    let mut secret = channel_subscription("3", "secret", 20);
    secret.room_type = rc_api::RoomType::Private;
    fake.script_fetch(
        None,
        Ok(vec![
            channel_subscription("1", "alpha", 10),
            channel_subscription("2", "beta", 30),
            secret,
        ]),
    );
    let harness = Harness::with_batch(fake, jiff::SignedDuration::from_secs(BATCH));
    harness.login("http://h", "alice");
    harness.events_until(|event| matches!(event, Event::Rooms(_)));
    harness
}

/// Checks that no room change or unread total comes for a little while.
fn assert_quiet(harness: &Harness) {
    loop {
        match harness.recv_timeout(Duration::from_millis(300)) {
            Err(RecvTimeoutError::Timeout) => return,
            // Whether news is held is not what these tests are about.
            Ok(Event::HoldingNews(_)) => {}
            other => panic!("{other:?}"),
        }
    }
}

/// Holds news back in the channel `alpha`: one more unread, at the same time.
fn hold_news(fake: &Fake, unread: u32) {
    let mut sub = channel_subscription("1", "alpha", 10);
    sub.unread = unread;
    sub.last_message = None;
    assert!(fake.push(0, Pushed::Subscription(sub)));
}

/// The next event of the account, the others aside, if it is about held news.
fn next_holding(harness: &Harness) -> bool {
    loop {
        if let Event::HoldingNews(holding) = harness.recv_timeout(Duration::from_secs(5)).unwrap() {
            return holding;
        }
    }
}

// ROOM-011
#[test]
fn pushes_into_a_channel_are_held_until_the_batch_is_due() {
    let fake = Fake::new();
    let harness = logged_in_batching(&fake);
    let mut unread = channel_subscription("1", "alpha", 10);
    unread.unread = 2;
    unread.last_message = None;
    assert!(fake.push(0, Pushed::Subscription(unread)));
    assert!(fake.push(
        0,
        Pushed::RoomChanged {
            room: "1".to_owned().into(),
            last_message: Some(at(50)),
            topic: None,
        }
    ));
    // Neither the list nor the badge hears of it.
    assert_quiet(&harness);

    // The deadline is the interval after the first held push.
    harness
        .clock
        .advance(Duration::from_secs(BATCH.unsigned_abs() - 1));
    assert_quiet(&harness);
    harness.clock.advance(Duration::from_secs(1));
    let mut shown = channel("1", "alpha", 50);
    shown.unread = 2;
    assert_eq!(
        harness.next_event(),
        rooms_change(RoomChange::Moved {
            from: 2,
            to: 0,
            item: shown,
        })
    );
    assert_eq!(
        harness.next_unread(),
        Unread {
            rooms: 1,
            mentions: 0,
        }
    );

    // The batch is over: the next held push starts another, and nothing else wakes the actor.
    assert_quiet(&harness);
    let mut more = channel_subscription("2", "beta", 30);
    more.unread = 1;
    more.last_message = None;
    assert!(fake.push(0, Pushed::Subscription(more)));
    assert_quiet(&harness);
    harness
        .clock
        .advance(Duration::from_secs(BATCH.unsigned_abs()));
    let mut shown = channel("2", "beta", 30);
    shown.unread = 1;
    assert_eq!(
        harness.next_event(),
        rooms_change(RoomChange::Updated {
            index: 1,
            item: shown,
        })
    );
    assert_eq!(
        harness.next_unread(),
        Unread {
            rooms: 2,
            mentions: 0,
        }
    );
}

// ROOM-011
#[test]
fn a_mention_shows_a_channel_at_once_and_the_unread_total_follows() {
    let fake = Fake::new();
    let harness = logged_in_batching(&fake);
    let mut quiet = channel_subscription("1", "alpha", 10);
    quiet.unread = 1;
    quiet.last_message = None;
    assert!(fake.push(0, Pushed::Subscription(quiet)));
    assert_quiet(&harness);

    let mut mentioned = channel_subscription("2", "beta", 30);
    mentioned.unread = 1;
    mentioned.user_mentions = 1;
    mentioned.last_message = None;
    assert!(fake.push(0, Pushed::Subscription(mentioned)));
    let mut shown = channel("2", "beta", 30);
    shown.unread = 1;
    shown.user_mentions = 1;
    assert_eq!(
        harness.next_event(),
        rooms_change(RoomChange::Updated {
            index: 0,
            item: shown,
        })
    );
    assert_eq!(
        harness.next_unread(),
        Unread {
            rooms: 1,
            mentions: 1,
        }
    );
}

// ROOM-011
#[test]
fn marking_a_channel_unread_from_the_menu_shows_at_once() {
    let fake = Fake::new();
    let harness = logged_in_batching(&fake);

    harness.account().mark_unread("1".to_owned().into());

    let mut unread = channel("1", "alpha", 10);
    unread.unread = 1;
    unread.alert = true;
    let events = harness.events_until(|event| matches!(event, Event::RoomAction { .. }));
    assert_eq!(
        events[0],
        rooms_change(RoomChange::Updated {
            index: 2,
            item: unread
        })
    );
}

// ROOM-012
#[test]
fn releasing_by_command_shows_the_news_before_the_deadline() {
    let fake = Fake::new();
    let harness = logged_in_batching(&fake);
    hold_news(&fake, 2);
    assert_quiet(&harness);

    harness.account().release_channels();

    let mut shown = channel("1", "alpha", 10);
    shown.unread = 2;
    assert_eq!(
        harness.next_event(),
        rooms_change(RoomChange::Updated {
            index: 2,
            item: shown,
        })
    );
    assert_eq!(
        harness.next_unread(),
        Unread {
            rooms: 1,
            mentions: 0,
        }
    );
    // The batch is over: the old deadline brings nothing, and nothing is held.
    harness
        .clock
        .advance(Duration::from_secs(BATCH.unsigned_abs()));
    assert_quiet(&harness);
    // Nothing held: the command is a no-op.
    harness.account().release_channels();
    assert_quiet(&harness);
}

// ROOM-012
#[test]
fn the_holding_news_event_comes_only_when_it_flips() {
    let fake = Fake::new();
    let harness = logged_in_batching(&fake);
    // Nothing came from the start, while nothing was held.
    hold_news(&fake, 1);
    assert!(next_holding(&harness));
    // More held news changes nothing.
    hold_news(&fake, 2);
    assert_quiet(&harness);

    harness.account().release_channels();
    assert!(!next_holding(&harness));

    // Reading the held room (on another device, here by the menu) ends the holding too.
    hold_news(&fake, 3);
    assert!(next_holding(&harness));
    harness.account().mark_read("1".to_owned().into());
    assert!(!next_holding(&harness));

    // So does a logout, which forgets the news.
    hold_news(&fake, 4);
    assert!(next_holding(&harness));
    harness.logout();
    assert!(!next_holding(&harness));
}
