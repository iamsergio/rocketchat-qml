// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Uploading files: the queue, the progress, and the message that ends up in the timeline.

use super::threads::reply;
use super::*;

thread_local! {
    /// The files `file` made in this test by name, for the `Upload`s the tests expect to carry
    /// their path.
    static FILES: std::cell::RefCell<HashMap<String, PathBuf>> = std::cell::RefCell::default();
}

/// A file of `size` bytes with this name.
fn file(name: &str, size: usize) -> PathBuf {
    let path = rc_test_server::temp_file(name, &vec![b'x'; size]);
    FILES.with(|files| files.borrow_mut().insert(name.to_owned(), path.clone()));
    path
}

fn id(number: u64) -> UploadId {
    UploadId(number)
}

/// An upload of room 1.
fn upload(number: u64, name: &str, size: u64, state: UploadState) -> Upload {
    Upload {
        id: id(number),
        room: "1".to_owned().into(),
        thread: None,
        name: name.to_owned(),
        path: FILES.with(|files| files.borrow().get(name).cloned().unwrap_or_default()),
        size,
        state,
    }
}

fn uploads_event(uploads: Vec<Upload>) -> Event {
    Event::Uploads(uploads)
}

fn no_uploads() -> Event {
    Event::Uploads(Vec::new())
}

/// The message the server makes of the file, as the room shows it.
fn posted(file: &str, text: &str) -> Message {
    uploaded_message("1", file, text, 1_000, None)
}

fn appended_message(message: Message) -> Event {
    Event::Timeline(vec![TimelineChange::Appended(vec![message])])
}

/// The server's settings for uploads, with editing off so that a policy event says when they have
/// arrived: the rules apply from the turn that tells it.
fn set_server_rules(fake: &Fake, enabled: bool, max_bytes: Option<u64>) -> Event {
    let settings = rc_api::MessageSettings {
        editing_allowed: false,
        uploads_enabled: enabled,
        upload_max_bytes: max_bytes,
        ..rc_api::MessageSettings::default()
    };
    fake.state().settings = Some(Ok(settings));
    Event::MessagePolicy(MessagePolicy::from(settings))
}

fn upload_to_room(harness: &Harness, path: &Path, text: &str) {
    harness
        .account()
        .upload_file("1".to_owned().into(), path.to_path_buf(), text, None);
}

// ACT-009
#[test]
fn a_file_goes_through_both_steps_and_its_message_enters_the_open_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    let confirm_gate = Arc::new(Notify::new());
    fake.script_upload(Some(Arc::clone(&gate)), vec![50], Ok("f1"));
    fake.script_confirm(Some(Arc::clone(&confirm_gate)), Ok(1_000));
    let path = file("notes.txt", 100);

    upload_to_room(&harness, &path, "  look  ");

    let uploading = |sent| upload(0, "notes.txt", 100, UploadState::Uploading { sent });
    harness.expect_events(vec![
        uploads_event(vec![uploading(0)]),
        Event::UploadProgress {
            id: id(0),
            sent: 50,
        },
    ]);
    assert_eq!(
        fake.wait_for_upload_calls(1),
        [UploadCall {
            room: "1".into(),
            path,
            name: "notes.txt".into(),
        }]
    );
    gate.notify_one();
    harness.expect_events(vec![uploads_event(vec![upload(
        0,
        "notes.txt",
        100,
        UploadState::Confirming,
    )])]);
    assert_eq!(
        fake.wait_for_confirm_calls(1),
        [ConfirmCall {
            room: "1".into(),
            file: "f1".into(),
            text: "look".into(),
            thread: None,
        }]
    );
    confirm_gate.notify_one();
    // The message and the end of the upload come in one turn.
    harness.expect_events(vec![appended_message(posted("f1", "look")), no_uploads()]);
}

// ACT-009
#[test]
fn uploads_run_one_at_a_time_in_the_order_they_were_asked_for() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_upload(Some(Arc::clone(&gate)), vec![], Ok("f1"));
    let (a, b, c) = (file("a.txt", 1), file("b.txt", 2), file("c.txt", 3));

    upload_to_room(&harness, &a, "");
    upload_to_room(&harness, &b, "");
    upload_to_room(&harness, &c, "");

    let running = upload(0, "a.txt", 1, UploadState::Uploading { sent: 0 });
    let queued_b = upload(1, "b.txt", 2, UploadState::Queued);
    let queued_c = upload(2, "c.txt", 3, UploadState::Queued);
    harness.expect_events(vec![
        uploads_event(vec![running]),
        uploads_event(vec![
            upload(0, "a.txt", 1, UploadState::Uploading { sent: 0 }),
            queued_b,
        ]),
        uploads_event(vec![
            upload(0, "a.txt", 1, UploadState::Uploading { sent: 0 }),
            upload(1, "b.txt", 2, UploadState::Queued),
            queued_c,
        ]),
    ]);
    fake.wait_for_upload_calls(1);
    assert_eq!(fake.state().upload_calls.len(), 1);

    gate.notify_one();

    let events = harness.events_until(|event| *event == no_uploads());
    let appended: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Timeline(changes) => match changes.as_slice() {
                [TimelineChange::Appended(messages)] => Some(messages[0].id.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(appended, ["msg-f1", "msg-file2", "msg-file3"]);
    let names: Vec<_> = (fake.state().upload_calls.iter())
        .map(|call| call.name.clone())
        .collect();
    assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
    let files: Vec<_> = (fake.state().confirm_calls.iter())
        .map(|call| call.file.clone())
        .collect();
    assert_eq!(files, ["f1", "file2", "file3"]);
}

// ACT-009
#[test]
fn the_progress_comes_at_most_once_per_percent() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_upload(
        Some(Arc::clone(&gate)),
        vec![5, 6, 10, 11, 19, 100, 101, 1_000],
        Ok("f1"),
    );

    upload_to_room(&harness, &file("big.bin", 1_000), "");

    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "big.bin",
            1_000,
            UploadState::Uploading { sent: 0 },
        )]),
        Event::UploadProgress {
            id: id(0),
            sent: 10,
        },
        Event::UploadProgress {
            id: id(0),
            sent: 100,
        },
        Event::UploadProgress {
            id: id(0),
            sent: 1_000,
        },
    ]);
    gate.notify_one();
    // The list that follows has the progress made so far.
    harness.expect_events(vec![uploads_event(vec![upload(
        0,
        "big.bin",
        1_000,
        UploadState::Confirming,
    )])]);
}

// ACT-009
#[test]
fn a_progress_of_an_upload_that_was_cancelled_is_not_told() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_upload(Some(Arc::clone(&gate)), vec![], Ok("f1"));
    upload_to_room(&harness, &file("a.txt", 10), "");
    harness.next_event();
    fake.wait_for_upload_calls(1);

    harness.account().cancel_upload(id(0));
    harness.expect_events(vec![no_uploads()]);
    gate.notify_one();

    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.state().uploads_returned, 0);
    harness.account().cancel_upload(id(0));
    drop(harness);
}

// ACT-009
#[test]
fn the_echo_that_comes_first_finds_the_message_there_and_nothing_shows_twice() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let confirm_gate = Arc::new(Notify::new());
    fake.script_confirm(Some(Arc::clone(&confirm_gate)), Ok(1_000));
    upload_to_room(&harness, &file("a.txt", 10), "");
    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.txt",
            10,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(0, "a.txt", 10, UploadState::Confirming)]),
    ]);
    fake.wait_for_confirm_calls(1);

    assert!(fake.push(0, Pushed::Message(posted("file1", ""))));
    harness.expect_events(vec![appended_message(posted("file1", ""))]);
    confirm_gate.notify_one();
    fake.wait_for_confirms_returned(1);

    // Only the upload goes: the message is held already.
    harness.expect_events(vec![no_uploads()]);
    assert!(fake.push(0, pushed_message("1", "c", 2_000)));
    harness.expect_events(vec![appended("c", 2_000)]);
}

// ACT-009
#[test]
fn the_echo_that_comes_later_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    upload_to_room(&harness, &file("a.txt", 10), "");
    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.txt",
            10,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(0, "a.txt", 10, UploadState::Confirming)]),
        appended_message(posted("file1", "")),
        no_uploads(),
    ]);

    assert!(fake.push(0, Pushed::Message(posted("file1", ""))));
    assert!(fake.push(0, pushed_message("1", "c", 2_000)));

    harness.expect_events(vec![appended("c", 2_000)]);
}

// ACT-009
#[test]
fn a_message_confirmed_while_the_room_loads_is_not_lost_to_the_page() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let history_gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&history_gate)),
        page(vec![message("b", 2), message("a", 1)], false),
    );
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);

    upload_to_room(&harness, &file("a.txt", 10), "");
    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.txt",
            10,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(0, "a.txt", 10, UploadState::Confirming)]),
        // The upload is done, and the message waits for the page.
        no_uploads(),
    ]);
    history_gate.notify_one();

    harness.expect_events(vec![
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), posted("file1", "")],
        ),
        status("1", false, false, false, None),
    ]);
}

// ACT-009
#[test]
fn an_upload_in_the_open_thread_is_a_reply_that_the_room_never_hears_of() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    super::threads::open_thread_of_b(&fake, &harness);

    harness.account().upload_file(
        "1".to_owned().into(),
        file("a.txt", 10),
        "",
        Some("b".to_owned().into()),
    );

    let thread_upload = |state| Upload {
        thread: Some("b".to_owned().into()),
        ..upload(0, "a.txt", 10, state)
    };
    harness.expect_events(vec![
        uploads_event(vec![thread_upload(UploadState::Uploading { sent: 0 })]),
        uploads_event(vec![thread_upload(UploadState::Confirming)]),
        Event::ThreadTimeline(vec![TimelineChange::Appended(vec![uploaded_message(
            "1",
            "file1",
            "",
            1_000,
            Some(&"b".to_owned().into()),
        )])]),
        no_uploads(),
    ]);
    assert_eq!(
        fake.wait_for_confirm_calls(1)[0].thread,
        Some("b".to_owned())
    );
    flush_pushed(&fake, &harness, "1");
}

// ACT-009
#[test]
fn only_the_open_thread_can_be_uploaded_into() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness.account().upload_file(
        "1".to_owned().into(),
        file("a.txt", 10),
        "",
        Some(reply("r1", 3, "b").id),
    );
    drop(harness);

    assert!(fake.state().upload_calls.is_empty());
}

// ACT-009
#[test]
fn logged_out_clients_and_unknown_rooms_upload_nothing() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    // Logged out.
    upload_to_room(&harness, &file("a.txt", 10), "");
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    // Not in the room list.
    upload_to_room(&harness, &file("a.txt", 10), "");
    drop(harness);

    assert!(fake.state().upload_calls.is_empty());
}

// ACT-009
#[test]
fn a_file_that_is_too_large_fails_at_once_without_the_network_and_does_not_block_the_queue() {
    let fake = Fake::new();
    let policy = set_server_rules(&fake, true, Some(5));
    let harness = logged_in_harness(&fake);
    harness.expect_events(vec![policy]);
    let (big, small) = (file("big.bin", 6), file("small.bin", 5));

    upload_to_room(&harness, &big, "");
    upload_to_room(&harness, &small, "");

    let too_large = upload(
        0,
        "big.bin",
        6,
        UploadState::Failed(UploadError::TooLarge { limit: 5 }),
    );
    harness.expect_events(vec![
        uploads_event(vec![too_large.clone()]),
        uploads_event(vec![
            too_large.clone(),
            upload(1, "small.bin", 5, UploadState::Uploading { sent: 0 }),
        ]),
    ]);
    assert_eq!(fake.wait_for_upload_calls(1)[0].name, "small.bin");
    let events = harness.events_until(
        |event| matches!(event, Event::Uploads(list) if list.len() == 1 && list[0] == too_large),
    );
    assert!(!events.is_empty());
    assert_eq!(fake.state().upload_calls.len(), 1);
}

// ACT-009
#[test]
fn a_server_that_takes_no_uploads_fails_them_at_once() {
    let fake = Fake::new();
    let policy = set_server_rules(&fake, false, None);
    let harness = logged_in_harness(&fake);
    harness.expect_events(vec![policy]);

    upload_to_room(&harness, &file("a.txt", 10), "");

    harness.expect_events(vec![uploads_event(vec![upload(
        0,
        "a.txt",
        10,
        UploadState::Failed(UploadError::Disabled),
    )])]);
    drop(harness);
    assert!(fake.state().upload_calls.is_empty());
}

// ACT-009
#[test]
fn a_file_that_cannot_be_read_fails_at_once_and_a_retry_looks_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let dir = file("anchor", 1);
    let path = dir.with_file_name("late.txt");
    FILES.with(|files| {
        files
            .borrow_mut()
            .insert("late.txt".to_owned(), path.clone())
    });

    upload_to_room(&harness, &path, "");

    let Event::Uploads(list) = harness.next_event() else {
        panic!("not the uploads");
    };
    let [failed] = list.as_slice() else {
        panic!("not one upload: {list:?}");
    };
    assert_eq!((failed.name.as_str(), failed.size), ("late.txt", 0));
    assert!(
        matches!(
            failed.state,
            UploadState::Failed(UploadError::Unreadable(_))
        ),
        "{failed:?}"
    );
    assert!(fake.state().upload_calls.is_empty());

    // Still not there: still failed, and told again.
    harness.account().retry_upload(id(0));
    assert!(matches!(harness.next_event(), Event::Uploads(_)));
    std::fs::write(&path, b"abc").unwrap();
    harness.account().retry_upload(id(0));
    harness.expect_events(vec![uploads_event(vec![upload(
        0,
        "late.txt",
        3,
        UploadState::Uploading { sent: 0 },
    )])]);
    assert_eq!(fake.wait_for_upload_calls(1)[0].name, "late.txt");
}

// ACT-009
#[test]
fn a_refusal_of_the_server_fails_the_upload_with_its_words_and_a_retry_uploads_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_upload(
        None,
        vec![],
        Err(ApiError::Server {
            status: 400,
            message: "File type is not accepted".into(),
        }),
    );
    upload_to_room(&harness, &file("a.svg", 10), "");

    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.svg",
            10,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(
            0,
            "a.svg",
            10,
            UploadState::Failed(UploadError::Refused("File type is not accepted".into())),
        )]),
    ]);
    harness.account().retry_upload(id(0));
    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.svg",
            10,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(0, "a.svg", 10, UploadState::Confirming)]),
    ]);
    assert_eq!(fake.wait_for_upload_calls(2).len(), 2);
}

// ACT-009
#[test]
fn a_failed_confirmation_is_retried_without_uploading_the_file_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_confirm(None, Err(ApiError::Unreachable("down".into())));
    upload_to_room(&harness, &file("a.txt", 10), "hi");
    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.txt",
            10,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(0, "a.txt", 10, UploadState::Confirming)]),
        uploads_event(vec![upload(
            0,
            "a.txt",
            10,
            UploadState::Failed(UploadError::Connection),
        )]),
    ]);

    harness.account().retry_upload(id(0));

    harness.expect_events(vec![
        uploads_event(vec![upload(0, "a.txt", 10, UploadState::Confirming)]),
        appended_message(posted("file1", "hi")),
        no_uploads(),
    ]);
    assert_eq!(fake.state().upload_calls.len(), 1);
    let confirms = fake.state().confirm_calls.clone();
    assert_eq!(confirms.len(), 2);
    assert_eq!(confirms[0], confirms[1]);
}

// ACT-009
#[test]
fn retrying_an_upload_that_did_not_fail_does_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_upload(Some(gate), vec![], Ok("f1"));
    upload_to_room(&harness, &file("a.txt", 10), "");
    upload_to_room(&harness, &file("b.txt", 10), "");
    harness.next_event();
    harness.next_event();

    harness.account().retry_upload(id(0));
    harness.account().retry_upload(id(1));
    harness.account().retry_upload(id(7));

    std::thread::sleep(Duration::from_millis(50));
    assert!(harness.recv_timeout(Duration::from_millis(50)).is_err());
}

// ACT-009
#[test]
fn a_failed_upload_keeps_its_place_when_it_is_retried() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_upload(None, vec![], Err(ApiError::Unreachable("down".into())));
    fake.script_upload(Some(Arc::clone(&gate)), vec![], Ok("f2"));
    let (a, b) = (file("a.txt", 1), file("b.txt", 1));
    upload_to_room(&harness, &a, "");
    // a fails, b runs (held at its gate).
    harness.events_until(|event| {
        matches!(event, Event::Uploads(list)
        if list.len() == 1 && matches!(list[0].state, UploadState::Failed(_)))
    });
    upload_to_room(&harness, &b, "");
    harness.next_event();
    fake.wait_for_upload_calls(2);

    harness.account().retry_upload(id(0));

    // It waits for b, and is first in the list.
    harness.expect_events(vec![uploads_event(vec![
        upload(0, "a.txt", 1, UploadState::Queued),
        upload(1, "b.txt", 1, UploadState::Uploading { sent: 0 }),
    ])]);
    gate.notify_one();
    let calls = fake.wait_for_upload_calls(2);
    assert_eq!(calls[1].name, "b.txt");
}

// ACT-009
#[test]
fn a_queued_a_failed_and_an_uploading_upload_can_be_cancelled() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_upload(None, vec![], Err(ApiError::Unreachable("down".into())));
    fake.script_upload(Some(Arc::clone(&gate)), vec![], Ok("f2"));
    for name in ["a.txt", "b.txt", "c.txt"] {
        upload_to_room(&harness, &file(name, 1), "");
    }
    // a failed, b runs at its gate, c waits.
    harness.events_until(|event| {
        matches!(event, Event::Uploads(list)
        if list.len() == 3 && list[1].state == UploadState::Uploading { sent: 0 }
            && matches!(list[0].state, UploadState::Failed(_)))
    });
    fake.wait_for_upload_calls(2);

    // The queued one.
    harness.account().cancel_upload(id(2));
    harness.expect_events(vec![uploads_event(vec![
        upload(0, "a.txt", 1, UploadState::Failed(UploadError::Connection)),
        upload(1, "b.txt", 1, UploadState::Uploading { sent: 0 }),
    ])]);
    // The failed one.
    harness.account().cancel_upload(id(0));
    harness.expect_events(vec![uploads_event(vec![upload(
        1,
        "b.txt",
        1,
        UploadState::Uploading { sent: 0 },
    )])]);
    // The one under way is stopped, and the request is dropped.
    harness.account().cancel_upload(id(1));
    harness.expect_events(vec![no_uploads()]);
    gate.notify_one();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.state().uploads_returned, 1);
    assert!(fake.state().confirm_calls.is_empty());
}

// ACT-009
#[test]
fn cancelling_an_upload_starts_the_next_one() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_upload(Some(gate), vec![], Ok("f1"));
    upload_to_room(&harness, &file("a.txt", 1), "");
    upload_to_room(&harness, &file("b.txt", 1), "");
    harness.next_event();
    harness.next_event();

    harness.account().cancel_upload(id(0));

    harness.expect_events(vec![uploads_event(vec![upload(
        1,
        "b.txt",
        1,
        UploadState::Uploading { sent: 0 },
    )])]);
    assert_eq!(fake.wait_for_upload_calls(2)[1].name, "b.txt");
}

// ACT-009
#[test]
fn an_upload_being_confirmed_cannot_be_cancelled() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_confirm(Some(Arc::clone(&gate)), Ok(1_000));
    upload_to_room(&harness, &file("a.txt", 1), "");
    harness.expect_events(vec![
        uploads_event(vec![upload(
            0,
            "a.txt",
            1,
            UploadState::Uploading { sent: 0 },
        )]),
        uploads_event(vec![upload(0, "a.txt", 1, UploadState::Confirming)]),
    ]);
    fake.wait_for_confirm_calls(1);

    harness.account().cancel_upload(id(0));
    gate.notify_one();

    harness.expect_events(vec![appended_message(posted("file1", "")), no_uploads()]);
}

// ACT-009
#[test]
fn logging_out_stops_and_forgets_the_uploads() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_upload(Some(Arc::clone(&gate)), vec![], Ok("f1"));
    upload_to_room(&harness, &file("a.txt", 1), "");
    upload_to_room(&harness, &file("b.txt", 1), "");
    harness.next_event();
    harness.next_event();
    fake.wait_for_upload_calls(1);

    harness.logout();

    let events = harness.events_until(|event| *event == no_uploads());
    assert!(events.contains(&session_event(logged_out(None))));
    gate.notify_one();
    std::thread::sleep(Duration::from_millis(50));
    // Nothing of the stopped request arrives, and the next upload never started.
    assert_eq!(fake.state().uploads_returned, 0);
    assert_eq!(fake.state().upload_calls.len(), 1);
    assert!(fake.state().confirm_calls.is_empty());
    // Logged out already, and with none: no event at all.
    harness.logout();
}

// ACT-009
#[test]
fn a_refused_token_on_an_upload_expires_the_session_and_forgets_the_uploads() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_upload(None, vec![], Err(ApiError::Unauthorized));
    upload_to_room(&harness, &file("a.txt", 1), "");
    upload_to_room(&harness, &file("b.txt", 1), "");

    let events = harness.events_until(|event| *event == no_uploads());

    assert!(events.contains(&session_event(logged_out(Some(
        LoginFailure::SessionExpired
    )))));
    assert_eq!(fake.state().upload_calls.len(), 1);
}

// ACT-009
#[test]
fn a_refused_token_on_a_confirmation_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_confirm(None, Err(ApiError::Unauthorized));
    upload_to_room(&harness, &file("a.txt", 1), "");

    let events = harness.events_until(|event| *event == no_uploads());

    assert!(events.contains(&session_event(logged_out(Some(
        LoginFailure::SessionExpired
    )))));
}
