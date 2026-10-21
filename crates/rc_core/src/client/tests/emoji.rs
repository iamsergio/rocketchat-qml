// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The custom emoji a session fetches.

use super::*;

fn party() -> rc_api::CustomEmoji {
    rc_api::CustomEmoji {
        name: "party".into(),
        aliases: vec!["celebrate".into()],
        extension: "gif".into(),
    }
}

fn party_emoji(server: &str) -> CustomEmojis {
    CustomEmojis::new(server, &[party()])
}

// RICH-003
#[test]
fn login_fetches_the_custom_emoji_after_the_rooms() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().emoji = Some(Ok(vec![party()]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
        Event::CustomEmoji(party_emoji("http://h")),
    ]);
}

// RICH-003
#[test]
fn a_server_without_custom_emoji_sends_no_event_for_them() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    assert!(
        harness
            .events
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
}

// RICH-003
#[test]
fn logout_forgets_the_custom_emoji() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().emoji = Some(Ok(vec![party()]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
        Event::CustomEmoji(party_emoji("http://h")),
    ]);
    harness.client.logout();
    harness.expect_events(vec![
        session_event(logged_out(None)),
        Event::CustomEmoji(CustomEmojis::default()),
    ]);
}

// RICH-003
#[test]
fn a_failed_custom_emoji_fetch_leaves_the_session_alone() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().emoji = Some(Err(ApiError::Unreachable("down".into())));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
}
