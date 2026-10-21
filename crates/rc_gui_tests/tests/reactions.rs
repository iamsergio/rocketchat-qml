// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Reactions to messages, against the local test server (`RC_TEST_SERVER_URL`). The reactions of
//! other users are made through the server's API and the tests wait for the chips to show up; the
//! user's own are made through the message menu, the emoji picker and the chips. The menu and the
//! picker are popups, so their items are found from the window while they are open. Each test makes
//! its own users, channel and emoji and only looks at those.

mod common;

use std::time::Duration;

use rc_gui_tests::paths::{main_window, message, message_row, reaction, reaction_row};
use rc_gui_tests::{Modifiers, MouseButton};
use rc_test_server::{Admin, server_url, unique};

use common::{TIMEOUT, choose, launch, log_in_and_wait_for_room, open_room, pick_emoji};

// RICH-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reaction_shows_a_chip_with_the_emoji_and_the_count_and_taking_it_back_removes_it() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let other = Admin::login_as(&user.username, &user.password);
    let posted = admin.post_message(&channel_id, &format!("react to me {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&posted.id, message_row::BODY), TIMEOUT)
        .unwrap();
    let chip = reaction(&posted.id, 0, "");
    assert!(!gui.exists_and_visible(&chip).unwrap());
    assert!(
        !gui.exists_and_visible(&message(&posted.id, message_row::REACTIONS))
            .unwrap()
    );

    admin.react(&posted.id, ":thumbsup:", true);

    gui.wait_until_visible(&chip, TIMEOUT).unwrap();
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::EMOJI), "👍", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
    // Admin reacted, not the user who looks at the room.
    assert!(!gui.checked(&chip).unwrap());

    // The viewer's own reaction to the same emoji counts, and marks the chip.
    other.react(&posted.id, ":thumbsup:", true);
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "2", TIMEOUT)
        .unwrap();
    gui.wait_until("the chip marked as the user's own", TIMEOUT, || {
        gui.checked(&chip)
    })
    .unwrap();

    // Taking one back changes the count, and the last one removes the chip.
    admin.react(&posted.id, ":thumbsup:", false);
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
    other.react(&posted.id, ":thumbsup:", false);
    gui.wait_until_hidden(&chip, TIMEOUT).unwrap();
    gui.wait_until_hidden(&message(&posted.id, message_row::REACTIONS), TIMEOUT)
        .unwrap();
}

// RICH-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn reactions_are_shown_in_the_order_of_the_server_and_only_the_viewers_own_is_marked() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let viewer = Admin::login_as(&user.username, &user.password);
    let posted = admin.post_message(&channel_id, &format!("two reactions {}", unique()));
    admin.react(&posted.id, ":tada:", true);
    viewer.react(&posted.id, ":rocket:", true);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    // They come with the history this time, not as live updates.
    open_room(gui, &channel_id);

    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::EMOJI), "🎉", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&reaction(&posted.id, 1, reaction_row::EMOJI), "🚀", TIMEOUT)
        .unwrap();
    assert!(!gui.checked(&reaction(&posted.id, 0, "")).unwrap());
    assert!(gui.checked(&reaction(&posted.id, 1, "")).unwrap());
}

// RICH-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_custom_emoji_reaction_shows_the_image_of_the_emoji() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let name = format!("gt_react_{}", unique().replace('-', "_"));
    admin.create_emoji(&name);
    let posted = admin.post_message(&channel_id, &format!("custom reaction {}", unique()));
    admin.react(&posted.id, &format!(":{name}:"), true);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    // The list of custom emoji comes after the rooms; until then the shortcode shows.
    let url = format!(
        "{}/emoji-custom/{name}.png",
        server_url().trim_end_matches('/')
    );
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::EMOJI), &url, TIMEOUT)
        .unwrap();
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
}

// ACT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reaction_is_added_through_the_message_menu_and_the_emoji_picker() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = admin.post_message(&channel_id, &format!("react to me {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&posted.id, message_row::BODY), TIMEOUT)
        .unwrap();
    // No reactions yet, so no "+" either: the menu is the way to the first one.
    assert!(
        !gui.exists_and_visible(&message(&posted.id, message_row::REACTION_ADD))
            .unwrap()
    );

    choose(gui, &posted.id, main_window::MESSAGE_REACT_ACTION);
    pick_emoji(gui, "tada");

    let chip = reaction(&posted.id, 0, "");
    gui.wait_until_visible(&chip, TIMEOUT).unwrap();
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::EMOJI), "🎉", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
    assert!(gui.checked(&chip).unwrap());
    assert!(
        !gui.exists_and_visible(main_window::ROOM_ACTION_ERROR)
            .unwrap()
    );
}

// ACT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn clicking_the_chip_of_the_users_own_reaction_takes_it_back() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let viewer = Admin::login_as(&user.username, &user.password);
    let posted = admin.post_message(&channel_id, &format!("take it back {}", unique()));
    viewer.react(&posted.id, ":rocket:", true);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    let chip = reaction(&posted.id, 0, "");
    gui.wait_until_visible(&chip, TIMEOUT).unwrap();
    assert!(gui.checked(&chip).unwrap());

    gui.mouse_click(&chip).unwrap();

    gui.wait_until_hidden(&chip, TIMEOUT).unwrap();
    gui.wait_until_hidden(&message(&posted.id, message_row::REACTIONS), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::ROOM_ACTION_ERROR)
            .unwrap()
    );
}

// ACT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn clicking_the_chip_of_the_reaction_of_somebody_else_joins_it_and_clicking_again_leaves() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = admin.post_message(&channel_id, &format!("me too {}", unique()));
    admin.react(&posted.id, ":thumbsup:", true);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    let chip = reaction(&posted.id, 0, "");
    let count = reaction(&posted.id, 0, reaction_row::COUNT);
    gui.wait_until_visible(&chip, TIMEOUT).unwrap();
    gui.wait_until_text(&count, "1", TIMEOUT).unwrap();
    assert!(!gui.checked(&chip).unwrap());

    gui.mouse_click(&chip).unwrap();

    gui.wait_until_text(&count, "2", TIMEOUT).unwrap();
    gui.wait_until("the chip marked as the user's own", TIMEOUT, || {
        gui.checked(&chip)
    })
    .unwrap();

    gui.mouse_click(&chip).unwrap();

    gui.wait_until_text(&count, "1", TIMEOUT).unwrap();
    gui.wait_until("the chip no longer marked", TIMEOUT, || {
        gui.checked(&chip).map(|checked| !checked)
    })
    .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::ROOM_ACTION_ERROR)
            .unwrap()
    );
}

// ACT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_plus_chip_adds_another_reaction_after_the_ones_there_are() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = admin.post_message(&channel_id, &format!("one more {}", unique()));
    admin.react(&posted.id, ":thumbsup:", true);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    let add = message(&posted.id, message_row::REACTION_ADD);
    gui.wait_until_visible(&add, TIMEOUT).unwrap();

    gui.mouse_click(&add).unwrap();
    pick_emoji(gui, "rocket");

    gui.wait_until_text(&reaction(&posted.id, 1, reaction_row::EMOJI), "🚀", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&reaction(&posted.id, 1, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
    assert!(gui.checked(&reaction(&posted.id, 1, "")).unwrap());
    // The first one is the other user's, as it was.
    assert!(!gui.checked(&reaction(&posted.id, 0, "")).unwrap());
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
}

// ACT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_custom_emoji_of_the_server_can_be_picked_to_react() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let name = format!("gt_pick_{}", unique().replace('-', "_"));
    admin.create_emoji(&name);
    let posted = admin.post_message(&channel_id, &format!("custom pick {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&posted.id, message_row::BODY), TIMEOUT)
        .unwrap();

    // The picker lists the custom emoji once the app has them, which may be after it opened.
    choose(gui, &posted.id, main_window::MESSAGE_REACT_ACTION);
    pick_emoji(gui, &name);

    let url = format!(
        "{}/emoji-custom/{name}.png",
        server_url().trim_end_matches('/')
    );
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::EMOJI), &url, TIMEOUT)
        .unwrap();
    gui.wait_until_text(&reaction(&posted.id, 0, reaction_row::COUNT), "1", TIMEOUT)
        .unwrap();
    assert!(gui.checked(&reaction(&posted.id, 0, "")).unwrap());
}

// ACT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_system_message_has_no_reaction_item_in_a_menu() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    admin.set_topic(&channel_id, "Lunch at noon");
    let topic = admin.last_message(&channel_id);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&topic.id, message_row::SYSTEM), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&message(&topic.id, message_row::MENU_BUTTON))
            .unwrap()
    );

    gui.mouse_click_with(
        &format!("{}/msg_{}", main_window::WINDOW, topic.id),
        MouseButton::Right,
        Modifiers::NONE,
    )
    .unwrap();

    // The menu would be there by now; it is not.
    gui.wait(Duration::from_millis(500)).unwrap();
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_REACT_ACTION)
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(&message(&topic.id, message_row::REACTION_ADD))
            .unwrap()
    );
}
