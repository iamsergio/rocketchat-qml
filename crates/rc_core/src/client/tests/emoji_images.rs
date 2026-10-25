// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Image files of custom emoji, for a frontend that can't load a URL.

use super::*;

fn emoji(name: &str, aliases: &[&str], extension: &str) -> rc_api::CustomEmoji {
    rc_api::CustomEmoji {
        name: name.into(),
        aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
        extension: extension.into(),
    }
}

/// Where the fake cache keeps the image of the emoji `name` whose list entry says `gif`.
fn emoji_file(name: &str) -> PathBuf {
    PathBuf::from(format!("/cache/h/emoji/{name}.gif.png"))
}

fn emoji_update(name: &str) -> EmojiImageUpdate {
    EmojiImageUpdate {
        name: name.to_owned(),
        file: Some(emoji_file(name)),
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn png_file(bytes: &[u8]) -> Result<Image, ApiError> {
    Ok(Image {
        bytes: bytes.to_vec(),
        content_type: Some("image/png".to_owned()),
    })
}

/// Logs in to a server whose list holds `names` (and the alias `celebrate` of the first), with the
/// list received.
fn logged_in_with_emoji(fake: &Fake, cache: &FakeCache, names: &[&str]) -> Harness {
    let mut list: Vec<_> = names.iter().map(|name| emoji(name, &[], "gif")).collect();
    if let Some(first) = list.first_mut() {
        first.aliases.push("celebrate".to_owned());
    }
    fake.state().emoji = Some(Ok(list));
    let harness = logged_in_with_cache(fake, cache);
    harness.wait_for_custom_emoji();
    harness
}

fn want(harness: &Harness, names: &[&str]) {
    harness
        .account()
        .want_emoji_images(names.iter().map(|name| (*name).to_owned()).collect());
}

// RICH-007
#[test]
fn the_image_of_a_wanted_emoji_is_downloaded_and_sent_once_it_is_stored() {
    let fake = Fake::new();
    fake.script_emoji_image("party.gif", None, png_file(b"pixels"));
    let cache = FakeCache::default();
    let harness = logged_in_with_emoji(&fake, &cache, &["party"]);

    want(&harness, &["party"]);

    assert_eq!(harness.next_emoji_images(), [emoji_update("party")]);
    assert_eq!(
        cache.emoji_bytes("h", "party", "gif"),
        Some(b"pixels".to_vec())
    );
}

// RICH-007
#[test]
fn nothing_is_downloaded_unless_a_frontend_asks() {
    let fake = Fake::new();
    let harness = logged_in_with_emoji(&fake, &FakeCache::default(), &["party"]);

    harness.assert_no_emoji_images();

    assert!(fake.state().emoji_image_calls.is_empty());
}

// RICH-007
#[test]
fn a_cached_image_is_sent_at_once_and_refreshed_once() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_emoji_image("party.gif", Some(gate.clone()), png_file(b"new"));
    let cache = FakeCache::default();
    cache.seed_emoji("h", "party", "gif", "png", b"old");
    let harness = logged_in_with_emoji(&fake, &cache, &["party"]);

    want(&harness, &["party"]);

    // Before the download answers.
    assert_eq!(harness.next_emoji_images(), [emoji_update("party")]);
    fake.wait_for_emoji_image_calls(1);
    gate.notify_one();
    // The bytes differ: the same file is announced again.
    assert_eq!(harness.next_emoji_images(), [emoji_update("party")]);
    assert_eq!(
        cache.emoji_bytes("h", "party", "gif"),
        Some(b"new".to_vec())
    );
}

// RICH-007
#[test]
fn unchanged_bytes_send_no_second_update() {
    let fake = Fake::new();
    fake.script_emoji_image("party.gif", None, png_file(b"same"));
    let cache = FakeCache::default();
    cache.seed_emoji("h", "party", "gif", "png", b"same");
    let harness = logged_in_with_emoji(&fake, &cache, &["party"]);

    want(&harness, &["party"]);

    assert_eq!(harness.next_emoji_images(), [emoji_update("party")]);
    fake.wait_for_emoji_image_calls(1);
    harness.assert_no_emoji_images();
}

// RICH-007
#[test]
fn a_name_the_list_doesnt_hold_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_with_emoji(&fake, &FakeCache::default(), &["party"]);

    want(&harness, &["missing"]);

    harness.assert_no_emoji_images();
    assert!(fake.state().emoji_image_calls.is_empty());
}

// RICH-007
#[test]
fn an_alias_is_downloaded_as_the_emoji_it_names() {
    let fake = Fake::new();
    fake.script_emoji_image("party.gif", None, png_file(b"pixels"));
    let harness = logged_in_with_emoji(&fake, &FakeCache::default(), &["party"]);

    want(&harness, &["celebrate"]);

    assert_eq!(
        harness.next_emoji_images(),
        [EmojiImageUpdate {
            name: "celebrate".to_owned(),
            file: Some(emoji_file("party")),
        }]
    );
}

// RICH-007
#[test]
fn wanting_an_emoji_twice_downloads_it_once() {
    let fake = Fake::new();
    fake.script_emoji_image("party.gif", None, png_file(b"pixels"));
    let harness = logged_in_with_emoji(&fake, &FakeCache::default(), &["party"]);

    want(&harness, &["party", "party"]);
    assert_eq!(harness.next_emoji_images(), [emoji_update("party")]);
    want(&harness, &["party"]);
    harness.assert_no_emoji_images();

    assert_eq!(fake.state().emoji_image_calls, ["party.gif"]);
}

// RICH-007
#[test]
fn no_more_than_four_emoji_images_download_at_once() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    let names = ["a", "b", "c", "d", "e", "f"];
    for name in names {
        fake.script_emoji_image(
            &format!("{name}.gif"),
            Some(gate.clone()),
            png_file(name.as_bytes()),
        );
    }
    let harness = logged_in_with_emoji(&fake, &FakeCache::default(), &names);

    want(&harness, &names);

    fake.wait_for_emoji_image_calls(4);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.state().emoji_image_calls.len(), 4);
    assert_eq!(fake.state().emoji_image_max_running, 4);

    // One finishing lets the next start.
    gate.notify_one();
    fake.wait_for_emoji_image_calls(5);
    gate.notify_waiters();
    fake.wait_for_emoji_image_calls(6);
    assert!(fake.state().emoji_image_max_running <= 4);
}

// RICH-007
#[test]
fn logging_out_forgets_the_emoji_images() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed_emoji("h", "party", "gif", "png", b"old");
    let harness = logged_in_with_emoji(&fake, &cache, &["party"]);
    want(&harness, &["party"]);
    assert_eq!(harness.next_emoji_images(), [emoji_update("party")]);

    harness.logout();

    assert_eq!(
        harness.next_emoji_images(),
        [EmojiImageUpdate {
            name: "party".to_owned(),
            file: None,
        }]
    );
}
