// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Images of attachments.

use super::*;

/// The path an image of the tests is uploaded at.
fn image_path(name: &str) -> String {
    format!("/file-upload/id/{name}.png")
}

/// Where the fake cache keeps the image at `image_path(name)`.
fn image_file(name: &str) -> PathBuf {
    PathBuf::from(format!("/cache/h/attachment/{}.png", image_path(name)))
}

fn image_update(name: &str) -> ImageUpdate {
    ImageUpdate {
        path: image_path(name),
        file: Some(image_file(name)),
    }
}

/// A message of room `1` with an image attachment for each of `names`.
fn message_with_images(id: &str, secs: i64, names: &[&str]) -> Message {
    let mut message = message(id, secs);
    message.attachments = names
        .iter()
        .map(|name| Attachment {
            title: format!("{name}.png"),
            link: image_path(name),
            description: None,
            size: None,
            image: Some(ImageAttachment {
                path: image_path(name),
                width: Some(2),
                height: Some(1),
            }),
        })
        .collect();
    message
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn png_file(bytes: &[u8]) -> Result<Image, ApiError> {
    Ok(Image {
        bytes: bytes.to_vec(),
        content_type: Some("image/png".to_owned()),
    })
}

// RICH-005
#[test]
fn the_image_of_an_attachment_is_downloaded_and_sent_once_it_is_stored() {
    let fake = Fake::new();
    fake.script_file(&image_path("a"), None, png_file(b"pixels"));
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    fake.script_history(None, page(vec![message_with_images("m", 1, &["a"])], true));
    harness.account().open_room("1".to_owned().into());

    assert_eq!(harness.next_images(), [image_update("a")]);
    assert_eq!(
        cache.attachment_bytes("h", &image_path("a")),
        Some(b"pixels".to_vec())
    );
}

// RICH-005
#[test]
fn a_cached_image_is_sent_at_once_and_never_downloaded() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_attachment("h", &image_path("a"), "png", b"cached");
    let harness = logged_in_with_cache(&fake, &cache);
    fake.script_history(None, page(vec![message_with_images("m", 1, &["a"])], true));
    harness.account().open_room("1".to_owned().into());

    assert_eq!(harness.next_images(), [image_update("a")]);
    harness.assert_no_images();
    assert!(fake.state().file_calls.is_empty());
}

// RICH-005
#[test]
fn the_image_of_a_live_message_is_wanted() {
    let fake = Fake::new();
    fake.script_file(&image_path("a"), None, png_file(b"pixels"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a"])));

    assert_eq!(harness.next_images(), [image_update("a")]);
}

// RICH-005
#[test]
fn an_image_is_downloaded_once_however_often_it_is_wanted() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_file(&image_path("a"), Some(gate.clone()), png_file(b"pixels"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);
    fake.push(0, Pushed::Message(message_with_images("m1", 1, &["a"])));
    fake.wait_for_file_calls(1);
    // Held by the gate: other messages with the same image find the download in flight.
    fake.push(0, Pushed::Message(message_with_images("m2", 2, &["a"])));
    harness.wait_for_appended("m2");
    gate.notify_one();
    assert_eq!(harness.next_images(), [image_update("a")]);
    fake.push(0, Pushed::Message(message_with_images("m3", 3, &["a"])));
    harness.wait_for_appended("m3");

    assert_eq!(fake.state().file_calls, [image_path("a")]);
}

// RICH-005
#[test]
fn messages_without_an_image_want_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);
    let mut file = message_with_images("f", 1, &["a"]);
    file.attachments[0].image = None;

    fake.push(0, Pushed::Message(file));
    fake.push(0, Pushed::Message(message("t", 2)));
    harness.wait_for_appended("t");

    harness.assert_no_images();
    assert!(fake.state().file_calls.is_empty());
}

// RICH-005
#[test]
fn an_image_of_a_type_the_cache_doesnt_keep_is_not_shown() {
    let fake = Fake::new();
    fake.script_file(
        &image_path("a"),
        None,
        Ok(Image {
            bytes: b"<html>".to_vec(),
            content_type: Some("text/html".to_owned()),
        }),
    );
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a"])));
    fake.wait_for_file_calls(1);

    harness.assert_no_images();
    assert_eq!(cache.attachment_bytes("h", &image_path("a")), None);
}

// RICH-005
#[test]
fn an_image_the_server_refuses_is_not_asked_for_again() {
    let fake = Fake::new();
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m1", 1, &["a"])));
    fake.wait_for_file_calls(1);
    fake.push(0, Pushed::Message(message_with_images("m2", 2, &["a"])));
    harness.wait_for_appended("m2");

    harness.assert_no_images();
    assert_eq!(fake.state().file_calls.len(), 1);
}

// RICH-005
#[test]
fn an_image_that_couldnt_be_downloaded_is_tried_again_when_wanted_again() {
    let fake = Fake::new();
    fake.script_file(
        &image_path("a"),
        None,
        Err(ApiError::Unreachable("down".into())),
    );
    fake.script_file(&image_path("a"), None, png_file(b"pixels"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);
    fake.push(0, Pushed::Message(message_with_images("m1", 1, &["a"])));
    fake.wait_for_file_calls(1);
    harness.assert_no_images();

    fake.push(0, Pushed::Message(message_with_images("m2", 2, &["a"])));

    assert_eq!(harness.next_images(), [image_update("a")]);
    assert_eq!(fake.state().file_calls.len(), 2);
}

// RICH-005
#[test]
fn no_more_than_four_images_download_at_once() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    let names = ["1", "2", "3", "4", "5", "6"];
    for name in names {
        fake.script_file(
            &image_path(name),
            Some(gate.clone()),
            png_file(name.as_bytes()),
        );
    }
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &names)));
    fake.wait_for_file_calls(4);
    // Give a fifth the time to start, wrongly.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.state().file_calls.len(), 4);
    assert_eq!(fake.state().file_max_running, 4);

    gate.notify_waiters();
    fake.wait_for_file_calls(5);
    gate.notify_waiters();
    fake.wait_for_file_calls(6);
    gate.notify_waiters();
    assert!(fake.state().file_max_running <= 4);
    let mut shown = 0;
    while shown < 6 {
        shown += harness.next_images().len();
    }
}

// RICH-005
#[test]
fn logging_out_forgets_the_images_and_drops_what_was_still_downloading() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_attachment("h", &image_path("a"), "png", b"cached");
    let gate = Arc::new(Notify::new());
    fake.script_file(&image_path("b"), Some(gate.clone()), png_file(b"two"));
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);
    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a", "b"])));
    assert_eq!(harness.next_images(), [image_update("a")]);
    fake.wait_for_file_calls(1);

    harness.logout();
    gate.notify_waiters();

    assert_eq!(
        harness.next_images(),
        [ImageUpdate {
            path: image_path("a"),
            file: None
        }]
    );
    harness.assert_no_images();
    // The cache keeps the file for the next session.
    assert_eq!(
        cache.attachment_bytes("h", &image_path("a")),
        Some(b"cached".to_vec())
    );
}

// RICH-005
#[test]
fn the_image_cache_is_per_server() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_attachment("other:3000", &image_path("a"), "png", b"another server's");
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);

    fake.push(0, Pushed::Message(message_with_images("m", 1, &["a"])));
    fake.wait_for_file_calls(1);

    harness.assert_no_images();
}
