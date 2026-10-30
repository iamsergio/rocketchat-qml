// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The fake answers `around` and `newer` as the server does, so the tests of the jump can rely on
//! it.

use super::*;

fn credentials() -> Credentials {
    Credentials {
        user_id: "uid".to_owned().into(),
        token: SecretString::from("token"),
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn ids(messages: &[Message]) -> Vec<&str> {
    messages.iter().map(|m| m.id.as_str()).collect()
}

/// Room `1` with `m1`..`m9` at seconds 2, 4..18, in no order, and a hidden reply `r` at second 11,
/// between `m5` and `m6`, that the room doesn't show.
fn fake_with_room() -> Fake {
    let fake = Fake::new();
    let mut messages: Vec<Message> = (1..=9).map(|n| message(&format!("m{n}"), n * 2)).collect();
    messages.push(Message {
        thread_id: Some("m1".to_owned().into()),
        ..message("r", 11)
    });
    messages.reverse();
    fake.set_room_messages("1", messages);
    fake
}

fn around(fake: &Fake, id: &str, limit: usize) -> Result<Option<MessageWindow>, ApiError> {
    block_on(fake.around(
        &credentials(),
        &"1".to_owned().into(),
        &id.to_owned().into(),
        limit,
    ))
}

fn newer(fake: &Fake, after: Option<i64>, limit: usize) -> NewerPage {
    let after = after.map(|secs| jiff::Timestamp::from_second(secs).unwrap());
    block_on(fake.newer(&credentials(), &"1".to_owned().into(), after, limit)).unwrap()
}

// MSG-008
#[test]
fn around_gives_half_the_limit_older_and_fewer_newer_oldest_first() {
    let fake = fake_with_room();

    let window = around(&fake, "m4", 4).unwrap().unwrap();
    assert_eq!(ids(&window.messages), ["m2", "m3", "m4", "m5"]);
    assert!(window.more_before && window.more_after);

    let window = around(&fake, "m3", 5).unwrap().unwrap();
    assert_eq!(ids(&window.messages), ["m1", "m2", "m3", "m4", "m5"]);
    assert!(!window.more_before && window.more_after);
    assert_eq!(
        fake.state().around_calls[0],
        AroundCall {
            room: "1".to_owned(),
            message: "m4".to_owned(),
            limit: 4
        }
    );
}

// MSG-008
#[test]
fn around_does_not_fill_the_side_at_an_edge_and_leaves_hidden_replies_out() {
    let fake = fake_with_room();

    let window = around(&fake, "m9", 4).unwrap().unwrap();
    assert_eq!(ids(&window.messages), ["m7", "m8", "m9"]);
    assert!(window.more_before && !window.more_after);

    let window = around(&fake, "m1", 4).unwrap().unwrap();
    assert_eq!(ids(&window.messages), ["m1", "m2"]);
    assert!(!window.more_before && window.more_after);

    // The reply `r` takes a place of the limit and is then left out.
    let window = around(&fake, "m5", 6).unwrap().unwrap();
    assert_eq!(ids(&window.messages), ["m2", "m3", "m4", "m5", "m6"]);
    let window = around(&fake, "m4", 5).unwrap().unwrap();
    assert_eq!(ids(&window.messages), ["m2", "m3", "m4", "m5"]);
}

// MSG-008
#[test]
fn around_a_message_the_room_does_not_hold_is_none_and_a_script_wins() {
    let fake = fake_with_room();
    assert_eq!(around(&fake, "nope", 4).unwrap(), None);

    fake.script_around(None, Err(ApiError::Unauthorized));
    assert!(matches!(
        around(&fake, "m4", 4),
        Err(ApiError::Unauthorized)
    ));
    assert!(around(&fake, "m4", 4).unwrap().is_some());
}

// MSG-008
#[test]
fn newer_excludes_the_time_given_pages_oldest_first_and_ends_on_a_short_page() {
    let fake = fake_with_room();

    let page = newer(&fake, Some(4), 3);
    assert_eq!(ids(&page.messages), ["m3", "m4", "m5"]);
    assert!(!page.reached_end);

    // The hidden reply counts against the limit and is then left out.
    let page = newer(&fake, Some(10), 3);
    assert_eq!(ids(&page.messages), ["m6", "m7"]);
    assert!(!page.reached_end);

    let page = newer(&fake, Some(14), 5);
    assert_eq!(ids(&page.messages), ["m8", "m9"]);
    assert!(page.reached_end);

    let page = newer(&fake, None, 50);
    assert_eq!(page.messages.len(), 9);
    assert!(page.reached_end);
    fake.script_newer(None, Err(ApiError::Unauthorized));
    let failed = block_on(fake.newer(&credentials(), &"1".to_owned().into(), None, 5));
    assert!(matches!(failed, Err(ApiError::Unauthorized)));
    assert_eq!(
        fake.state().newer_calls[0].after,
        Some(jiff::Timestamp::from_second(4).unwrap())
    );
}
