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
- Models emit fine-grained row inserts, updates and removals. No `beginResetModel` for incremental changes.
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
- [ ] Start a DM, search the directory, join a channel
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
- [ ] Settings page (notifications, theme)
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
- [ ] Slint GUI gaps against the QML app: tooltips, the room menu and its error strip, the list's scrollbar, the topic rendered (it is shown as written), the login fields following an identity that arrives after a switch, a replaced avatar file refreshing (Slint's image cache keys on the path)
- [ ] Spix server in Rust for Slint (in the spix fork), `--spix-port`, on Slint's `ElementHandle` search API, element names from `accessible-id`. Headless in CI: Slint's testing backend in Spix builds
- [ ] ctest runs the GUI tests in `slint.txt` against `rocketchat_slint`. TRACEABILITY.md gets its Slint column, and a check that the list and the column agree
- [ ] Features, in the order of the GUI test files: auth, accounts, rooms, messages, realtime, sending, room menu, reactions, threads

## Phase 8: personal nice to haves
- [ ] Move logout icon. Logout is seldom used. right lcick on the server rail on a server tile can have the logout option.
- [ ] Channels are not showing description

## Later

- Packaging (Flatpak, AppImage, Windows installer).

## Decisions

- **One GUI test suite for both frontends.** It speaks the Spix XML-RPC protocol. The Qt app embeds C++ Spix and the Slint app embeds a Rust Spix server. See [docs/design/gui-testing.md](docs/design/gui-testing.md).
- **Qt ≥ 6.12.0.**
- **No fake server.** Integration and GUI tests run against a real Rocket.Chat (`scripts/run_test_server.sh`), which CI shares. Failures that a real server can't produce on demand are tested with port fakes (`rc_core`) and wiremock (`rc_api`).
- **File attachments open their server URL in the browser, without the token.** That works when the browser has a Rocket.Chat session. Downloading them through the app comes with file upload (Phase 5). Images are shown inline from the file cache.
- **Rocket.Chat server.** Target the version the company server runs (check it with `/api/info` in Phase 1). Support older versions only where it's cheap. No compatibility shims for old servers.

# Issues

- **rc_slint's room list relies on an undocumented Slint rule.** Slint makes only the rows in sight for a `for` inside a component named `ListView` (the compiler matches the name, `object_tree.rs`), and `ui/list_view.slint` is a style-free one so named, since the standard widgets' one brings a style. Check it on each Slint upgrade.
- **Slint's licence isn't chosen yet.** `deny.toml` has a temporary exception for Slint's crates, which allows both GPL-3.0 and the royalty-free licence. Decide between them (the royalty-free one requires attribution in the app) before any release.
- **rc_slint has no screen-reader support.** It is built without Slint's `accessibility` feature (AccessKit). Turn it on with the features that need it; the element names of the GUI tests (`accessible-id`) work without it.
- **Slint pulls in unmaintained crates.** On Linux its winit backend always depends on zbus (the xdg desktop settings portal), and Wayland's window decorations on ttf-parser. `deny.toml` ignores their four "unmaintained" advisories; drop the ignores when Slint or winit move off them.
- **Bulk deletions aren't followed.** An admin pruning a room sends `<room id>/deleteMessageBulk` instead of one `deleteMessage` per message. The messages stay on screen until the room is opened again with a catch-up (`chat.syncMessages` lists them as deleted), so subscribe to it next to `deleteMessage`.
- **MongoDB 8.x won't start on Linux ≥ 6.19** (tcmalloc bug, [SERVER-121912](https://jira.mongodb.org/browse/SERVER-121912)). The test server uses MongoDB 7.0, which Rocket.Chat 8 accepts with a deprecation warning. Rocket.Chat 9 drops 7.0, so revisit before moving the test server to RC 9.
- **Windows isn't built yet.** MSVC Debug builds need `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL` and `MAP_IMPORTED_CONFIG_DEBUG=RELEASE` on the Qt targets (cxx-qt-cmake warns about this), and vcpkg's debug triplet builds with `/MDd`, which doesn't link against the Rust staticlib. Add a build-only `windows-latest` CI job before adding more C++ dependencies.
