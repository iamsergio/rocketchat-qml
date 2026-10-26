# File upload

The user picks files to send (ACT-009). This is the core and `rc_view` part (`rc_api`, `rc_core`,
`rc_view`), the [Qt UI](#qt-side) and the [Slint UI](#slint-side).

## What the server does

The contract tests (`crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`) record it.
Checked on Rocket.Chat 8.0.

- **Two steps.** `rooms.upload` is gone (404). `POST /api/v1/rooms.media/<rid>` takes a multipart
  body with one part `file` (its filename is the name the file gets) and answers
  `{"file":{"_id","url"}}`: the server holds the file, but nothing is posted. `POST
  /api/v1/rooms.mediaConfirm/<rid>/<fileId>` with the JSON `{"msg","description","tmid"}`, all
  optional, posts it and answers `{"message": {...}}`: a normal message whose `attachments` are the
  ones `rc_api` reads already (an image has `image_url`, dimensions...).
- **Refusals.** No part `file`: 400 `[No file uploaded]`. A room the user isn't in:
  `{"success":false,"error":"unauthorized"}` (an `Error::Server`, not the 401 that expires the
  session). A blacklisted type (the test server blacklists `image/svg+xml`): 400 `File type is not
  accepted. [error-invalid-file-type]`. An unknown file id on the confirm: `[invalid-file]`.
- **The server picks the message's id.** `_id` and `tshow` are refused with a Match error, so the
  client can't choose the id (unlike [sending](sending.md)) and a thread upload is never also shown in
  the channel (`tmid` is the whole thread support).
- **A confirm may be repeated.** Confirming the same file again answers with the same message (same
  `_id`), so a confirm whose answer was lost can be tried again.
- **The type is the server's business.** It sniffs the file: a PNG sent as
  `application/octet-stream` comes back as `image/png` with its dimensions. So `rc_api` sends every
  file as `application/octet-stream`, and nothing guesses types.
- **Settings.** The public settings `FileUpload_Enabled` and `FileUpload_MaxFileSize` (bytes, -1 for
  no limit; 100 MiB on the test server) come with the message settings, in the same request
  (`MessageSettings::uploads_enabled`, `upload_max_bytes`).

## rc_api

`RestSession::upload_file(room, path, file_name, progress) -> file id` streams the file from disk
(a tokio file read in 64 KiB chunks, with the known length), never into memory, and calls `progress`
with the number of bytes handed to the connection as each chunk is pulled; the last call is the
file's length. It has its own timeout of an hour: the client's 30 seconds would end a big file on a
slow line. A file that can't be opened is `Error::File`. `confirm_upload(room, file id, text,
thread parent) -> Message`.

## The queue

The account's uploads form one list in the order they were asked for (`Actor::uploads`, in
`account/uploads.rs`). One runs at a time: the others are `Queued`, so the messages come in the order
the files were picked. A failed upload stays in the list, in its place, and doesn't block the others.

`AccountHandle::upload_file(room, path, text, thread)` is ignored unless the account is logged in,
the room is in the room list and, with `thread`, that thread is the open one (as for
`send_message`). The text is trimmed and goes as `msg` (may be empty). Before anything is queued the
core asks what needs no network, with a stat of the file and the server's settings (read at login,
so an upload asked for before they arrive is left to the server to refuse):

| Result | Upload |
|---|---|
| the server takes no uploads | `Failed(Disabled)` |
| the file is missing, can't be read or isn't a file | `Failed(Unreadable(why))`, size 0 |
| the file is bigger than the limit | `Failed(TooLarge { limit })` |
| else | `Queued`, then `Uploading` when its turn comes |

The states of an upload (`rc_core::uploads::UploadState`):

```
Queued -> Uploading { sent } -> Confirming -> (the upload leaves the list)
             |                      |
             +----> Failed(UploadError) <----+
```

`UploadError`: `TooLarge { limit }`, `Disabled`, `Unreadable(String)`, `Refused(String)` (the
server's words), `Connection` (the server can't be reached). `Unauthorized` is no failure of the
upload: it expires the session, as sending does.

## Events

- `Event::Uploads(Vec<Upload>)`: the whole list (it is short), whenever an upload is added, changes
  state or goes. An `Upload` has its `id` (`UploadId`, made by the core, in the order asked), `room`,
  `thread`, file `name`, `size` and `state`.
- `Event::UploadProgress { id, sent }`: the bytes handed to the connection, at most once per whole
  percent of the file per upload (the throttle is in the upload task: no clock). The list does not
  repeat it; the actor keeps the latest number, so the `Uploading { sent }` of a later list is
  current.

A logout and an expired session abort the request in flight, forget every upload, and tell it with an
empty `Event::Uploads` (if the list held any). Tasks are tagged with the login attempt, and an answer
of an older attempt is dropped.

## Cancel and retry

`cancel_upload(id)` removes a `Queued` or `Failed` upload, and aborts an `Uploading` one (the task is
aborted, which drops the request); the next queued upload starts. It is ignored while `Confirming`:
the message may be posted already, and the upload then ends by itself.

`retry_upload(id)` puts a `Failed` upload back in the queue, in its place in the list. If the server
has the file already (the failure was the confirm, say `Connection`), only the confirm is sent again,
which is safe because it is repeatable; else the checks above run again (the file may exist now) and
the file is uploaded again.

## How the message enters the timeline

The confirm's answer is applied like a message the server pushes (`add_live_message`): that is the
path the realtime echo takes too, so whichever comes first, the other finds the message held and
changes nothing, and nothing shows twice or goes missing. In particular a room whose newest page is
still loading holds the message for the page. `Timeline::settle` is not used: it settles an unsent
copy, and an upload has none (the server picks the id, so there is nothing to show before the
answer). The upload leaves the list in the same actor turn, after the timeline events, so the
frontend never shows the file twice or neither.

A thread upload is a reply that isn't shown in the room (`tmid` without `tshow`); it enters the
thread's timeline if that thread is still open, and else the thread shows it when it is opened.

## rc_view

`rc_view::upload_view` makes what a frontend shows, with no toolkit types:

- `UploadRow { id, name, size (in words), progress (0..1), status, error (in words), can_retry,
  can_cancel }`; `UploadStatus::{Queued, Uploading, Confirming, Failed}` has `name()` for roles. The
  error is in words: `TooLarge` names the limit ("The file is larger than the server's limit of 100
  MB"). A failed upload can be retried; all but one that is `Confirming` can be cancelled.
- `rows(uploads, Place::Room(room) | Place::Thread(parent))`: the rows of the uploads posted there. A
  room's are the ones without a thread.
- `changes(old, new) -> Vec<ListChange<UploadRow>>`: removals, updates and insertions by `UploadId`,
  the fine-grained changes of a model (`row_ops` maps them to Qt's calls). A progress is an update of
  one row.

`Cache` keeps the account's uploads from the events (`uploads()`, with the progress applied), and
gives `room_upload_rows()` (the open room) and `thread_upload_rows()` (the open thread). A frontend
calls `changes(rows_its_model_holds, cache.room_upload_rows())` after `Event::Uploads`,
`Event::UploadProgress` and whenever the open room or thread changes.

## What rc_view adds for the UIs

- `UploadRow` has the `status_line` (`1.8 of 4.2 MB`, `Waiting · 4.2 MB`, `Posting…`, or the error)
  and a `thumbnail` (`staged_view::Thumbnail`: the file URL for png, jpg, jpeg, gif and webp,
  else the extension in capitals, at most four letters). `rc_core::uploads::Upload` carries the
  file's `path` for it. A `Refused` error is shown without the code the server ends its words with
  in brackets.
- `rc_view::staged_view` is the files attached to a composer and not sent: `Staged` (add in order,
  without repeats, remove by index), `StagedFile` (name, size in words, thumbnail) and `plan(text,
  quoting, files)`, which says what Send does: the text goes with the first file, unless the
  composer quotes a message, when it is a message of its own (with its quote) before the files,
  which then have no text.
- `rc_view::startup::test_attachments()` reads `RC_TEST_ATTACH_FILES`, the paths a GUI test wants
  the attach button to take in place of the file dialog (Spix can't drive one). It is the same for
  every frontend, so the tests run unchanged against the Slint app, and only builds with the
  `test-attach-files` feature (Spix builds) read it.

## Qt side

Each composer (the room's, the thread pane's) owns two lists, as singletons because a list model
that QML creates as an element can't be linted (CXX-QT.md): `RoomStagedFiles` and
`ThreadStagedFiles` (`staged_files_model.rs`; add, remove, clear, `submit`), and `RoomUploads` and
`ThreadUploads` (`upload_list_model.rs`; cancel and retry by row). `Composer.qml` picks the pair
with `forThread` and only wires them.

- **Staging.** The paperclip `composerAttach` (hidden while editing and while previewing a
  channel) opens a `FileDialog` for several files, or stages `RC_TEST_ATTACH_FILES` when that is
  set. Files dropped on the room, or on the thread pane (then they go to the thread's composer), do
  the same: `DropZone.qml` covers the pane below its header with a `DropArea` and, while files are
  dragged over, an overlay that says where they go. The files show as chips in the bar
  `composerAttachments`. Staged files belong to a composer: **opening another room, or another
  thread, drops them** (the composer clears its list when `Room.roomId` or `Thread.parentId`
  changes), as it drops a quote. The uploads already asked for go on in the core.
- **Sending.** Send and Enter work with text or with staged files. With files the list's `submit`
  asks the hub, which sends the message of a quoting text, then calls `upload_file` for each file
  in order (`parent` set in the thread pane; "also send to the room" doesn't apply to files, which
  the server never shows in the room). The field and the chips clear.
- **Uploads.** The hub gives each of the two upload lists its rows from the cache on `Uploads`,
  `UploadProgress`, and whenever the open room or thread changes; the model computes
  `upload_view::changes` against what it holds and announces them row by row, so a progress
  repaints one row. `composerUploads` lists them over the field, below the chips.
- **Tests.** `crates/rc_gui_tests/tests/uploads.rs`: staging and taking files out; sending two
  files with text (the order, the text on the first); an SVG the server refuses (the reason in
  words, Retry, Remove); an upload from the thread pane. Dropping isn't tested: Spix can't drag.

## Slint side

The same behaviour, look and element names as the Qt side, from the same `rc_view` rows and the same `Theme` values (`file-chip-*`, `thumbnail-size`, `upload-*`, `drop-*`, in `theme.slint`); all four tests of `uploads.rs` run against `rocketchat_slint` unchanged.

- **Models.** `StagedModel` (`staged_model.rs`) is `rc_view::staged_view::Staged` as rows: adding or taking out a file announces that row alone, and a picture's thumbnail is read once, when the file is added. `UploadModel` (`upload_model.rs`) holds the `UploadRow`s and `show`s a new list with `upload_view::changes`, announced through `row_ops` as in the thread list, so a progress is `row_changed` of one row. `App` has one of each for the room and one for the thread, set on the window as `staged`, `uploads`, `thread-staged` and `thread-uploads`. The Slint structs are `StagedFileRow` and `UploadFileRow`; the status is its `name()` (`queued`, `uploading`, `confirming`, `failed`), which the view reads and decides nothing else from.
- **Views.** `Composer` has the paperclip (`composerAttach`, left of the field, not while editing or previewing), then `StagedBar` and `UploadsBar` over their hairlines and always made, with no height while they are empty (`visible` is what the tests see); the composer's height adds theirs. A chip is a fixed-size `Rectangle`, laid out by a `FlexboxLayout` so that they wrap. `Thumbnail`, `CrossButton` (the cross of a chip, and of an upload that can be cancelled; a failed one has the `FlatButton` "Remove") and `DropOverlay` are components of their own. Send is enabled with text or with files.
- **Commands.** `attach(for-thread)`, `remove-staged`, `cancel-upload` and `retry-upload` (by row) reach `App` through the page, which tells the room's composer from the thread's by a `bool`. `send` and `send-reply` send the files with the text when files wait (`App::send_with_files`, the same as `hub::send_with_files` of the Qt side: `plan`, a quoting text first, `upload_file` per file, the files cleared). The files wait per conversation: `show_room` and `show_thread` clear them when the room or the thread changes. `show_uploads` runs on `Event::Uploads`, `Event::UploadProgress`, `TimelineStatus`, `ThreadStatus` and `show_all`.
- **The file dialog.** The paperclip opens `rfd::AsyncFileDialog` (the desktop's file chooser portal on Linux) from `slint::spawn_local`; rfd runs the portal call on a thread of its own and wakes the event loop with the answer, so the GUI never blocks. A second click while it is open does nothing. It has no parent window. With `RC_TEST_ATTACH_FILES` set (builds with `test-attach-files`) the files are taken in place of the dialog, as in Qt.
- **Dropping.** `app/uploads.rs::accept_drops` hooks winit's window events through Slint's `unstable-winit-030` feature: `HoveredFile` shows `dropOverlay` ("Drop to attach to #room", `dropTitle`) over the room under its header, `HoveredFileCancelled` and `DroppedFile` hide it, and a dropped file goes to the room's composer. Limits: winit 0.30 reports no drag on Wayland at all (X11, macOS and Windows do), and no position for one, so the thread pane is no drop target (the Qt app has one); under the tests' backend there is no winit window, so the hook does nothing and the overlay is not tested. See SLINT.md.
- **Left out.** The tooltips of the paperclip and of the crosses (the Slint gaps item of PLAN.md), and the middle elision of a long file name (Slint elides at the end only).

## Tests

`crates/rc_core/src/client/tests/uploads.rs` (the queue and its order, progress and its throttle, the
echo coming first or later, a room still loading, threads, too large, disabled, unreadable, refused,
a confirm-only retry, cancel in each state, logout, `Unauthorized`), `rc_view::upload_view`,
`rc_view::staged_view`, `rc_view::startup` and `rc_view::cache`, the mock-server tests of
`crates/rc_api/tests/rest.rs`, and the contract tests of `rc_api` and `rc_core` against the real
server. The stale-attempt check has no test of its own: an aborted task cannot report, so an answer
of an older attempt cannot be made to arrive. The GUI tests are in `uploads.rs` of `rc_gui_tests`
(both apps), and `rc_slint` has unit tests of `StagedModel`, `UploadModel` and of the composer's width.
