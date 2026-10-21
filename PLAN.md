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

## Phase 0: Skeleton

Done when: the app opens a window showing the version string, which comes from Rust. One GUI smoke test passes locally and in CI.

- [x] Cargo workspace at the repo root (`crates/`), `rust-toolchain.toml`, workspace lints, `rustfmt.toml`
- [x] Empty crates: `rc_api`, `rc_core`, `rc_qt`, `rc_gui_tests`
- [x] CMake: Corrosion + cxx-qt, `find_package(Qt6 6.12 ...)`. `rc_qt` builds as a static lib and is linked into the existing `src/gui` executable
- [x] `AppInfo` QObject (version) exposed to QML and shown in `Main.qml`
- [x] vcpkg as a submodule in `3rdparty/vcpkg` (the iamsergio fork, same commit Spix pins). Root `vcpkg.json` in manifest mode, with AnyRPC under a `spix` feature so builds without Spix pull nothing. The presets set the vcpkg toolchain
- [x] Spix: CMake option `ROCKETCHATQML_SPIX` (ON in `dev`), declared before `project()` so it can add `spix` to `VCPKG_MANIFEST_FEATURES`. `add_subdirectory(3rdparty/spix)` with only the QtQuick scene, no examples or tests. The host starts the test server on the port given by `--spix-port`
- [x] `rc_gui_tests`: harness that launches the app binary (`QT_QPA_PLATFORM=offscreen`) on a free port, waits for Spix, and kills the app on drop. The binary is configurable, so the same tests can target the Slint app later. Smoke test APP-001
- [x] `rc_gui_tests`: `Gui` client wrapper that exposes only the Spix calls both servers support, and the path constants module
- [x] `tracing` setup: stderr, plus a log file in dev builds
- [x] `ctest` runs `cargo test` for the workspace and the GUI tests
- [x] CI: fmt, clippy, `cargo test`, `cargo deny` (licenses/advisories), `reuse lint`, CMake build, GUI tests
- [x] `.mcp.json` entry for QtCommander (dev only)
- [x] `scripts/run_test_server.sh`: local Rocket.Chat (docker by default, podman with `RC_ENGINE=podman`) with MongoDB and NATS, admin preset, wizard and rate limits off
- [x] Update README (build requirements: Rust, Qt ≥ 6.12, `git submodule update --init`)

## Phase 1: Log in, list rooms

Done when: you can log in to the local test server with a username and password, see your rooms, restart without logging in again, and log out.

- [x] `rc_api`: REST client with auth headers, `GET /api/info` (server version check), `login`, `me`, `logout`, `subscriptions.get`
- [x] `rc_core`: `Client` handle plus session actor. Session state machine: `LoggedOut → Connecting → LoggedIn` (`Resuming` instead of `Connecting` for a stored session), with failure reasons
- [x] `rc_core`: `SecretStore` port with keyring and in-memory adapters. Resume the session from the stored token at startup
- [x] `rc_core`: room domain model (channel/group/DM, name, unread, mention count)
- [x] CI runs the `rc_api` and `rc_core` contract tests against the local test server (the runner joins its network)
- [x] `rc_qt`: `SessionVm` (state, error, login/logout invokables), `RoomListModel`
- [x] QML: login page (server URL, user, password, error text, busy state), room list, logout
- [x] Features AUTH-001…AUTH-005, ROOM-001…ROOM-003 tested

## Phase 2: Read messages

Done when: clicking a room shows its history with authors and times, and scrolling up loads older messages.

- [x] `rc_api`: room history (`channels/groups/im.history`, depending on room type). Covered by contract tests that post their own messages
- [x] `rc_core`: message domain model (plain text body for now), `MessageStore` port with an in-memory adapter, pagination cursor per room
- [x] `rc_core`: system messages (join, leave, topic changed) as their own kind
- [x] `rc_qt`: `MessageListModel` (prepend for older messages, append for newer), `RoomVm` (current room)
- [x] QML: room view, message delegate, day separators, load older on reaching the top
- [x] Features MSG-001…MSG-005 tested

## Phase 3: Realtime and sending

Done when: messages from others appear live, unread badges update, you can send messages, and the client recovers after the network drops.

- [x] `rc_api`: DDP client (connect, ping/pong, method calls, subscriptions, resume-token login)
- [x] `rc_core`: sync engine. Subscribe to `__my_messages__` and `stream-notify-user` (subscriptions/rooms changed), route events into state
- [x] `rc_core`: reconnect with exponential backoff and jitter. Catch up on missed messages after reconnecting
- [x] `rc_core`: send with an optimistic local echo (pending → sent / failed), retry
- [x] `rc_core`: mark a room read when it's opened. Room list ordered by last activity
- [x] Tests: a small TCP proxy in front of the test server to drop connections on demand
- [x] `rc_qt` / QML: composer, pending/failed indicators with retry
- [x] `rc_qt` / QML: connection status banner
- [x] Contract tests for DDP and realtime. The REST contract tests already exist and run in CI (since Phase 1) on the shared server
- [x] Contract tests must pass on a dirty server, in any order and in parallel:
  - each test creates its own users and rooms with unique names (per-run random suffix) through the admin API
  - tests never change server settings. A setting tests need goes into the script's `OVERWRITE_SETTING_*` list, so it's part of the server definition
  - tests assert only on objects they created (no "the room list has exactly N rooms")
- [x] Features RT-001…RT-004, SEND-001, SEND-002, ROOM-004, ROOM-005 tested

## Phase 4: Rich content

Done when: messages render formatting, mentions, emoji, avatars, images and reactions, and live edits/deletes show up.

- [x] `rc_core`: convert the server's parsed-markdown (`md`) AST into a GUI-agnostic rich-text block model. Each frontend renders that model. Fall back to `msg` text when there is no `md`
- [x] `rc_qt`: render blocks to the subset of rich text that Qt supports (links, code, quotes, lists)
- [x] Mentions (highlight @me), unicode emoji shortcodes, custom server emoji. Custom emoji images are public (`/emoji-custom/<name>.<ext>`), so the message shows them straight from the server; the file-cache item can take them over later. The emoji list is fetched per sync cycle, not kept live by `updateEmojiCustom`
- [x] `rc_core`: file cache port. Avatars are downloaded to the cache directory and exposed to the GUI as local file URLs. They are kept between sessions and refreshed once per session; the server pushes changes. Only avatars somebody uploaded are shown (`avatarETag`); the others show initials. The test server serves the drawn avatar whatever is uploaded, so the picture of a custom avatar isn't tested
- [x] `rc_core`: images through the file cache port, for the attachments below. An uploaded file never changes, so an image is downloaded only if the cache lacks it
- [x] Image attachments shown inline, other attachments as links
- [x] Realtime message edits and deletes, plus a reactions display. The connection follows the open room's deletions (`stream-notify-room`), and `chat.syncMessages` brings a reopened or reconnected room up to date. Bulk deletions (`deleteMessageBulk`, an admin pruning a room) are not followed yet: see Issues
- [x] Features RICH-001…RICH-007 tested

## Phase 5: Interacting

Done when: day-to-day use works. Edit, delete, react, threads, DMs, search and uploads.

- [x] Edit and delete your own messages
- [x] Add and remove reactions (emoji picker)
- [x] Favorites first in the room list, in their own section
- [x] Room menu: favorite, mark as read or unread, leave
- [x] Threads: open a thread panel, reply, see thread counts in the main timeline
- [x] Reply/quote a message
- [ ] Typing indicators (send and receive)
- [ ] Start a DM, search the directory, join a channel
- [ ] File upload with progress
- [ ] Features ACT-001…ACT-009 tested

## Multiple accounts

Done when: you can be signed in to several servers at once, switch between them from a rail of servers, and see the unread counts of the ones in the background. Restarting resumes every account.

One session actor per account under a thin accounts layer: the actor already holds exactly one session, so it barely changes. One account is on screen at a time.

- [x] `rc_core`: split `client.rs` into the handle and the account actor (its `impl` blocks by concern), and the actor tests into a harness plus one file per topic. No behaviour change
- [x] `rc_core`: one shared core (runtime, connector, file cache, store worker) separate from the account actor. Still one account
- [ ] `rc_core`: the `SecretStore` port keeps a list of accounts (`AccountId`, server, user, token or none), with memory, file and keyring adapters (one keyring entry per account plus an index). The session stored by earlier builds is dropped: log in again
- [ ] `rc_core`: accounts layer. Add, log in, remove; events tagged with the account; unread and mention totals per account; all accounts resumed at startup, independently; logging out removes the account; an expired one stays listed without a token, for a prefilled login; the same server and user can't be added twice. Two users on one server are fine
- [ ] `rc_qt`: refactor the hub first, so that routing to an account is one place, not one per command: a single way to reach the client (today each of the twenty-odd commands repeats "find the client, else warn"), and the cache and the registered view models as separate parts. No behaviour change
- [ ] `rc_qt`: one cache per account and a current account that the existing view models show; an `Accounts` model; commands go to the current account
- [ ] QML: mockup first, then a rail of servers with badges and an add button, the login page in add-account mode with Cancel, and a prefilled login for an expired account
- [ ] Features ACCT-001…ACCT-007 tested. The second server in tests is the test server behind `rc_test_server::Proxy`

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

## Later

- Slint frontend (`rc_slint`, pure Rust) reusing `rc_core`. Separate plan.
- Spix RPC server in Rust for Slint (in the spix fork), checked by running the unchanged `rc_gui_tests` against `rc_slint`.
- Packaging (Flatpak, AppImage, Windows installer).

## Decisions

- **One GUI test suite for both frontends.** It speaks the Spix XML-RPC protocol. The Qt app embeds C++ Spix and the Slint app embeds a Rust Spix server. See [docs/design/gui-testing.md](docs/design/gui-testing.md).
- **Qt ≥ 6.12.0.**
- **No fake server.** Integration and GUI tests run against a real Rocket.Chat (`scripts/run_test_server.sh`), which CI shares. Failures that a real server can't produce on demand are tested with port fakes (`rc_core`) and wiremock (`rc_api`).
- **File attachments open their server URL in the browser, without the token.** That works when the browser has a Rocket.Chat session. Downloading them through the app comes with file upload (Phase 5). Images are shown inline from the file cache.
- **Rocket.Chat server.** Target the version the company server runs (check it with `/api/info` in Phase 1). Support older versions only where it's cheap. No compatibility shims for old servers.

# Issues

- **Bulk deletions aren't followed.** An admin pruning a room sends `<room id>/deleteMessageBulk` instead of one `deleteMessage` per message. The messages stay on screen until the room is opened again with a catch-up (`chat.syncMessages` lists them as deleted), so subscribe to it next to `deleteMessage`.
- **MongoDB 8.x won't start on Linux ≥ 6.19** (tcmalloc bug, [SERVER-121912](https://jira.mongodb.org/browse/SERVER-121912)). The test server uses MongoDB 7.0, which Rocket.Chat 8 accepts with a deprecation warning. Rocket.Chat 9 drops 7.0, so revisit before moving the test server to RC 9.
- **Windows isn't built yet.** MSVC Debug builds need `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL` and `MAP_IMPORTED_CONFIG_DEBUG=RELEASE` on the Qt targets (cxx-qt-cmake warns about this), and vcpkg's debug triplet builds with `/MDd`, which doesn't link against the Rust staticlib. Add a build-only `windows-latest` CI job before adding more C++ dependencies.
