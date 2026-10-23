// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The list of a room's threads in the pane beside it, against the local test server
//! (`RC_TEST_SERVER_URL`): opened with the button in the room's header, ordered by the last reply,
//! filtered to the threads the user follows, and the way between it and a thread (a row opens the
//! thread, the back button and Escape lead back). Each test makes its own user and channel and only
//! looks at those.

#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{
    main_window, message, message_row, thread_list_row, thread_list_row_part, thread_message,
};
use rc_gui_tests::{App, Key, Modifiers};
use rc_test_server::{Admin, Posted, TestUser, unique};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, open_room, utc_time};

/// A channel with four threads, "t0" to "t3", started by admin, with one reply each, made in the
/// order t1, t3, t0, t2: the most recent last reply is t2's, and the list says t2, t0, t3, t1.
struct Fixture {
    admin: Admin,
    user: TestUser,
    channel_id: String,
    parents: Vec<Posted>,
    /// The replies, in the order of their parents.
    replies: Vec<Posted>,
}

const ORDER: [usize; 4] = [2, 0, 3, 1];

impl Fixture {
    fn new() -> Self {
        let admin = Admin::login();
        let user = admin.create_user();
        let (channel_id, _) = admin.create_channel(&[&user.username]);
        let parents: Vec<Posted> = (0..4)
            .map(|n| admin.post_message(&channel_id, &format!("t{n} {}", unique())))
            .collect();
        let mut replies: Vec<Option<Posted>> = vec![None, None, None, None];
        for n in [1, 3, 0, 2] {
            replies[n] = Some(admin.post_in_thread(
                &channel_id,
                &parents[n].id,
                &format!("reply to t{n}"),
                false,
            ));
        }
        Self {
            admin,
            user,
            channel_id,
            parents,
            replies: replies.into_iter().flatten().collect(),
        }
    }

    /// The app, logged in as the user, with the channel open.
    fn open(&self, app: &App) {
        let gui = app.gui();
        log_in_and_wait_for_room(gui, &self.user, &self.channel_id);
        open_room(gui, &self.channel_id);
        gui.wait_until_visible(&message(&self.parents[0].id, message_row::BODY), TIMEOUT)
            .unwrap();
    }

    fn row(&self, n: usize) -> String {
        thread_list_row(&self.parents[n].id, "")
    }

    fn part(&self, n: usize, part: &str) -> String {
        thread_list_row(&self.parents[n].id, part)
    }
}

/// The vertical position of a row of the list, to compare the order of rows.
fn y_of(gui: &rc_gui_tests::Gui, path: &str) -> f64 {
    gui.bounding_box(path).unwrap().y
}

/// Waits until the list holds `rows` rows, and every one of `expected` is in view in that order.
fn wait_for_order(gui: &rc_gui_tests::Gui, fixture: &Fixture, expected: &[usize]) {
    gui.wait_until("the rows of the list", TIMEOUT, || {
        Ok(gui.count(main_window::THREAD_LIST_VIEW)? == expected.len()
            && expected
                .iter()
                .all(|&n| gui.exists_and_visible(&fixture.row(n)).unwrap_or(false)))
    })
    .unwrap();
    let ys: Vec<f64> = expected
        .iter()
        .map(|&n| y_of(gui, &fixture.row(n)))
        .collect();
    assert!(
        ys.windows(2).all(|pair| pair[0] < pair[1]),
        "{expected:?}: {ys:?}"
    );
}

/// Opens the pane with the header button and waits for the list.
fn open_list(gui: &rc_gui_tests::Gui) {
    gui.mouse_click(main_window::ROOM_THREADS_BUTTON).unwrap();
    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
}

/// Opens the thread of a parent of the room with the summary under it, in the room.
fn open_thread_from_summary(gui: &rc_gui_tests::Gui, fixture: &Fixture, n: usize) {
    gui.mouse_click(&message(
        &fixture.parents[n].id,
        message_row::THREAD_SUMMARY,
    ))
    .unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_header_button_opens_the_threads_of_the_room_with_the_most_recent_reply_first() {
    let fixture = Fixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    assert!(!gui.exists_and_visible(main_window::THREAD_PANE).unwrap());

    open_list(gui);

    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::THREAD_LIST_TITLE).unwrap(), "Threads");
    assert!(
        !gui.exists_and_visible(main_window::THREAD_BACK_BUTTON)
            .unwrap()
    );
    assert!(gui.checked(main_window::THREAD_LIST_FILTER_ALL).unwrap());
    assert!(
        !gui.checked(main_window::THREAD_LIST_FILTER_FOLLOWING)
            .unwrap()
    );
    wait_for_order(gui, &fixture, &ORDER);
    // Each row says who started the thread, when it last had a reply and how many it has.
    let n = ORDER[0];
    gui.wait_until_text(
        &fixture.part(n, thread_list_row_part::SUMMARY),
        "1 reply",
        TIMEOUT,
    )
    .unwrap();
    let author = fixture
        .admin
        .name
        .as_deref()
        .unwrap_or(&fixture.admin.username);
    assert_eq!(
        gui.text(&fixture.part(n, thread_list_row_part::AUTHOR))
            .unwrap(),
        author
    );
    assert_eq!(
        gui.text(&fixture.part(n, thread_list_row_part::TIME))
            .unwrap(),
        utc_time(&fixture.replies[n].ts)
    );
    assert!(
        gui.text(&fixture.part(n, thread_list_row_part::TEXT))
            .unwrap()
            .starts_with("t2 ")
    );
    assert!(
        gui.exists_and_visible(&fixture.part(n, thread_list_row_part::AVATAR_PLACEHOLDER))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::THREAD_LIST_EMPTY)
            .unwrap()
    );
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_header_button_toggles_the_pane_whatever_it_shows() {
    let fixture = Fixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);

    open_list(gui);
    gui.mouse_click(main_window::ROOM_THREADS_BUTTON).unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();

    // With a thread open it closes the thread and the list: the whole pane.
    open_list(gui);
    wait_for_order(gui, &fixture, &ORDER);
    gui.mouse_click(&fixture.row(0)).unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::ROOM_THREADS_BUTTON).unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    // Opened again it is the list, at the front, and no thread is open.
    open_list(gui);
    assert!(
        !gui.exists_and_visible(main_window::THREAD_BACK_BUTTON)
            .unwrap()
    );
    gui.mouse_click(main_window::THREAD_CLOSE).unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn following_lists_the_threads_the_user_started_or_replied_to() {
    let fixture = Fixture::new();
    // The user replies to t1: they follow it from then on.
    let me = Admin::login_as(&fixture.user.username, &fixture.user.password);
    me.post_in_thread(&fixture.channel_id, &fixture.parents[1].id, "me too", false);
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_list(gui);
    wait_for_order(gui, &fixture, &[1, 2, 0, 3]);

    gui.mouse_click(main_window::THREAD_LIST_FILTER_FOLLOWING)
        .unwrap();

    gui.wait_until("only t1 to be listed", TIMEOUT, || {
        Ok(gui.count(main_window::THREAD_LIST_VIEW)? == 1
            && gui.exists_and_visible(&fixture.row(1))?)
    })
    .unwrap();
    assert!(
        gui.checked(main_window::THREAD_LIST_FILTER_FOLLOWING)
            .unwrap()
    );
    assert!(!gui.checked(main_window::THREAD_LIST_FILTER_ALL).unwrap());
    gui.wait_until_text(
        &fixture.part(1, thread_list_row_part::SUMMARY),
        "2 replies",
        TIMEOUT,
    )
    .unwrap();
    gui.wait_until_text(
        &fixture.part(1, thread_list_row_part::TIME),
        &utc_time(&me_last(&me, &fixture)),
        TIMEOUT,
    )
    .unwrap();

    gui.mouse_click(main_window::THREAD_LIST_FILTER_ALL)
        .unwrap();
    wait_for_order(gui, &fixture, &[1, 2, 0, 3]);
}

/// When the last reply of the thread of `parents[1]` was made, which is the user's.
fn me_last(me: &Admin, fixture: &Fixture) -> String {
    me.last_in_thread(&fixture.parents[1].id)
        .map(|posted| posted.ts)
        .unwrap()
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_row_opens_its_thread_and_back_returns_to_the_list_with_its_filter() {
    let fixture = Fixture::new();
    let me = Admin::login_as(&fixture.user.username, &fixture.user.password);
    me.post_in_thread(&fixture.channel_id, &fixture.parents[1].id, "me too", false);
    me.post_in_thread(
        &fixture.channel_id,
        &fixture.parents[3].id,
        "and this",
        false,
    );
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_list(gui);
    gui.mouse_click(main_window::THREAD_LIST_FILTER_FOLLOWING)
        .unwrap();
    gui.wait_until("the followed threads", TIMEOUT, || {
        Ok(gui.count(main_window::THREAD_LIST_VIEW)? == 2)
    })
    .unwrap();

    gui.mouse_click(&fixture.row(1)).unwrap();

    // The thread is in the pane: back button and title, the parent and its replies, the composer.
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::THREAD_TITLE).unwrap(), "Thread");
    assert!(!gui.exists_and_visible(main_window::THREAD_LIST).unwrap());
    gui.wait_until("the parent in the pane", TIMEOUT, || {
        Ok(gui
            .text(&thread_message(&fixture.parents[1].id, message_row::BODY))?
            .starts_with("t1 "))
    })
    .unwrap();
    gui.wait_until_text(
        &thread_message(&fixture.replies[1].id, message_row::BODY),
        "reply to t1",
        TIMEOUT,
    )
    .unwrap();

    gui.mouse_click(main_window::THREAD_BACK_BUTTON).unwrap();

    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::THREAD_BACK_BUTTON)
            .unwrap()
    );
    // The list is as it was: the followed threads, in the filter the user chose.
    assert!(
        gui.checked(main_window::THREAD_LIST_FILTER_FOLLOWING)
            .unwrap()
    );
    assert_eq!(gui.count(main_window::THREAD_LIST_VIEW).unwrap(), 2);
    assert!(gui.exists_and_visible(&fixture.row(3)).unwrap());
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_summary_under_a_message_opens_its_thread_and_back_leads_to_the_list() {
    let fixture = Fixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);

    open_thread_from_summary(gui, &fixture, 0);

    gui.wait_until_text(main_window::THREAD_TITLE, "Thread", TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(main_window::THREAD_LIST).unwrap());
    gui.mouse_click(main_window::THREAD_BACK_BUTTON).unwrap();

    // The list was not open: back opens it, with all the threads.
    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
    wait_for_order(gui, &fixture, &ORDER);
    assert!(gui.checked(main_window::THREAD_LIST_FILTER_ALL).unwrap());
    // And the close button of a thread closes the whole pane, the list it hides included.
    gui.mouse_click(&fixture.row(2)).unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::THREAD_CLOSE).unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    open_list(gui);
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn escape_in_a_thread_goes_back_to_the_list_and_in_the_list_closes_it() {
    let fixture = Fixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_thread_from_summary(gui, &fixture, 3);
    gui.mouse_click(main_window::THREAD_COMPOSER_FIELD).unwrap();

    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::ESCAPE,
        Modifiers::NONE,
    )
    .unwrap();

    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::THREAD_BACK_BUTTON)
            .unwrap()
    );
    wait_for_order(gui, &fixture, &ORDER);

    gui.enter_key(main_window::THREAD_LIST, Key::ESCAPE, Modifiers::NONE)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();

    // Escape in a thread opened from the list goes back to the list that is open already.
    open_list(gui);
    wait_for_order(gui, &fixture, &ORDER);
    gui.mouse_click(&fixture.row(2)).unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::THREAD_COMPOSER_FIELD).unwrap();
    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::ESCAPE,
        Modifiers::NONE,
    )
    .unwrap();
    gui.wait_until_visible(main_window::THREAD_LIST, TIMEOUT)
        .unwrap();
    wait_for_order(gui, &fixture, &ORDER);
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_live_reply_moves_its_thread_to_the_top() {
    let fixture = Fixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_list(gui);
    wait_for_order(gui, &fixture, &ORDER);

    // The thread at the bottom gets a reply.
    let last = ORDER[3];
    fixture.admin.post_in_thread(
        &fixture.channel_id,
        &fixture.parents[last].id,
        "late",
        false,
    );

    gui.wait_until_text(
        &fixture.part(last, thread_list_row_part::SUMMARY),
        "2 replies",
        TIMEOUT,
    )
    .unwrap();
    // The time of the last reply is at the top right of the row.
    gui.wait_until_text(
        &fixture.part(last, thread_list_row_part::TIME),
        &utc_time(
            &fixture
                .admin
                .last_in_thread(&fixture.parents[last].id)
                .unwrap()
                .ts,
        ),
        TIMEOUT,
    )
    .unwrap();
    wait_for_order(gui, &fixture, &[1, 2, 0, 3]);
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_that_gets_its_first_reply_joins_the_list_at_the_top() {
    let fixture = Fixture::new();
    let fresh = fixture.admin.post_message(&fixture.channel_id, "fresh");
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_list(gui);
    wait_for_order(gui, &fixture, &ORDER);

    fixture
        .admin
        .post_in_thread(&fixture.channel_id, &fresh.id, "first!", false);

    let row = thread_list_row(&fresh.id, "");
    gui.wait_until_visible(&row, TIMEOUT).unwrap();
    assert!(y_of(gui, &row) < y_of(gui, &fixture.row(ORDER[0])));
    assert_eq!(gui.count(main_window::THREAD_LIST_VIEW).unwrap(), 5);
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_empty_filter_says_so_in_its_own_words() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, name) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, &format!("only message {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();

    open_list(gui);

    gui.wait_until_visible(main_window::THREAD_LIST_EMPTY, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::THREAD_LIST_EMPTY_TITLE).unwrap(),
        format!("No threads in #{name} yet")
    );
    assert_eq!(
        gui.text(main_window::THREAD_LIST_EMPTY_HINT).unwrap(),
        "Start one with Reply in thread on a message."
    );

    gui.mouse_click(main_window::THREAD_LIST_FILTER_FOLLOWING)
        .unwrap();
    gui.wait_until_text(
        main_window::THREAD_LIST_EMPTY_TITLE,
        &format!("No threads you follow in #{name}"),
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(
        gui.text(main_window::THREAD_LIST_EMPTY_HINT).unwrap(),
        "Reply in a thread to follow it."
    );

    // The first reply makes the thread, and the empty words go.
    admin.post_in_thread(&channel_id, &parent.id, "a reply", false);
    gui.mouse_click(main_window::THREAD_LIST_FILTER_ALL)
        .unwrap();
    gui.wait_until_visible(&thread_list_row(&parent.id, ""), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::THREAD_LIST_EMPTY)
            .unwrap()
    );
}

// ACT-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_list_loads_more_threads_when_a_page_does_not_fill_it() {
    let fixture = Fixture::new();
    // One thread at a time.
    let app = App::launch_with(&[("TZ", "UTC"), ("RC_THREADS_PAGE_SIZE", "1")]).unwrap();
    let gui = app.gui();
    fixture.open(&app);

    open_list(gui);

    // Each page is one row, and the end of the list is in view until the pane is full or the last
    // thread is there: the pages follow each other.
    wait_for_order(gui, &fixture, &ORDER);
    gui.wait_until_hidden(main_window::THREAD_LIST_LOADING_MORE, TIMEOUT)
        .unwrap();
}

/// A channel with two threads: "mine", which the user started (so they follow it), and "theirs",
/// started by admin, each with a reply of the starter's own, so that nothing is unread yet.
struct UnreadFixture {
    admin: Admin,
    user: TestUser,
    channel_id: String,
    channel_name: String,
    mine: Posted,
    theirs: Posted,
}

impl UnreadFixture {
    fn new() -> Self {
        let admin = Admin::login();
        let user = admin.create_user();
        let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
        let me = Admin::login_as(&user.username, &user.password);
        let mine = me.post_message(&channel_id, &format!("mine {}", unique()));
        me.post_in_thread(&channel_id, &mine.id, "my own reply", false);
        let theirs = admin.post_message(&channel_id, &format!("theirs {}", unique()));
        admin.post_in_thread(&channel_id, &theirs.id, "their own reply", false);
        Self {
            admin,
            user,
            channel_id,
            channel_name,
            mine,
            theirs,
        }
    }

    fn open(&self, app: &App) {
        let gui = app.gui();
        log_in_and_wait_for_room(gui, &self.user, &self.channel_id);
        open_room(gui, &self.channel_id);
        gui.wait_until_visible(&message(&self.theirs.id, message_row::BODY), TIMEOUT)
            .unwrap();
        assert!(
            !gui.exists_and_visible(main_window::ROOM_THREADS_BADGE)
                .unwrap()
        );
    }
}

/// A part of the row of the thread started by `parent`.
fn row_of(parent: &Posted, part: &str) -> String {
    thread_list_row(&parent.id, part)
}

// ACT-011
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_in_a_followed_thread_shows_a_badge_on_the_button_and_marks_the_row_unread() {
    let fixture = UnreadFixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_list(gui);
    gui.wait_until_visible(&row_of(&fixture.mine, ""), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&row_of(&fixture.mine, thread_list_row_part::NEW))
            .unwrap()
    );

    fixture
        .admin
        .post_in_thread(&fixture.channel_id, &fixture.mine.id, "news", false);

    // The badge says one, in grey, and the row says it is new, without a mention.
    gui.wait_until_visible(main_window::ROOM_THREADS_BADGE, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_THREADS_BADGE_COUNT, "1", TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::ROOM_THREADS_BADGE).unwrap(), "unread");
    gui.wait_until_visible(&row_of(&fixture.mine, thread_list_row_part::NEW), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&row_of(&fixture.mine, thread_list_row_part::MENTION))
            .unwrap()
    );
    // The other thread is not.
    assert!(
        !gui.exists_and_visible(&row_of(&fixture.theirs, thread_list_row_part::NEW))
            .unwrap()
    );
    gui.wait_until_text(main_window::THREAD_LIST_FILTER_UNREAD_COUNT, "1", TIMEOUT)
        .unwrap();
}

// ACT-011
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_that_mentions_the_user_turns_the_badge_red_and_shows_the_at() {
    let fixture = UnreadFixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    open_list(gui);
    gui.wait_until_visible(&row_of(&fixture.theirs, ""), TIMEOUT)
        .unwrap();
    fixture
        .admin
        .post_in_thread(&fixture.channel_id, &fixture.mine.id, "news", false);
    gui.wait_until_text(main_window::ROOM_THREADS_BADGE_COUNT, "1", TIMEOUT)
        .unwrap();

    // A mention counts in a thread the user doesn't follow, too.
    fixture.admin.post_in_thread(
        &fixture.channel_id,
        &fixture.theirs.id,
        &format!("hey @{}", fixture.user.username),
        false,
    );

    gui.wait_until_text(main_window::ROOM_THREADS_BADGE_COUNT, "2", TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_THREADS_BADGE, "mention", TIMEOUT)
        .unwrap();
    gui.wait_until_visible(
        &row_of(&fixture.theirs, thread_list_row_part::MENTION),
        TIMEOUT,
    )
    .unwrap();
    assert!(
        gui.exists_and_visible(&row_of(&fixture.theirs, thread_list_row_part::NEW))
            .unwrap()
    );
    // Only that thread mentions.
    assert!(
        !gui.exists_and_visible(&row_of(&fixture.mine, thread_list_row_part::MENTION))
            .unwrap()
    );
}

// ACT-011
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_unread_filter_lists_only_the_unread_threads_and_opening_one_reads_it() {
    let fixture = UnreadFixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    fixture
        .admin
        .post_in_thread(&fixture.channel_id, &fixture.mine.id, "news", false);
    gui.wait_until_text(main_window::ROOM_THREADS_BADGE_COUNT, "1", TIMEOUT)
        .unwrap();
    open_list(gui);
    gui.wait_until_visible(&row_of(&fixture.theirs, ""), TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::THREAD_LIST_FILTER_UNREAD)
        .unwrap();

    // Only the unread thread, in the filter that says so.
    gui.wait_until("only the unread thread", TIMEOUT, || {
        Ok(gui.count(main_window::THREAD_LIST_VIEW)? == 1
            && gui.exists_and_visible(&row_of(&fixture.mine, ""))?)
    })
    .unwrap();
    assert!(gui.checked(main_window::THREAD_LIST_FILTER_UNREAD).unwrap());

    // Opening it reads it: the badge and the count go, and so does the row's mark.
    gui.mouse_click(&row_of(&fixture.mine, "")).unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_THREADS_BADGE, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_LIST_FILTER_UNREAD_COUNT, TIMEOUT)
        .unwrap();

    // Back to the list: the thread has left the unread ones, and the filter says so.
    gui.mouse_click(main_window::THREAD_BACK_BUTTON).unwrap();
    gui.wait_until_visible(main_window::THREAD_LIST_EMPTY, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::THREAD_LIST_EMPTY_TITLE).unwrap(),
        format!("No unread threads in #{}", fixture.channel_name)
    );
    assert_eq!(
        gui.text(main_window::THREAD_LIST_EMPTY_HINT).unwrap(),
        "New replies in a thread you follow show up here."
    );
    gui.mouse_click(main_window::THREAD_LIST_FILTER_ALL)
        .unwrap();
    gui.wait_until_visible(&row_of(&fixture.mine, ""), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&row_of(&fixture.mine, thread_list_row_part::NEW))
            .unwrap()
    );
}

// ACT-011
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_that_comes_while_the_thread_is_open_does_not_leave_it_unread() {
    let fixture = UnreadFixture::new();
    let app = launch();
    let gui = app.gui();
    fixture.open(&app);
    gui.mouse_click(&message(&fixture.mine.id, message_row::THREAD_SUMMARY))
        .unwrap();
    gui.wait_until_visible(main_window::THREAD_BACK_BUTTON, TIMEOUT)
        .unwrap();

    let reply = fixture.admin.post_in_thread(
        &fixture.channel_id,
        &fixture.mine.id,
        "while you look",
        false,
    );

    gui.wait_until_text(
        &thread_message(&reply.id, message_row::BODY),
        "while you look",
        TIMEOUT,
    )
    .unwrap();
    // The badge may show for a moment: it is gone once the server has read it.
    std::thread::sleep(std::time::Duration::from_millis(500));
    gui.wait_until_hidden(main_window::ROOM_THREADS_BADGE, TIMEOUT)
        .unwrap();
}

// ACT-011
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn threads_that_were_unread_before_the_room_opened_show_as_such() {
    let fixture = UnreadFixture::new();
    fixture
        .admin
        .post_in_thread(&fixture.channel_id, &fixture.mine.id, "news", false);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &fixture.user, &fixture.channel_id);
    open_room(gui, &fixture.channel_id);

    gui.wait_until_text(main_window::ROOM_THREADS_BADGE_COUNT, "1", TIMEOUT)
        .unwrap();
    open_list(gui);
    gui.wait_until_visible(&row_of(&fixture.mine, thread_list_row_part::NEW), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&row_of(&fixture.theirs, thread_list_row_part::NEW))
            .unwrap()
    );
}
