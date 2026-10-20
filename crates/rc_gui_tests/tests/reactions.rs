// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Reactions to messages, against the local test server (`RC_TEST_SERVER_URL`). The reactions are
//! made by users through the server's API and the tests wait for the chips to show up. Each test
//! makes its own users, channel and emoji and only looks at those.

mod common;

use rc_gui_tests::paths::{message, message_row, reaction, reaction_row};
use rc_test_server::{Admin, server_url, unique};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, open_room};

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
