# File upload

The user picks files to send (ACT-009). This is the core and `rc_view` part (`rc_api`, `rc_core`,
`rc_view`); the Qt and the Slint UIs come after it.

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

## Tests

`crates/rc_core/src/client/tests/uploads.rs` (the queue and its order, progress and its throttle, the
echo coming first or later, a room still loading, threads, too large, disabled, unreadable, refused,
a confirm-only retry, cancel in each state, logout, `Unauthorized`), `rc_view::upload_view` and
`rc_view::cache`, the mock-server tests of `crates/rc_api/tests/rest.rs`, and the contract tests of
`rc_api` and `rc_core` against the real server. The stale-attempt check has no test of its own: an
aborted task cannot report, so an answer of an older attempt cannot be made to arrive. GUI tests
come with the UIs.
