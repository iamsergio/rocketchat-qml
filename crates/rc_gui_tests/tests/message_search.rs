// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The search of a room's messages in the pane beside it, against the local test server
//! (`RC_TEST_SERVER_URL`): the header button and Ctrl+F, the results newest first with their day,
//! the words marked, a reply in a thread, the notes, the pane taking the place of the threads, and
//! the keys. Each test makes its own user and channels. The Qt app only for now.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{main_window, message_search_row, message_search_row_part as part};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::{Admin, Posted, TestUser};

use common::{TIMEOUT, log_in_and_wait_for_room, open_room};

/// Starts the app without the pause before the messages are searched for, and in UTC.
fn launch() -> App {
    App::launch_with(&[("TZ", "UTC"), ("RC_MESSAGE_SEARCH_DEBOUNCE_MS", "0")]).unwrap()
}

/// A channel the user is in, with these messages from admin, oldest first, and the reply in the
/// thread of the last one.
struct Fixture {
    user: TestUser,
    channel_id: String,
    channel_name: String,
    messages: Vec<Posted>,
    reply: Posted,
}

impl Fixture {
    fn new(admin: &Admin) -> Self {
        let user = admin.create_user();
        let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
        let messages: Vec<Posted> = [
            "the alpha plan",
            "only beta here",
            "Alpha again, ALPHA",
            "a parent without the word",
        ]
        .iter()
        .map(|text| admin.post_message(&channel_id, text))
        .collect();
        let reply = admin.post_in_thread(&channel_id, &messages[3].id, "alpha in a thread", false);
        Self {
            user,
            channel_id,
            channel_name,
            messages,
            reply,
        }
    }

    /// The app, logged in as the user, with the channel open.
    fn open(&self, gui: &Gui) {
        log_in_and_wait_for_room(gui, &self.user, &self.channel_id);
        open_room(gui, &self.channel_id);
        gui.wait_until_visible(main_window::ROOM_SEARCH_MESSAGES_BUTTON, TIMEOUT)
            .unwrap();
    }
}

/// The element `part` of the result row of `message`.
fn row(message: &Posted, part: &str) -> String {
    message_search_row(&message.id, part)
}

/// Opens the pane with the header button.
fn open_search(gui: &Gui) {
    gui.mouse_click(main_window::ROOM_SEARCH_MESSAGES_BUTTON)
        .unwrap();
    gui.wait_until_visible(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();
}

/// Types `text` in the field, which has the focus once the pane is open.
fn search_for(gui: &Gui, text: &str) {
    gui.input_text(main_window::MESSAGE_SEARCH_FIELD, text)
        .unwrap();
}

fn press(gui: &Gui, key: Key) {
    gui.enter_key(main_window::MESSAGE_SEARCH_FIELD, key, Modifiers::NONE)
        .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_header_button_and_ctrl_f_open_the_pane_with_the_field_focused() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);

    open_search(gui);

    assert_eq!(
        gui.text(main_window::MESSAGE_SEARCH_TITLE).unwrap(),
        "Search"
    );
    assert_eq!(
        gui.text(main_window::MESSAGE_SEARCH_ROOM).unwrap(),
        format!("in #{}", fixture.channel_name)
    );
    gui.wait_until_text(
        main_window::MESSAGE_SEARCH_EMPTY_TITLE,
        &format!("Search #{}", fixture.channel_name),
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(
        gui.text(main_window::MESSAGE_SEARCH_HINT).unwrap(),
        "Whole words, any case"
    );
    // The focus is in the field: what is typed goes there.
    search_for(gui, "abc");
    gui.wait_until_text(main_window::MESSAGE_SEARCH_FIELD, "abc", TIMEOUT)
        .unwrap();

    // The button closes it again.
    gui.mouse_click(main_window::ROOM_SEARCH_MESSAGES_BUTTON)
        .unwrap();
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();

    // Ctrl+F opens it, with the field empty.
    gui.enter_key(main_window::COMPOSER_FIELD, Key::F, Modifiers::CONTROL)
        .unwrap();
    gui.wait_until_visible(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::MESSAGE_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    search_for(gui, "abc");
    gui.wait_until_text(main_window::MESSAGE_SEARCH_FIELD, "abc", TIMEOUT)
        .unwrap();
    // Ctrl+F with the pane open selects the text: the next typing replaces it.
    gui.enter_key(main_window::COMPOSER_FIELD, Key::F, Modifiers::CONTROL)
        .unwrap();
    search_for(gui, "xyz");
    gui.wait_until_text(main_window::MESSAGE_SEARCH_FIELD, "xyz", TIMEOUT)
        .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_word_lists_the_messages_that_hold_it_newest_first_with_the_word_marked() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);
    open_search(gui);

    search_for(gui, "alpha");

    let (first, second, third) = (&fixture.reply, &fixture.messages[2], &fixture.messages[0]);
    for found in [first, second, third] {
        gui.wait_until_visible(&row(found, ""), TIMEOUT).unwrap();
    }
    // Only those: three messages and the label of their day.
    gui.wait_until("the rows", TIMEOUT, || {
        Ok(gui.count(main_window::MESSAGE_SEARCH_LIST)? == 4)
    })
    .unwrap();
    for other in [&fixture.messages[1], &fixture.messages[3]] {
        assert!(!gui.exists_and_visible(&row(other, "")).unwrap());
    }
    assert_eq!(gui.text(main_window::MESSAGE_SEARCH_DAY).unwrap(), "Today");
    let y = |found: &Posted| gui.bounding_box(&row(found, "")).unwrap().y;
    assert!(y(first) < y(second) && y(second) < y(third));

    // The word is marked, whatever its case, and the text is not the whole message otherwise.
    let text = gui.text(&row(third, part::TEXT)).unwrap();
    assert!(text.contains("<b>alpha</b>"), "{text}");
    let text = gui.text(&row(second, part::TEXT)).unwrap();
    assert!(
        text.contains("<b>Alpha</b>") && text.contains("<b>ALPHA</b>"),
        "{text}"
    );
    assert_eq!(
        gui.text(&row(third, part::AUTHOR)).unwrap(),
        gui.text(&row(second, part::AUTHOR)).unwrap()
    );
    assert!(!gui.text(&row(third, part::TIME)).unwrap().is_empty());

    // A reply in a thread says so; the others don't.
    assert!(gui.exists_and_visible(&row(first, part::THREAD)).unwrap());
    assert!(!gui.exists_and_visible(&row(second, part::THREAD)).unwrap());
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_word_nobody_wrote_says_so_and_clearing_the_field_goes_back_to_the_note() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);
    open_search(gui);

    search_for(gui, "gamma");

    gui.wait_until_visible(main_window::MESSAGE_SEARCH_NO_RESULTS, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::MESSAGE_SEARCH_NO_RESULTS_TITLE)
            .unwrap(),
        "No messages with “gamma”"
    );
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_SEARCH_EMPTY)
            .unwrap()
    );

    // The cross empties the field; the note says what to do again.
    gui.mouse_click(main_window::MESSAGE_SEARCH_CLEAR).unwrap();
    gui.wait_until_visible(main_window::MESSAGE_SEARCH_EMPTY, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_NO_RESULTS, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_CLEAR, TIMEOUT)
        .unwrap();

    // Results, then the field emptied by hand: the rows go.
    search_for(gui, "beta");
    gui.wait_until_visible(&row(&fixture.messages[1], ""), TIMEOUT)
        .unwrap();
    gui.set_text(main_window::MESSAGE_SEARCH_FIELD, "").unwrap();
    gui.wait_until_visible(main_window::MESSAGE_SEARCH_EMPTY, TIMEOUT)
        .unwrap();
    gui.wait_until("no rows", TIMEOUT, || {
        Ok(gui.count(main_window::MESSAGE_SEARCH_LIST)? == 0)
    })
    .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_search_and_the_threads_take_turns_in_the_pane() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);

    // The threads, then the search: the list goes.
    gui.mouse_click(main_window::ROOM_THREADS_BUTTON).unwrap();
    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
    open_search(gui);
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();

    // The threads button opens the list and closes the search.
    gui.mouse_click(main_window::ROOM_THREADS_BUTTON).unwrap();
    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();

    // A thread open, then the search: the thread goes.
    gui.wait_until_visible(&common_thread_row(&fixture), TIMEOUT)
        .unwrap();
    gui.mouse_click(&common_thread_row(&fixture)).unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    open_search(gui);
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
}

/// The row of the thread of the fixture in the list of threads.
fn common_thread_row(fixture: &Fixture) -> String {
    rc_gui_tests::paths::thread_list_row(&fixture.messages[3].id, "")
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn escape_and_the_close_button_close_the_pane() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);
    open_search(gui);

    press(gui, Key::ESCAPE);
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();

    open_search(gui);
    gui.mouse_click(main_window::MESSAGE_SEARCH_CLOSE).unwrap();
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn another_room_keeps_the_pane_open_with_an_empty_field() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let (other_id, other_name) = admin.create_channel(&[&fixture.user.username]);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);
    open_search(gui);
    search_for(gui, "alpha");
    gui.wait_until_visible(&row(&fixture.messages[0], ""), TIMEOUT)
        .unwrap();

    gui.wait_until_visible(
        &rc_gui_tests::paths::room(&other_id, rc_gui_tests::paths::room_row::NAME),
        TIMEOUT,
    )
    .unwrap();
    open_room(gui, &other_id);

    gui.wait_until_text(main_window::MESSAGE_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        main_window::MESSAGE_SEARCH_EMPTY_TITLE,
        &format!("Search #{other_name}"),
        TIMEOUT,
    )
    .unwrap();
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
    assert_eq!(
        gui.text(main_window::MESSAGE_SEARCH_ROOM).unwrap(),
        format!("in #{other_name}")
    );
    gui.wait_until("no rows", TIMEOUT, || {
        Ok(gui.count(main_window::MESSAGE_SEARCH_LIST)? == 0)
    })
    .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn up_and_down_move_the_cursor_through_the_messages_and_wrap() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    fixture.open(gui);
    open_search(gui);
    search_for(gui, "alpha");
    let rows = [&fixture.reply, &fixture.messages[2], &fixture.messages[0]];
    gui.wait_until_visible(&row(rows[2], ""), TIMEOUT).unwrap();
    let on = |expected: usize| {
        gui.wait_until("the cursor", TIMEOUT, || {
            for (n, found) in rows.iter().enumerate() {
                if gui.checked(&row(found, ""))? != (n == expected) {
                    return Ok(false);
                }
            }
            Ok(true)
        })
        .unwrap();
    };

    // On the newest result to begin with.
    on(0);
    press(gui, Key::DOWN);
    on(1);
    press(gui, Key::DOWN);
    on(2);
    // Past the last it wraps, over the label of the day.
    press(gui, Key::DOWN);
    on(0);
    press(gui, Key::UP);
    on(2);
    // Enter doesn't leave the search yet.
    press(gui, Key::RETURN);
    on(2);
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
}
