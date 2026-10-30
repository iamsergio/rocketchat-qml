// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The search of a room's messages in the pane beside it, against the local test server
//! (`RC_TEST_SERVER_URL`): the header button and Ctrl+F, the results newest first with their day,
//! the words marked, a reply in a thread, the notes, the pane taking the place of the threads, and
//! the keys, and showing a result in the room: marked, with the pill when the room holds only a
//! window onto the past (a history page of a few messages, `RC_HISTORY_PAGE_SIZE`). Each test makes
//! its own user and channels. The Qt app only for now.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use rc_gui_tests::paths::{
    main_window, message, message_row, message_search_row, message_search_row_part as part,
    thread_message,
};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::{Admin, Posted, TestUser};

use common::{TIMEOUT, log_in_and_wait_for_room, open_room, type_and_send};

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
        "Whole words · from:user · has:url · on:dd/mm/yyyy"
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
    // Enter shows the message in the room and keeps the search open, the cursor where it was.
    press(gui, Key::RETURN);
    gui.wait_until_visible(&message(&rows[2].id, message_row::FOUND), TIMEOUT)
        .unwrap();
    on(2);
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
}

/// How many messages a history page holds in the tests of showing a result, and how many filler
/// messages come between the oldest result and the newest.
const PAGE: &str = "6";
const FILLERS: usize = 12;

/// Starts the app with a small history page, so that a channel of a few posts has messages the
/// room does not hold.
fn launch_paged() -> App {
    App::launch_with(&[
        ("TZ", "UTC"),
        ("RC_MESSAGE_SEARCH_DEBOUNCE_MS", "0"),
        ("RC_HISTORY_PAGE_SIZE", PAGE),
    ])
    .unwrap()
}

/// A channel the user is in with a result older than the page the room loads (`first`), twelve
/// fillers of many lines each (so that a few of them fill the view) and a result among the newest
/// (`last`), all with the word "zebra" in the results only.
struct History {
    user: TestUser,
    channel_id: String,
    first: Posted,
    last: Posted,
}

impl History {
    fn new(admin: &Admin) -> Self {
        let user = admin.create_user();
        let (channel_id, _) = admin.create_channel(&[&user.username]);
        let first = admin.post_message(&channel_id, "zebra at the start");
        let tall = "line\n".repeat(14);
        for n in 0..FILLERS {
            admin.post_message(&channel_id, &format!("filler {n}\n{tall}"));
        }
        let last = admin.post_message(&channel_id, "zebra at the end");
        Self {
            user,
            channel_id,
            first,
            last,
        }
    }

    /// The app, logged in, with the room open and the search for "zebra" showing its two results.
    fn open_and_search(&self, gui: &Gui) {
        log_in_and_wait_for_room(gui, &self.user, &self.channel_id);
        open_room(gui, &self.channel_id);
        gui.wait_until_visible(&message(&self.last.id, message_row::BODY), TIMEOUT)
            .unwrap();
        open_search(gui);
        search_for(gui, "zebra");
        for found in [&self.first, &self.last] {
            gui.wait_until_visible(&row(found, ""), TIMEOUT).unwrap();
        }
    }
}

fn pill_shows(gui: &Gui) -> bool {
    gui.exists_and_visible(main_window::ROOM_JUMP_TO_LATEST)
        .unwrap()
}

fn marked(gui: &Gui, found: &Posted) -> bool {
    gui.exists_and_visible(&message(&found.id, message_row::FOUND))
        .unwrap()
}

fn picked(gui: &Gui, found: &Posted) -> bool {
    gui.exists_and_visible(&row(found, part::PICKED)).unwrap()
}

/// Jumps to the oldest result with a click: the room holds a window around it.
fn jump_to_the_first(gui: &Gui, history: &History) {
    gui.mouse_click(&row(&history.first, "")).unwrap();
    gui.wait_until_visible(&message(&history.first.id, message_row::FOUND), TIMEOUT)
        .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_click_on_a_recent_result_marks_the_message_in_the_room_and_picks_the_row() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    assert!(!marked(gui, &history.last) && !picked(gui, &history.last));

    gui.mouse_click(&row(&history.last, "")).unwrap();

    gui.wait_until_visible(&message(&history.last.id, message_row::FOUND), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&row(&history.last, part::PICKED), TIMEOUT)
        .unwrap();
    // The room holds it: nothing was cut off, and the search stays open on the other row too.
    assert!(!pill_shows(gui));
    assert!(!picked(gui, &history.first));
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_SEARCH_MISSING)
            .unwrap()
    );
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn enter_on_the_cursors_row_shows_the_message_like_a_click() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);

    // The cursor starts on the newest result.
    press(gui, Key::RETURN);

    gui.wait_until_visible(&message(&history.last.id, message_row::FOUND), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&row(&history.last, part::PICKED), TIMEOUT)
        .unwrap();
    assert!(!pill_shows(gui));

    // Down and Enter: the mark moves to the other message, and so does the pick.
    press(gui, Key::DOWN);
    press(gui, Key::RETURN);
    gui.wait_until_visible(&message(&history.first.id, message_row::FOUND), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&message(&history.last.id, message_row::FOUND), TIMEOUT)
        .unwrap();
    assert!(picked(gui, &history.first) && !picked(gui, &history.last));
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_result_older_than_the_loaded_page_is_marked_in_a_window_with_the_pill() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    assert!(!pill_shows(gui));

    jump_to_the_first(gui, &history);

    gui.wait_until_visible(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&row(&history.first, part::PICKED), TIMEOUT)
        .unwrap();
    // The room does not hold the newest message, and the search is still there.
    assert!(
        !gui.exists_and_visible(&message(&history.last.id, message_row::BODY))
            .unwrap()
    );
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
    // The message is in the view, not left at the edge of the content.
    let list = gui.bounding_box(main_window::MESSAGE_LIST).unwrap();
    let found = gui
        .bounding_box(&message(&history.first.id, message_row::BODY))
        .unwrap();
    assert!(
        found.y >= list.y && found.y < list.y + list.height,
        "{found:?} in {list:?}"
    );
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn scrolling_down_a_window_loads_the_newer_messages_until_the_pill_goes() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    jump_to_the_first(gui, &history);
    gui.wait_until_visible(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();

    // Wheel down until the room joins the newest messages: the pill goes, the newest message is
    // there, and the mark stays on the message that was shown.
    let mut batches = 0;
    while pill_shows(gui) {
        assert!(batches < 60, "the wheel did not reach the newest messages");
        gui.mouse_wheel(
            main_window::MESSAGE_LIST,
            0,
            -120,
            10,
            Duration::from_millis(10),
        )
        .unwrap();
        gui.wait(Duration::from_millis(300)).unwrap();
        batches += 1;
    }
    // The mark is the core's and stays; the message itself may have been scrolled out of the
    // created rows, so the row in the search says it.
    assert!(picked(gui, &history.first));
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_pill_brings_the_newest_messages_and_clears_the_mark() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    jump_to_the_first(gui, &history);
    gui.wait_until_visible(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::ROOM_JUMP_TO_LATEST).unwrap();

    gui.wait_until_hidden(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&history.last.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(!marked(gui, &history.first));
    // The row is not picked any more either.
    gui.wait_until_hidden(&row(&history.first, part::PICKED), TIMEOUT)
        .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn end_in_the_room_shows_the_latest_like_the_pill() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    jump_to_the_first(gui, &history);
    gui.wait_until_visible(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();

    // The keys are in the search field until the messages are clicked.
    gui.mouse_click(&message(&history.first.id, message_row::BODY))
        .unwrap();
    gui.enter_key(main_window::MESSAGE_LIST, Key::END, Modifiers::NONE)
        .unwrap();

    gui.wait_until_hidden(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&history.last.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(!marked(gui, &history.first));
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn sending_while_detached_shows_the_latest() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    jump_to_the_first(gui, &history);
    gui.wait_until_visible(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();

    type_and_send(gui, "hello from afar");

    gui.wait_until_hidden(main_window::ROOM_JUMP_TO_LATEST, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&history.last.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(!marked(gui, &history.first));
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_deleted_message_says_so_in_the_pane_and_leaves_the_room_alone() {
    let admin = Admin::login();
    let history = History::new(&admin);
    let app = launch_paged();
    let gui = app.gui();
    history.open_and_search(gui);
    admin.delete_message(&history.channel_id, &history.first.id);

    gui.mouse_click(&row(&history.first, "")).unwrap();

    gui.wait_until_text(
        main_window::MESSAGE_SEARCH_MISSING,
        "This message no longer exists.",
        TIMEOUT,
    )
    .unwrap();
    assert!(!pill_shows(gui));
    assert!(!picked(gui, &history.first));
    assert!(
        gui.exists_and_visible(&message(&history.last.id, message_row::BODY))
            .unwrap()
    );

    // The next search takes the note away.
    search_for(gui, "s");
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_MISSING, TIMEOUT)
        .unwrap();
}

/// The app with the fixture's channel open and the search for "alpha" showing, the hidden reply
/// among the results.
fn search_alpha(gui: &Gui, fixture: &Fixture) {
    fixture.open(gui);
    open_search(gui);
    search_for(gui, "alpha");
    gui.wait_until_visible(&row(&fixture.reply, ""), TIMEOUT)
        .unwrap();
}

/// Picks the hidden reply: the room marks its parent and the thread opens over the results.
fn pick_the_reply(gui: &Gui, fixture: &Fixture) {
    gui.mouse_click(&row(&fixture.reply, "")).unwrap();
    gui.wait_until_visible(
        &message(&fixture.messages[3].id, message_row::FOUND),
        TIMEOUT,
    )
    .unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(
        &thread_message(&fixture.reply.id, message_row::BODY),
        TIMEOUT,
    )
    .unwrap();
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_hidden_reply_marks_its_parent_in_the_room_and_opens_its_thread_over_the_results() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    search_alpha(gui, &fixture);

    pick_the_reply(gui, &fixture);

    // The thread takes the pane; the results are hidden, not gone. The room did not scroll to
    // nor mark the reply, which it does not show.
    gui.wait_until_hidden(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();
    assert!(!marked(gui, &fixture.reply));
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_SEARCH_MISSING)
            .unwrap()
    );

    // Back returns to the results, with the query and the picked row.
    gui.mouse_click(main_window::THREAD_BACK_BUTTON).unwrap();
    gui.wait_until_visible(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    assert!(picked(gui, &fixture.reply));
    assert!(!picked(gui, &fixture.messages[0]));
    assert!(marked(gui, &fixture.messages[3]));
    assert_eq!(
        gui.text(main_window::MESSAGE_SEARCH_FIELD).unwrap(),
        "alpha"
    );
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn escape_in_the_thread_returns_to_the_results_and_a_plain_result_shows_in_the_room() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    search_alpha(gui, &fixture);
    pick_the_reply(gui, &fixture);

    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::ESCAPE,
        Modifiers::NONE,
    )
    .unwrap();

    gui.wait_until_visible(main_window::MESSAGE_SEARCH_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    assert!(picked(gui, &fixture.reply));

    // A plain result afterwards: no thread, the room marks that message.
    gui.mouse_click(&row(&fixture.messages[0], "")).unwrap();
    gui.wait_until_visible(
        &message(&fixture.messages[0].id, message_row::FOUND),
        TIMEOUT,
    )
    .unwrap();
    assert!(picked(gui, &fixture.messages[0]) && !picked(gui, &fixture.reply));
    assert!(!gui.exists_and_visible(main_window::THREAD_PANE).unwrap());
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_close_button_of_a_thread_over_the_results_closes_both() {
    let admin = Admin::login();
    let fixture = Fixture::new(&admin);
    let app = launch();
    let gui = app.gui();
    search_alpha(gui, &fixture);
    pick_the_reply(gui, &fixture);

    gui.mouse_click(main_window::THREAD_CLOSE).unwrap();

    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_SEARCH_PANE)
            .unwrap()
    );
}

// MSG-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_also_sent_to_the_room_is_marked_itself_and_opens_its_thread() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, "a parent");
    let reply = admin.post_in_thread(&channel_id, &parent.id, "gamma in the room too", true);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(main_window::ROOM_SEARCH_MESSAGES_BUTTON, TIMEOUT)
        .unwrap();
    open_search(gui);
    search_for(gui, "gamma");
    gui.wait_until_visible(&row(&reply, ""), TIMEOUT).unwrap();

    gui.mouse_click(&row(&reply, "")).unwrap();

    gui.wait_until_visible(&message(&reply.id, message_row::FOUND), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    assert!(!marked(gui, &parent));
}
