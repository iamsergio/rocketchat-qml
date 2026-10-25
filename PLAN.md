# Plan

Rocket.Chat client: Rust core, QML GUI through cxx-qt. Design: [ARCHITECTURE.md](ARCHITECTURE.md). Feature IDs and their tests: [TRACEABILITY.md](TRACEABILITY.md).

At the end of every phase the app builds, runs against the local test server (`scripts/run_test_server.sh`) and passes all tests. Each phase ends with a demo for feedback. Development never touches the company server.

## Ground rules

- Rust edition 2024. The toolchain is pinned in `rust-toolchain.toml`. Lints are configured once in `[workspace.lints]`, with `clippy -D warnings` in CI.
- Library crates use `thiserror` error enums and never use `unwrap`/`expect`/`panic` outside tests (clippy `unwrap_used`/`expect_used` denied). `anyhow` is allowed only at the binary edge.
- `#![forbid(unsafe_code)]` everywhere except `rc_qt`.
- Domain IDs are newtypes (`RoomId`, `MessageId`, `UserId`). Secrets are wrapped in `secrecy` types. Wire types never leave `rc_api`.
- Use cases depend on traits (ports), never on concrete I/O, so every use case can be unit-tested with fakes.
- No logic in QML. Every interactive QML element gets an `objectName` from the GUI test contract.
- Models emit fine-grained row inserts, updates and removals. No `beginResetModel` for incremental changes. A model whose rows are a fresh result set, such as a search, resets: a new query is not a change of the old rows.
- Every feature has an ID in TRACEABILITY.md, and its tests carry a `// <ID>` comment.
- Every feature gets a unit test, a contract test against the local test server, or both. Every user-visible flow also gets a GUI test.
- Commit hygiene: `cargo fmt`, clippy, `cargo test` and the GUI tests all pass before each commit.


## Phase 5: Interacting

Done when: day-to-day use works. Edit, delete, react, threads, DMs, search and uploads.

- [x] Edit and delete your own messages
- [x] Add and remove reactions (emoji picker)
- [x] Favorites first in the room list, in their own section
- [x] Room menu: favorite, mark as read or unread, leave
- [x] Threads: open a thread panel, reply, see thread counts in the main timeline
- [x] Reply/quote a message
- [x] Typing indicators, received only: see [Typing indicators](#typing-indicators)
- [x] Search the room list by name (ROOM-010): a field below the room list, Ctrl+K focuses it, the matching rooms replace the list while it holds something. Only the user's own rooms, filtered locally: see [Search](docs/design/room-list.md#search)
- [x] A room's threads in the pane beside it (ACT-010): a header button opens the list, all or followed, the one with the most recent reply first, paged on scrolling; a row opens the thread, whose back button leads to the list: see [Threads](docs/design/threads.md#thread-list)
- [x] Unread threads (ACT-011): the header button's badge (grey, red for a mention), a row's emphasis, "New" and "@", and the "Unread" filter, from the subscription's `tunread`; opening a thread reads it (`readThreads`): see [Threads](docs/design/threads.md#unread-threads)
- [x] An author's name that fits is shown whole (MSG-006): the layout rounds the label's width down, so it takes the ceiling of its text width
- [ ] Start a DM, search the directory, join a channel. The directory search extends the field below the room list with the server's results
- [ ] File upload with progress
- [ ] Features ACT-001…ACT-009 tested

## Typing indicators

Done when: the open room shows who is typing in it, under its messages, and the open thread shows who is typing in that thread. The app never says that its own user is typing.

The server relays what the other clients emit on `stream-notify-room` as `<room id>/user-activity`, with the arguments `[username, activities, extras]`. Here `activities` holds `user-typing` while the user types, it is empty once they stop, and `extras.tmid` names the thread. The server refuses an emit under someone else's username. It accepts the legacy `<room id>/typing` but doesn't relay it. The web client emits again every 5 s while its user types and forgets a typist after 15 s without an event, so a stop that never comes still ends. All this was checked on the RC 8.0 test server.

- [x] `rc_api`: `subscribe_user_activity(room)` and `DdpEvent::UserActivity { room_id, username, typing, thread_id }`. Uploading or recording counts as not typing. Unit tests on frames taken from the server. `rc_test_server`: a `Typist`, a small synchronous DDP connection logged in as a test user, which starts and stops typing in a room or a thread the way the web client does (the `stream-notify-room` method). Contract test: a typist's start, thread and stop reach a connection that follows the room
- [x] `rc_core`: the realtime connection follows the open room's activity next to its deletions (`Pushed::Typing`). `Realtime::watch` stops being about deletions only. Contract test
- [x] `rc_core`: who is typing, kept with the open room: per typist, the room or the thread, and when they expire (15 s after their last event, through the `Clock`, which wakes the actor like the quiet-DM timer does). A typist ends on their stop, on a message of theirs in the same room or thread, or on expiry. Everyone ends on another room, a lost connection or a logout. Only the open room counts, and the user's own username is ignored, since their other clients are typing. `Event::Typing` and `Event::ThreadTyping` (the open thread's) carry the names in the order they started, only when they change. The name is the display name if a held message of the room has it, else the username. Unit tests with the fake clock
- [x] `rc_qt` / QML: `RoomVm.typing`, the line itself, written in Rust (`alice is typing…`, `alice and bob are typing…`, `alice, bob and 2 others are typing…`), with unit tests. A line between the messages and the composer, whose height is always reserved so the messages don't jump. Every size comes from Theme. Then the same for `ThreadVm` in the thread pane
- [x] Feature ACT-006 tested: GUI tests with a `Typist` in the open room, then in the open thread, that see the line come and go

## Phase 6: Persistence and desktop integration

Done when: the app starts instantly from cache (works offline read-only), shows desktop notifications, and has a settings page.

- [ ] `rc_store_sqlite`: `MessageStore` and room cache adapter, schema migrations
- [ ] Startup from cache, then a delta sync
- [ ] Bound the size of the file cache (avatars and attachment images so far): evict the least recently used files, never one the GUI shows
- [ ] Desktop notifications for DMs and mentions, unread count in the window title or tray
- [ ] Settings page (notifications, theme, the channel batch interval (ROOM-011, 25 min for now))
- [ ] Features PERS-001, PERS-002, DESK-001…DESK-003 tested

## Phase 7: Authentication extras

Done when: you can log in to servers that enforce 2FA or use SSO.

- [ ] 2FA (TOTP and email code)
- [ ] Personal access token login
- [ ] OAuth/SSO login through the browser
- [ ] Features AUTH-006…AUTH-008 tested

## Slint frontend

Done when: `rocketchat_slint`, a pure Rust app on `rc_core`, passes the GUI tests of every feature, run unchanged against it.

The Slint app grows one feature at a time, and CI stays green throughout: a second ctest entry runs the GUI tests against `rocketchat_slint`, but only those listed in `crates/rc_gui_tests/slint.txt`, which grows with each feature and never shrinks. TRACEABILITY.md gets a Slint column (`todo`, `partial`, `done`; `done` when all of a feature's GUI tests are in the list).

- [x] `rc_view`: the parts of `rc_qt` with no Qt types (the caches per account, the views, the row operations) move to a crate both frontends use. Rendering `RichText` stays with each frontend: `rc_qt` keeps the HTML for Qt's rich text. No behaviour change
- [x] `rc_slint`: the `rocketchat_slint` binary shows the version (APP-001, tested in-process with Slint's testing backend). A Slint `Theme` global (`ui/theme.slint`), under the same no-literals rule as `Theme.qml`. `deny.toml` has a temporary exception for Slint's licence. Logging setup is shared with `rc_qt` (`rc_core::logging::init_app`). It doesn't start the core yet
- [x] Split the hub of `rc_qt` from its Qt threading, so `rc_slint` reuses it: `rc_view::switchboard` decides what an event, or selecting, adding and removing an account, changes on screen (the account list's rows, the session view, a full snapshot, or the event for the current account's views), and `rc_qt`'s hub only carries that out on its view models. The secret store choice (`RC_SECRET_STORE`) and the fallback cache directory move to `rc_view` too. No behaviour change
- [x] Slint GUI, without Slint tests for now (they come with the Spix server): `rocketchat_slint` starts and stops the core, events reach the GUI thread through `slint::invoke_from_event_loop`. The server rail with its account tiles and the add button, the startup page, the login page (`--dev` fills it in), the sidebar with the server and user, the room list with its favorites and rooms sections, avatars or initials, unread and mention badges, and opening a room, which shows its header but no messages. Log out from the settings button. Element names (`accessible-id`) as in the GUI test contract. Same look as the QML app, from `Theme.qml`'s values
- [ ] Slint GUI gaps against the QML app: tooltips, the list's scrollbar, the login fields following an identity that arrives after a switch, a replaced avatar file refreshing (SLINT.md), custom emoji and inline images, which `StyledText` can't show (SLINT.md)
- [x] Spix server in Rust for Slint (`spix_slint`, in the spix fork), `--spix-port`, on Slint's `ElementHandle` search API, element names from `accessible-id`. Headless in CI: Slint's testing backend, in real time with its software renderer, in Spix builds, so that `takeScreenshot` (a PNG of an element) works and GUI tests can read pixels
- [x] ctest runs the GUI tests in `slint.txt` against `rocketchat_slint`. TRACEABILITY.md gets its Slint column, and a check that the list and the column agree
- [x] Slint sending (SEND-001, SEND-002): the composer under the open room's messages (Enter sends, Shift+Enter starts a line, the field grows with its text up to a height), the Send button, and the pending, failed and retry states in a message's header. `send` and `retry` go through `rc_view::room_view::outgoing` and the account's commands, as `rc_qt`'s room view model does. Without editing, quoting, the thread composer, the typing line and "also send to", which come with their own steps
- [x] Slint room menu (ROOM-006, ROOM-007): a right-click on a room row opens a popup with favorite, mark as read or unread (not for the open room) and leave (not for a direct message, from the row's `can-leave`); leaving asks first in a modal dialog over the window; a failed command shows its text in the error strip under the list until dismissed or the next command. The commands go through the account's `set_favorite`, `mark_read`, `mark_unread` and `leave_room`, as `rc_qt`'s hub does. Rooms moving between the favorites and the other section were handled by the room model already
- [x] Slint edited marker (RICH-006): a message the server says was edited shows "(edited)" after its time (`edited` on the row, from the domain message). Edits and deletes, live and in the catch-up on reopening a room, already reached the list through the model's `TimelineChange` handling
- [x] Slint typing line (ACT-006, the room's half): the line of who types in the open room sits between the messages and the composer and always takes its height, so the messages don't jump. It is `typing` of `rc_view::room_view::RoomState`, set on `Event::Typing` alone, without redoing the rest of the room header. The thread's line waits for the Slint thread pane
- [x] Slint room search (ROOM-010): the field at the bottom of the sidebar with its keycap and clear button, Ctrl+K, and the results in place of the list, as in the Qt app (see [Search](docs/design/room-list.md#search)). The matching is `rc_view::room_search::search_rooms` over the current account's cache, run on the GUI thread in `app.rs`; the results are their own small model (`RoomSearchModel`), replaced on each search, on `Event::Rooms`, `Event::QuietRooms` and an account switch while a search is active, with avatar updates reaching its rows. The results are a plain `for` in a bottom-aligned layout in a `Flickable`, not the lazy `ListView`: 500 matches take about 3 ms to make in a release build
- [x] Slint message menu, editing and deleting (ACT-001, ACT-002): each message the user can change has a "..." button (clear until the pointer is over the message) and a right-click menu with Edit and Delete, as in the Qt app (see [Editing and deleting your own messages](docs/design/editing-own-messages.md)). The row carries only a cheap `has-menu`, worked out when it is built and refreshed with the viewer; what the menu offers is asked from Rust (`menu-allowed`, `can_edit`/`can_delete` at `Timestamp::now()`) when it opens, since the policy has time limits. Delete asks in the generic `ConfirmDialog` (shared with leaving a room). The composer edits in place (the draft kept aside, an edit bar, "Save", Up in an empty field, Escape or another room to cancel), and `app.rs` keeps the action error and tells the composer how an edit ended. The menu's Quote, reaction and thread items come with those features. The room list's error was renamed `room-list-error`, so that the room action error means the message one, as in Qt.
- [x] Slint quoting (ACT-005): the message menu's Quote item (`msgQuoteAction`) for every message `can_quote` allows, so the "..." button and the right-click now open on other people's messages too (`has-menu` is `can_quote || can_edit || can_delete`; `MessageMenu` has `can-quote`). The composer shows a quote bar (author from `Cache::message_author`, the text as written from `message_text`, cut to its first line by clipping and not by Rust, a Cancel button) and `send` takes the quoted id for `outgoing`; sending, Escape, Cancel and another room drop the quote, quoting ends an edit and editing drops the quote, as in Qt. A message draws its quotes (`quote<index>` with `quoteAuthor`, `quoteTime`, `quoteBody`, `quoteBodyHtml`) from `rc_view::quote_view::quotes`, rendered through `rich_markdown::styled` like the body; `quotes` on the row is a `[QuoteRow]` model made only for a message that has quotes (the empty default `ModelRc` otherwise), and `msgBody` is left out when the text is empty and there are quotes. The thread composer waits for the Slint thread pane. ACT-001 and ACT-002 are done with the two menu tests.
- [x] Slint author name not elided when it fits (MSG-006): `msgAuthor` sets `accessible-description` to `truncated` while its `preferred-width` exceeds its width, and `spix_slint` answers the `truncated` property from it (Slint's testing API has no preferred size or elision, see SLINT.md), so the Qt test runs unchanged against the Slint app. The Slint header did not need a layout fix
- [ ] Features, in the order of the GUI test files: auth, accounts, rooms, messages, realtime, sending, room menu, reactions, threads

## Phase 8: personal nice to haves
- [ ] Move logout icon. Logout is seldom used. right lcick on the server rail on a server tile can have the logout option.
- [ ] Channels are not showing description
- [x] Batched channels (ROOM-011): the news of channels shows in batches at most every 25 minutes, see [Batched channels](docs/design/room-list.md#batched-channels)

## Later

- Packaging (Flatpak, AppImage, Windows installer).

## Decisions

- **One GUI test suite for both frontends.** It speaks the Spix XML-RPC protocol. The Qt app embeds C++ Spix and the Slint app embeds a Rust Spix server. See [docs/design/gui-testing.md](docs/design/gui-testing.md).
- **Qt ≥ 6.12.0.**
- **No fake server.** Integration and GUI tests run against a real Rocket.Chat (`scripts/run_test_server.sh`), which CI shares. Failures that a real server can't produce on demand are tested with port fakes (`rc_core`) and wiremock (`rc_api`).
- **File attachments open their server URL in the browser, without the token.** That works when the browser has a Rocket.Chat session. Downloading them through the app comes with file upload (Phase 5). Images are shown inline from the file cache.
- **Rocket.Chat server.** Target the version the company server runs (check it with `/api/info` in Phase 1). Support older versions only where it's cheap. No compatibility shims for old servers.

# Issues

- **Slint's licence isn't chosen yet.** `deny.toml` has a temporary exception for Slint's crates, which allows both GPL-3.0 and the royalty-free licence. Decide between them (the royalty-free one requires attribution in the app) before any release.
- **rc_slint has no screen-reader support.** It is built without Slint's `accessibility` feature (AccessKit). Turn it on with the features that need it; the element names of the GUI tests (`accessible-id`) work without it.
- **Slint pulls in unmaintained crates.** On Linux its winit backend always depends on zbus (the xdg desktop settings portal), and Wayland's window decorations on ttf-parser. `deny.toml` ignores their four "unmaintained" advisories; drop the ignores when Slint or winit move off them.
- **Bulk deletions aren't followed.** An admin pruning a room sends `<room id>/deleteMessageBulk` instead of one `deleteMessage` per message. The messages stay on screen until the room is opened again with a catch-up (`chat.syncMessages` lists them as deleted), so subscribe to it next to `deleteMessage`.
- **MongoDB 8.x won't start on Linux ≥ 6.19** (tcmalloc bug, [SERVER-121912](https://jira.mongodb.org/browse/SERVER-121912)). The test server uses MongoDB 7.0, which Rocket.Chat 8 accepts with a deprecation warning. Rocket.Chat 9 drops 7.0, so revisit before moving the test server to RC 9.
- **Windows isn't built yet.** MSVC Debug builds need `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL` and `MAP_IMPORTED_CONFIG_DEBUG=RELEASE` on the Qt targets (cxx-qt-cmake warns about this), and vcpkg's debug triplet builds with `/MDd`, which doesn't link against the Rust staticlib. Add a build-only `windows-latest` CI job before adding more C++ dependencies.
