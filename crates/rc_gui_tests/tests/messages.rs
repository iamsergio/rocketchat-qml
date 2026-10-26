// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Reading a room's messages, against the local test server (`RC_TEST_SERVER_URL`). Each test
//! makes its own user and rooms and only looks at those. The app runs with `TZ=UTC`, so the times
//! it shows are those the server stored.

mod common;

use std::time::Duration;

use rc_gui_tests::GuiError;
use rc_gui_tests::paths::{main_window, message, message_row, room, room_row};

use rc_test_server::{Admin, TestUser};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, open_room, post_as, press_home, utc_time};

/// More than one page of history, so that opening a room loads only the newest part.
const TOTAL: usize = 60;
/// How many messages the first page holds: `rc_core`'s `PAGE_SIZE`. The tests don't link to the
/// core; `count < TOTAL` after opening a room is what shows that paging happens at all.
const NEWEST_PAGE: usize = 50;

/// What the app shows as the author of admin's messages.
fn admin_name(admin: &Admin) -> &str {
    admin.name.as_deref().unwrap_or(&admin.username)
}

/// A new user and a channel they are in, with messages `texts` posted to it as admin. Returns the
/// channel id and the messages.
fn channel_with_messages(
    admin: &Admin,
    user: &TestUser,
    texts: &[String],
) -> (String, Vec<rc_test_server::Posted>) {
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = texts
        .iter()
        .map(|text| admin.post_message(&channel_id, text))
        .collect();
    (channel_id, posted)
}

// MSG-001, MSG-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn opening_a_room_shows_its_messages_with_author_and_time() {
    let admin = Admin::login();
    let user = admin.create_user();
    let texts: Vec<String> = ["first", "second", "third"]
        .map(|text| format!("{text} {}", rc_test_server::unique()))
        .to_vec();
    let (channel_id, posted) = channel_with_messages(&admin, &user, &texts);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    for (message_posted, text) in posted.iter().zip(&texts) {
        let id = &message_posted.id;
        gui.wait_until_visible(&message(id, message_row::BODY), TIMEOUT)
            .unwrap();
        assert_eq!(gui.text(&message(id, message_row::BODY)).unwrap(), *text);
        assert_eq!(
            gui.text(&message(id, message_row::AUTHOR)).unwrap(),
            admin_name(&admin)
        );
        assert_eq!(
            gui.text(&message(id, message_row::TIME)).unwrap(),
            utc_time(&message_posted.ts)
        );
    }
    // The room header names the room, and its row is the open one.
    assert!(gui.exists_and_visible(main_window::ROOM_TITLE).unwrap());
    assert!(
        !gui.exists_and_visible(main_window::ROOM_PLACEHOLDER)
            .unwrap()
    );
}

// MSG-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn opening_another_room_replaces_the_messages() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (first_id, first) = channel_with_messages(&admin, &user, &["in the first".to_owned()]);
    let (second_id, second) = channel_with_messages(&admin, &user, &["in the second".to_owned()]);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &first_id);
    gui.wait_until_visible(&room(&second_id, room_row::NAME), TIMEOUT)
        .unwrap();
    // Nothing is open yet.
    assert!(
        gui.exists_and_visible(main_window::ROOM_PLACEHOLDER)
            .unwrap()
    );

    open_room(gui, &first_id);
    gui.wait_until_visible(&message(&first[0].id, message_row::BODY), TIMEOUT)
        .unwrap();
    open_room(gui, &second_id);

    gui.wait_until_visible(&message(&second[0].id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&message(&first[0].id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert_eq!(gui.count(main_window::MESSAGE_LIST).unwrap(), 1);
}

// MSG-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn scrolling_to_the_top_loads_older_messages() {
    let admin = Admin::login();
    let user = admin.create_user();
    let total = TOTAL;
    let texts: Vec<String> = (0..total).map(|n| format!("message {n:03}")).collect();
    let (channel_id, posted) = channel_with_messages(&admin, &user, &texts);
    let (oldest, newest) = (&posted[0].id, &posted[total - 1].id);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    // Only the newest page is loaded, and the list is at its end.
    gui.wait_until_visible(&message(newest, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(gui.count(main_window::MESSAGE_LIST).unwrap() < total);
    assert!(
        !gui.exists_and_visible(&message(oldest, message_row::BODY))
            .unwrap()
    );
    assert!(!gui.exists_and_visible(main_window::HISTORY_START).unwrap());

    // Home goes to the top, which loads the rest.
    press_home(gui);
    gui.wait_until("every message loaded", TIMEOUT, || {
        Ok(gui.count(main_window::MESSAGE_LIST)? == total)
    })
    .unwrap();
    // The list keeps what was in view, so the oldest messages are above it: Home again.
    press_home(gui);
    gui.wait_until("the oldest message and the start in view", TIMEOUT, || {
        Ok(gui.exists_and_visible(&message(oldest, message_row::BODY))?
            && gui.exists_and_visible(main_window::HISTORY_START)?)
    })
    .unwrap();
    assert_eq!(
        gui.text(&message(oldest, message_row::BODY)).unwrap(),
        "message 000"
    );
}

// MSG-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn scrolling_up_with_the_wheel_loads_older_messages() {
    // Notches per batch and the batches allowed: the wheel runs until the condition holds, so the
    // window size does not matter, but a broken wheel must not spin forever.
    const NOTCH: i32 = 120;
    const STEPS: u32 = 10;
    const STEP_INTERVAL: Duration = Duration::from_millis(20);
    const MAX_BATCHES: usize = 60;

    let admin = Admin::login();
    let user = admin.create_user();
    let total = TOTAL;
    let texts: Vec<String> = (0..total).map(|n| format!("message {n:03}")).collect();
    let (channel_id, posted) = channel_with_messages(&admin, &user, &texts);
    let (oldest, newest) = (&posted[0].id, &posted[total - 1].id);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    gui.wait_until_visible(&message(newest, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(gui.count(main_window::MESSAGE_LIST).unwrap() < total);
    assert!(!gui.exists_and_visible(main_window::HISTORY_START).unwrap());

    let offset = || {
        gui.scroll_offset(main_window::MESSAGE_LIST, main_window::MESSAGE_LIST_OFFSET)
            .unwrap()
    };
    // The newest message is in view: no way scrolled up. The frames are counted from here, where
    // the app can count them.
    assert!(offset().abs() < 1.0, "{}", offset());
    let frames = gui.frame_count(main_window::WINDOW).unwrap();
    let at_the_start = || -> Result<bool, GuiError> {
        Ok(gui.count(main_window::MESSAGE_LIST)? == total
            && gui.exists_and_visible(&message(oldest, message_row::BODY))?
            && gui.exists_and_visible(main_window::HISTORY_START)?)
    };
    let mut batches = 0;
    while !at_the_start().unwrap() {
        assert!(batches < MAX_BATCHES, "the wheel did not reach the start");
        gui.mouse_wheel(main_window::MESSAGE_LIST, 0, NOTCH, STEPS, STEP_INTERVAL)
            .unwrap();
        batches += 1;
    }
    // Scrolled up past more than a screenful of history, and the view drew while it moved.
    let list_height = gui.bounding_box(main_window::MESSAGE_LIST).unwrap().height;
    assert!(
        offset() > list_height,
        "{} in a list of {list_height}",
        offset()
    );
    let frames_after = gui.frame_count(main_window::WINDOW).unwrap();
    assert_eq!(frames.is_some(), frames_after.is_some());
    assert!(
        frames_after >= frames.map(|frames| frames + 1),
        "{frames:?} then {frames_after:?}"
    );
    assert_eq!(
        gui.text(&message(oldest, message_row::BODY)).unwrap(),
        "message 000"
    );
}

// MSG-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn loading_older_messages_keeps_the_view_in_place() {
    let admin = Admin::login();
    let user = admin.create_user();
    let total = TOTAL;
    let texts: Vec<String> = (0..total).map(|n| format!("message {n:03}")).collect();
    let (channel_id, posted) = channel_with_messages(&admin, &user, &texts);
    // The first message of the newest page: it is at the top of the list when the older ones
    // arrive, and must still be there.
    let anchor = &posted[total - NEWEST_PAGE].id;
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&posted[total - 1].id, message_row::BODY), TIMEOUT)
        .unwrap();

    press_home(gui);
    gui.wait_until("every message loaded", TIMEOUT, || {
        Ok(gui.count(main_window::MESSAGE_LIST)? == total)
    })
    .unwrap();

    // The view settles after the rows are in: wait for the message that was first to be in view,
    // and then check that it stays there.
    let in_view = || -> Result<bool, GuiError> {
        let list = gui.bounding_box(main_window::MESSAGE_LIST)?;
        let path = message(anchor, message_row::BODY);
        if !gui.exists_and_visible(&path)? {
            return Ok(false);
        }
        let row = gui.bounding_box(&path)?;
        Ok(row.y >= list.y && row.y + row.height <= list.y + list.height)
    };
    gui.wait_until(
        "the message that was first to stay in view",
        TIMEOUT,
        in_view,
    )
    .unwrap();
    gui.wait(std::time::Duration::from_millis(300)).unwrap();
    assert!(in_view().unwrap());
}

// MSG-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_first_message_of_the_day_has_a_separator() {
    let admin = Admin::login();
    let user = admin.create_user();
    let texts = ["one".to_owned(), "two".to_owned()];
    let (channel_id, posted) = channel_with_messages(&admin, &user, &texts);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let separator = |n: usize| message(&posted[n].id, message_row::DAY_SEPARATOR);
    gui.wait_until_visible(&separator(0), TIMEOUT).unwrap();
    assert_eq!(gui.text(&separator(0)).unwrap(), "Today");
    gui.wait_until_visible(&message(&posted[1].id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(&separator(1)).unwrap());
}

// MSG-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn system_messages_are_shown_as_events_not_as_chat() {
    let admin = Admin::login();
    let user = admin.create_user();
    let other = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    admin.set_topic(&channel_id, "Lunch at noon");
    let topic = admin.last_message(&channel_id);
    admin.invite(&channel_id, &other.id);
    let added = admin.last_message(&channel_id);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    // The server sends admin's display name with some events (the topic) and only the username
    // with others (adding a user).
    gui.wait_until_visible(&message(&topic.id, message_row::SYSTEM), TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&message(&topic.id, message_row::SYSTEM)).unwrap(),
        format!("{} changed the topic to: Lunch at noon", admin_name(&admin))
    );
    gui.wait_until_visible(&message(&added.id, message_row::SYSTEM), TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&message(&added.id, message_row::SYSTEM)).unwrap(),
        format!("{} added {}", admin.username, other.username)
    );
    // They are events, not something somebody said.
    for id in [&topic.id, &added.id] {
        assert!(
            !gui.exists_and_visible(&message(id, message_row::AUTHOR))
                .unwrap()
        );
        assert!(
            !gui.exists_and_visible(&message(id, message_row::BODY))
                .unwrap()
        );
    }
}

// MSG-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_author_name_that_fits_is_not_elided() {
    let admin = Admin::login();
    // A fixed name: one that was elided, though it fits, until the header took the ceiling of its
    // width. A generated name would make the test pass or fail by its length alone.
    let user = admin.create_user_named("Sergio Martins");
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = post_as(
        &user,
        &channel_id,
        &format!("hello {}", rc_test_server::unique()),
    );
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    // The name fits the row, so it must be shown whole: nothing is elided.
    let author = message(&posted.id, message_row::AUTHOR);
    gui.wait_until_visible(&author, TIMEOUT).unwrap();
    assert_eq!(gui.text(&author).unwrap(), user.name);
    assert!(
        !gui.truncated(&author).unwrap(),
        "the author's name fits and must not be elided"
    );
}
