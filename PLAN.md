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
- Every feature gets a unit test, a core integration test against `rc_fake_server`, or both. Every user-visible flow also gets a GUI test.
- Commit hygiene: `cargo fmt`, clippy, `cargo test` and the GUI tests all pass before each commit.

## Phase 0: Skeleton

Done when: the app opens a window showing the version string, which comes from Rust. One GUI smoke test passes locally and in CI.

- [x] Cargo workspace at the repo root (`crates/`), `rust-toolchain.toml`, workspace lints, `rustfmt.toml`
- [x] Empty crates: `rc_api`, `rc_core`, `rc_qt`, `rc_fake_server`, `rc_gui_tests`
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

- [ ] `rc_api`: REST client with auth headers, `GET /api/info` (server version check), `login`, `me`, `logout`, `subscriptions.get`
- [ ] `rc_core`: `Client` handle plus session actor. Session state machine: `LoggedOut → Connecting → LoggedIn`, with failure reasons
- [ ] `rc_core`: `SecretStore` port with keyring and in-memory adapters. Resume the session from the stored token at startup
- [ ] `rc_core`: room domain model (channel/group/DM, name, unread, mention count)
- [ ] `rc_fake_server`: REST subset for the endpoints above, scriptable users and rooms
- [ ] `rc_qt`: `SessionVm` (state, error, login/logout invokables), `RoomListModel`
- [ ] QML: login page (server URL, user, password, error text, busy state), room list, logout
- [ ] Features AUTH-001…AUTH-005, ROOM-001…ROOM-003 tested

## Phase 2: Read messages

Done when: clicking a room shows its history with authors and times, and scrolling up loads older messages.

- [ ] `rc_api`: room history (`channels/groups/im.history`, depending on room type)
- [ ] `rc_core`: message domain model (plain text body for now), `MessageStore` port with an in-memory adapter, pagination cursor per room
- [ ] `rc_core`: system messages (join, leave, topic changed) as their own kind
- [ ] `rc_fake_server`: history with pagination
- [ ] `rc_qt`: `MessageListModel` (prepend for older messages, append for newer), `RoomVm` (current room)
- [ ] QML: room view, message delegate, day separators, load older on reaching the top
- [ ] Features MSG-001…MSG-005 tested

## Phase 3: Realtime and sending

Done when: messages from others appear live, unread badges update, you can send messages, and the client recovers after the network drops.

- [ ] `rc_api`: DDP client (connect, ping/pong, method calls, subscriptions, resume-token login)
- [ ] `rc_core`: sync engine. Subscribe to `__my_messages__` and `stream-notify-user` (subscriptions/rooms changed), route events into state
- [ ] `rc_core`: reconnect with exponential backoff and jitter. Catch up on missed messages after reconnecting
- [ ] `rc_core`: send with an optimistic local echo (pending → sent / failed), retry
- [ ] `rc_core`: mark a room read when it's opened. Room list ordered by last activity
- [ ] `rc_fake_server`: DDP endpoint (scriptable pushes, drop connection on demand)
- [ ] `rc_qt` / QML: composer, pending/failed indicators with retry, connection status banner
- [ ] Contract tests: core integration tests run against the local test server (opt-in with `RC_TEST_SERVER_URL`), so the fake server stays accurate. CI runs them in a separate job on a fresh runner that starts the server with `scripts/run_test_server.sh`
- [ ] Contract tests must pass on a dirty server, in any order and in parallel:
  - each test creates its own users and rooms with unique names (per-run random suffix) through the admin API
  - tests never change server settings. A setting tests need goes into the script's `OVERWRITE_SETTING_*` list, so it's part of the server definition
  - tests assert only on objects they created (no "the room list has exactly N rooms")
- [ ] Features RT-001…RT-004, SEND-001, SEND-002, ROOM-004, ROOM-005 tested

## Phase 4: Rich content

Done when: messages render formatting, mentions, emoji, avatars, images and reactions, and live edits/deletes show up.

- [ ] `rc_core`: convert the server's parsed-markdown (`md`) AST into a GUI-agnostic rich-text block model. Each frontend renders that model. Fall back to `msg` text when there is no `md`
- [ ] `rc_qt`: render blocks to the subset of rich text that Qt supports (links, code, quotes, lists)
- [ ] Mentions (highlight @me), unicode emoji shortcodes, custom server emoji
- [ ] `rc_core`: file cache port. Avatars and images are downloaded to the cache directory and exposed to the GUI as local file URLs
- [ ] Image attachments shown inline, other attachments as links
- [ ] Realtime message edits and deletes, plus a reactions display
- [ ] Features RICH-001…RICH-007 tested

## Phase 5: Interacting

Done when: day-to-day use works. Edit, delete, react, threads, DMs, search and uploads.

- [ ] Edit and delete your own messages
- [ ] Add and remove reactions (emoji picker)
- [ ] Threads: open a thread panel, reply, see thread counts in the main timeline
- [ ] Reply/quote a message
- [ ] Typing indicators (send and receive)
- [ ] Start a DM, search the directory, join a channel
- [ ] File upload with progress
- [ ] Features ACT-001…ACT-009 tested

## Phase 6: Persistence and desktop integration

Done when: the app starts instantly from cache (works offline read-only), shows desktop notifications, and has a settings page.

- [ ] `rc_store_sqlite`: `MessageStore` and room cache adapter, schema migrations
- [ ] Startup from cache, then a delta sync
- [ ] Desktop notifications for DMs and mentions, unread count in the window title or tray
- [ ] Settings page (notifications, theme)
- [ ] Features PERS-001, PERS-002, DESK-001…DESK-003 tested

## Phase 7: Authentication extras

Done when: you can log in to servers that enforce 2FA or use SSO.

- [ ] 2FA (TOTP and email code)
- [ ] Personal access token login
- [ ] OAuth/SSO login through the browser
- [ ] Multiple server accounts
- [ ] Features AUTH-006…AUTH-009 tested

## Later

- Slint frontend (`rc_slint`, pure Rust) reusing `rc_core`. Separate plan.
- Spix RPC server in Rust for Slint (in the spix fork), checked by running the unchanged `rc_gui_tests` against `rc_slint`.
- Packaging (Flatpak, AppImage, Windows installer).

## Decisions

- **One GUI test suite for both frontends.** It speaks the Spix XML-RPC protocol. The Qt app embeds C++ Spix and the Slint app embeds a Rust Spix server. See ARCHITECTURE.md, GUI testing contract.
- **Qt ≥ 6.12.0.**
- **Rocket.Chat server.** Target the version the company server runs (check it with `/api/info` in Phase 1). Support older versions only where it's cheap. No compatibility shims for old servers.

# Issues

- **MongoDB 8.x won't start on Linux ≥ 6.19** (tcmalloc bug, [SERVER-121912](https://jira.mongodb.org/browse/SERVER-121912)). The test server uses MongoDB 7.0, which Rocket.Chat 8 accepts with a deprecation warning. Rocket.Chat 9 drops 7.0, so revisit before moving the test server to RC 9.
- **Windows isn't built yet.** MSVC Debug builds need `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL` and `MAP_IMPORTED_CONFIG_DEBUG=RELEASE` on the Qt targets (cxx-qt-cmake warns about this), and vcpkg's debug triplet builds with `/MDd`, which doesn't link against the Rust staticlib. Add a build-only `windows-latest` CI job before adding more C++ dependencies.
