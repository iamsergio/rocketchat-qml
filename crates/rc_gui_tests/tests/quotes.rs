// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Quoting a message from its menu and the composer, against the local test server
//! (`RC_TEST_SERVER_URL`). The server makes a quote only from a link to its own `Site_Url`, which
//! `Admin::post_quote` builds, so these tests also hold when the app reaches the server by another
//! name. Each test makes its own user and channel and only looks at those.

mod common;

use rc_gui_tests::paths::{main_window, message, message_row, quote, quote_row};
use rc_test_server::{Admin, unique};

use common::{
    TIMEOUT, choose, escape, launch, log_in_and_wait_for_room, open_room, post_as,
    replace_text_and_press_enter, type_and_send, utc_time, wait_for_message_after, wait_until_sent,
};

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_of_somebody_else_is_quoted_through_its_menu() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let original = format!("original **bold** {}", unique());
    let other = admin.post_message(&channel_id, &original);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_QUOTE_BAR)
            .unwrap()
    );

    choose(gui, &other.id, main_window::MESSAGE_QUOTE_ACTION);

    // The bar says whom and what, in the words the message shows.
    gui.wait_until_visible(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    let name = admin.name.as_deref().unwrap_or(&admin.username);
    assert_eq!(gui.text(main_window::COMPOSER_QUOTE_AUTHOR).unwrap(), name);
    assert_eq!(
        gui.text(main_window::COMPOSER_QUOTE_TEXT).unwrap(),
        original
    );
    let reply = format!("my reply {}", unique());
    type_and_send(gui, &reply);

    let sent = wait_for_message_after(&admin, &channel_id, &other.id);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &reply, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
    wait_until_sent(gui);
    // The server's copy has the quote of the original, and the body has no link.
    let first = |part| quote(&sent.id, 0, part);
    gui.wait_until_visible(&first(quote_row::BODY), TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(&first(quote_row::BODY)).unwrap(), original);
    assert_eq!(gui.text(&first(quote_row::AUTHOR)).unwrap(), admin.username);
    assert_eq!(
        gui.text(&first(quote_row::TIME)).unwrap(),
        utc_time(&other.ts)
    );
    assert!(
        gui.text(&first(quote_row::BODY_HTML))
            .unwrap()
            .contains("<b>bold</b>")
    );
    assert!(
        !gui.text(&message(&sent.id, message_row::BODY_HTML))
            .unwrap()
            .contains("http")
    );
    assert!(!gui.exists_and_visible(&quote(&sent.id, 1, "")).unwrap());
    // The original has no quote.
    assert!(!gui.exists_and_visible(&quote(&other.id, 0, "")).unwrap());
}

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn cancelling_with_the_button_or_escape_sends_the_next_message_without_a_quote() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let other = admin.post_message(&channel_id, &format!("original {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();

    // The bar's button.
    choose(gui, &other.id, main_window::MESSAGE_QUOTE_ACTION);
    gui.wait_until_visible(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::COMPOSER_QUOTE_CANCEL).unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    let first = format!("first {}", unique());
    type_and_send(gui, &first);
    let sent = wait_for_message_after(&admin, &channel_id, &other.id);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &first, TIMEOUT)
        .unwrap();
    wait_until_sent(gui);
    assert!(!gui.exists_and_visible(&quote(&sent.id, 0, "")).unwrap());

    // Escape.
    choose(gui, &other.id, main_window::MESSAGE_QUOTE_ACTION);
    gui.wait_until_visible(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    escape(gui);
    gui.wait_until_hidden(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    let second = format!("second {}", unique());
    type_and_send(gui, &second);
    let sent_again = wait_for_message_after(&admin, &channel_id, &sent.id);
    gui.wait_until_text(
        &message(&sent_again.id, message_row::BODY),
        &second,
        TIMEOUT,
    )
    .unwrap();
    wait_until_sent(gui);
    assert!(
        !gui.exists_and_visible(&quote(&sent_again.id, 0, ""))
            .unwrap()
    );
}

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn opening_another_room_drops_the_quote() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (other_channel_id, _) = admin.create_channel(&[&user.username]);
    let other = admin.post_message(&channel_id, &format!("original {}", unique()));
    let elsewhere = admin.post_message(&other_channel_id, &format!("elsewhere {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &other.id, main_window::MESSAGE_QUOTE_ACTION);
    gui.wait_until_visible(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();

    open_room(gui, &other_channel_id);

    gui.wait_until_visible(&message(&elsewhere.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    // What is sent there quotes nothing.
    let text = format!("plain {}", unique());
    type_and_send(gui, &text);
    let sent = wait_for_message_after(&admin, &other_channel_id, &elsewhere.id);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    wait_until_sent(gui);
    assert!(!gui.exists_and_visible(&quote(&sent.id, 0, "")).unwrap());
}

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn quoting_ends_an_edit_and_editing_ends_a_quote() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let mine = format!("mine {}", unique());
    let own = post_as(&user, &channel_id, &mine);
    let theirs = format!("theirs {}", unique());
    let other = admin.post_message(&channel_id, &theirs);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, "my draft")
        .unwrap();
    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);
    gui.wait_until_text(main_window::COMPOSER_FIELD, &mine, TIMEOUT)
        .unwrap();

    // Quoting while editing: the edit ends and the draft is back, with the quote.
    choose(gui, &other.id, main_window::MESSAGE_QUOTE_ACTION);

    gui.wait_until_visible(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "my draft", TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::COMPOSER_QUOTE_TEXT).unwrap(), theirs);
    assert_eq!(gui.text(main_window::COMPOSER_SEND).unwrap(), "Send");

    // Editing while quoting: the quote goes.
    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);

    gui.wait_until_visible(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, &mine, TIMEOUT)
        .unwrap();
    // Escape ends the edit; nothing is left of the quote.
    escape(gui);
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_QUOTE_BAR)
            .unwrap()
    );
    gui.wait_until_text(main_window::COMPOSER_FIELD, "my draft", TIMEOUT)
        .unwrap();
}

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_quote_that_somebody_posts_shows_live_with_what_it_quotes() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, name) = admin.create_channel(&[&user.username]);
    let mine = format!("mine {}", unique());
    let own = post_as(&user, &channel_id, &mine);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();

    let reply = format!("admin replies {}", unique());
    let posted = admin.post_quote(&channel_id, &format!("channel/{name}"), &own.id, &reply);

    gui.wait_until_text(&message(&posted.id, message_row::BODY), &reply, TIMEOUT)
        .unwrap();
    gui.wait_until_text(&quote(&posted.id, 0, quote_row::BODY), &mine, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&quote(&posted.id, 0, quote_row::AUTHOR)).unwrap(),
        user.username
    );
    assert!(
        !gui.text(&message(&posted.id, message_row::BODY_HTML))
            .unwrap()
            .contains("http")
    );
}

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn editing_a_reply_keeps_its_quote() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let original = format!("original {}", unique());
    let other = admin.post_message(&channel_id, &original);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &other.id, main_window::MESSAGE_QUOTE_ACTION);
    let reply = format!("reply {}", unique());
    type_and_send(gui, &reply);
    let sent = wait_for_message_after(&admin, &channel_id, &other.id);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &reply, TIMEOUT)
        .unwrap();
    wait_until_sent(gui);
    gui.wait_until_visible(&quote(&sent.id, 0, quote_row::BODY), TIMEOUT)
        .unwrap();

    choose(gui, &sent.id, main_window::MESSAGE_EDIT_ACTION);

    // The field holds the reply without the link, and no quote bar: the quote is the message's.
    gui.wait_until_text(main_window::COMPOSER_FIELD, &reply, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_QUOTE_BAR)
            .unwrap()
    );
    let edited = format!("{reply} edited");
    replace_text_and_press_enter(gui, &edited);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &edited, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&sent.id, message_row::EDITED), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    // The server kept the quote, because the app sent its link again.
    assert_eq!(
        gui.text(&quote(&sent.id, 0, quote_row::BODY)).unwrap(),
        original
    );
    assert!(
        !gui.text(&message(&sent.id, message_row::BODY_HTML))
            .unwrap()
            .contains("http")
    );
}

// ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_quote_of_a_quote_shows_the_text_of_the_inner_message_without_its_link() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, name) = admin.create_channel(&[&user.username]);
    let path = format!("channel/{name}");
    let original_text = format!("original {}", unique());
    let original = admin.post_message(&channel_id, &original_text);
    let first_text = format!("first reply {}", unique());
    let first = admin.post_quote(&channel_id, &path, &original.id, &first_text);
    let second = admin.post_quote(&channel_id, &path, &first.id, "second reply");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);

    gui.wait_until_text(
        &message(&second.id, message_row::BODY),
        "second reply",
        TIMEOUT,
    )
    .unwrap();

    assert_eq!(
        gui.text(&quote(&second.id, 0, quote_row::BODY)).unwrap(),
        first_text
    );
    assert!(
        !gui.text(&quote(&second.id, 0, quote_row::BODY_HTML))
            .unwrap()
            .contains("http")
    );
    assert!(!gui.exists_and_visible(&quote(&second.id, 1, "")).unwrap());
    // The first reply shows its own quote, of the original.
    assert_eq!(
        gui.text(&quote(&first.id, 0, quote_row::BODY)).unwrap(),
        original_text
    );
}
