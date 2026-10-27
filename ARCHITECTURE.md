# Architecture

A Rocket.Chat desktop client. The logic is in Rust and doesn't depend on any GUI. The first frontend is QML, connected to Rust through cxx-qt. A second frontend in Slint is growing next to it and reuses everything below the GUI layer.

## Layers

```
┌───────────────────────────┐   ┌──────────────────────┐
│ QML (src/gui)             │   │ Slint (ui/*.slint)   │   views only, no logic
├───────────────────────────┤   ├──────────────────────┤
│ rc_qt  (cxx-qt bridge)    │   │ rc_slint             │   view models, thread hop
├───────────────────────────┴───┴──────────────────────┤
│ rc_view  (presentation state per account)            │   caches, views, row operations
├──────────────────────────────────────────────────────┤
│ rc_core  (application + domain)                      │   state, use cases, sync
├──────────────────────────────────────────────────────┤
│ rc_api   (Rocket.Chat protocol)                      │   REST + DDP, wire types
└──────────────────────────────────────────────────────┘
```

Dependencies only point downwards. Nothing in `rc_view`, `rc_core` or `rc_api` knows about Qt or Slint.

- **rc_api**: a typed client for the Rocket.Chat REST API and the realtime DDP protocol over WebSocket. It keeps no application state. Wire (serde) types stay here, and `rc_core` maps them to domain types so that protocol quirks don't leak upwards.
- **rc_core**: domain types (rooms, messages, users, with ID newtypes), the session state machine, the sync engine and the use cases (log in, open a room, send...). Frontends drive it through a `Client` handle that accepts commands and emits events. The core runs any number of accounts, one session actor each, and always at least one; every event names its account, and `Added` and `Removed` bracket an account's events (see [Accounts](docs/design/accounts.md)). It keeps lists such as the rooms in display order and the open room's messages (a `Timeline`, which merges history pages, de-duplicates and orders them) and emits index-based changes (insert, update, move, remove for rooms; reset, prepend, append, insert, update, move, remove for messages), which view models apply one to one. It defines ports (traits) for what it needs from outside: the server API, secret storage, persistence (a `MessageStore` holds the messages loaded per room; in memory for now) and a file cache for downloads (`FileCache`). Adapters implement them, with in-memory versions for tests. Avatars and attachment images are downloaded into a file cache and sent as files (`Event::Avatars`, `Event::Images`); custom emoji images (`Event::EmojiImages`) only when a frontend asks with `want_emoji_images`, as one that can't load URLs must (Slint), so the Qt app, which loads them by URL, pays nothing.
- **rc_view**: what the screens show for each account, in plain Rust that knows no GUI toolkit, so every frontend uses it. The cache per account (`Cache`, with the `Accounts` that holds them and the current one), the views (the values and texts a screen shows for rooms, messages, quotes, reactions, attachments, avatars, the session, threads and the emoji picker), the `Viewer` (whoever looks at the messages), the row operations that turn core events into list changes, and the `MessageList` that applies them (the messages newest first, with the avatar and image URLs and the rows an update touches). It also renders rich text as the HTML subset of Qt's rich text (`rich_html.rs`): the Qt app shows it, and every frontend gives it to the GUI tests. Showing rich text otherwise is each frontend's job. Its `developer-mode` feature (`--dev` fills the login page in) is set through the one of `rc_qt` or `rc_slint`.
- **rc_qt**: cxx-qt QObjects and `QAbstractListModel`s that turn core events into Qt properties, signals and incremental model updates, on top of `rc_view`. Its hub carries out on the Qt view models what `rc_view::switchboard` decides (which rows of the account list change, when every view gets a full snapshot, which events reach the views): the switchboard keeps an `rc_view` cache per account and the view models show the current one, one at a time; the `AccountList` model lists them all (see [Accounts](docs/design/accounts.md)). It also has the thread hop. It is the only crate that uses `unsafe` (cxx-qt bridge code).
- **rc_slint**: the Slint frontend, pure Rust (no Qt backend, no cxx-qt). The `rocketchat_slint` binary, built by cargo or by CMake with the `dev-slint`/`rel-slint` presets, with the `.slint` files in `crates/rc_slint/ui` (split like the QML: pages, the rail and its tile, the room list and its rows, shared widgets) and their visual constants in the `Theme` global (`theme.slint`), which mirrors `Theme.qml`; the font and icons are the QML app's. `main` sets up logging, handles `--dev`, makes the window and starts the core (`app::start`: the secret store from `RC_SECRET_STORE`, files under `$XDG_CACHE_HOME/rocketchat_slint`), runs the event loop and then drops the core. All state is one `App` on the GUI thread, in a thread local: the `Client`, an `rc_view::switchboard::Switchboard` that decides what each event, and selecting, adding and removing an account, change on screen (the same one the Qt hub uses), and the Slint models it shows them in: the tiles of the rail (a `VecModel`, changed row by row) and the room list (`RoomModel`, which applies the core's room changes as row changes, adds a heading row before each section while there are favorites, and makes a row, avatar image included, only when the list view asks for it). The core's callback only hops to the GUI thread with `slint::invoke_from_event_loop`. It shows the startup page, the login page (its three modes), the rail with the account tiles, the sidebar with the server, the user and the connection banner, the room list with unread and mention badges and avatars or initials, the header of the open room (kind icon, name, topic as plain text) and logging out from the settings button or the tile's menu. Messages, threads, the composer, the room menu and tool tips aren't there yet. The names of the GUI tests are the `accessible-id`s.
- **QML**: presentation only. It binds to view models and calls their invokables. It has no business logic and makes no network calls. The visual constants (sizes, spacing, radii, fonts, the error colour) live in the `Theme` singleton, `src/gui/Theme.qml`; the other colours come from the Controls palette, so they follow the system theme.
- **C++ host** (`src/gui/main.cpp`): sets up logging through `rc_qt` (`rc_core::logging`: stderr, plus `<executable>.log` in developer-mode builds, filtered by `RUST_LOG`), creates `QGuiApplication`, starts the core (`rc_qt::start_core(cacheDir)`, with `QStandardPaths::CacheLocation`; `RC_SECRET_STORE` picks where the session is kept, see below), registers the Rust QML module, loads `Main.qml`, and, in builds with `ROCKETCHATQML_SPIX`, starts the Spix test server when given `--spix-port <port>`. In developer-mode builds, `--dev` fills in the login page for the local test server (`rc_qt::enable_dev_login()`; `RC_DEV_SERVER`, `RC_DEV_USER` and `RC_DEV_PASS` override it). Release builds compile neither the option (`#ifdef ROCKETCHATQML_DEVELOPER_MODE`) nor the Rust side (`developer-mode` feature). After the event loop ends it stops the core (`rc_qt::stop_core()`). It does nothing else.

`RC_SECRET_STORE` chooses the secret store at startup (`rc_view::startup`). The store keeps a list of accounts (id, server, user, token or none), in the order they were first saved; the keyring keeps one entry per account plus an index entry with the ids in order, the file one JSON list:

- unset, or any other value: the OS keyring, with memory as the fallback if there is none;
- `memory`: kept in memory only, forgotten at exit;
- `file:<path>`: a plain JSON file (`rc_core::adapters::FileStore`), so a GUI test can restart the app and resume. It writes the token in clear text, so it only exists in builds with the `test-file-store` cargo feature, which CMake turns on with `ROCKETCHATQML_SPIX`. Without the feature the value logs a warning and uses memory, never the keyring.

Adapter crates (added when needed): `rc_store_sqlite` (persistence) and a keyring-based secret store.

Test-only crates:

- **rc_gui_tests**: GUI tests that drive the running app through Spix's XML-RPC interface. They identify elements by path only, so the same tests can run against the Slint frontend. They run against the local test server.
- **rc_test_server**: blocking helpers that create users, rooms and messages on the local test server through its admin API (`Admin`, which also edits, deletes and reacts to messages and can log in as any user, and `unique()`), and `Proxy`, a TCP proxy in front of the server (std threads) that tests point the client at: `drop_connections()` cuts every open connection, and `set_accepting(false)` makes new ones close at once, so that a test can keep the client away for a while. The contract tests and the GUI tests share them. It may panic, since a failed call should fail the test.

There is no fake Rocket.Chat. Contract and GUI tests use the real local test server (`scripts/run_test_server.sh`), which CI shares. Failures it can't produce on demand are tested with port fakes in `rc_core`, wiremock in `rc_api` for REST, and for the DDP transport (silent servers, dropped connections, malformed frames) an in-process scripted WebSocket peer on an in-memory pipe, the WebSocket analogue of wiremock.

## Topics

- [Threading](docs/design/threading.md): GUI thread, session actor, sync engine and reconnects, how events reach the view models
- [Accounts](docs/design/accounts.md): what an account is, adding and removing, the core task, event order, how rc_qt follows one account
- [Sending messages](docs/design/sending.md): client-picked ids, optimistic echo, retry, ordering of unsent messages
- [Rich text](docs/design/rich-text.md): the block/inline model, mapping the server's `md` tree, rendering to Qt rich text, mentions, emoji
- [Avatars](docs/design/avatars.md): file cache, which avatars exist, refresh policy, Qt side
- [Edits, deletions and reactions](docs/design/edits-deletions-reactions.md): following live changes, deletions of the open room, catch-up
- [Reacting](docs/design/reacting.md): adding and taking back reactions, applying them at once, the emoji picker, Qt side
- [Room list](docs/design/room-list.md): order and favorites section, the room menu and its rules, the server calls, applying them at once, Qt side
- [Attachments](docs/design/attachments.md): authenticated downloads, image cache, Qt side
- [File upload](docs/design/uploads.md): the server's two steps, the queue, states, cancel and retry, how the message enters the timeline, the rows of `rc_view`
- [Editing and deleting your own messages](docs/design/editing-own-messages.md): server policy, rules, answers, composer edit mode
- [Quoting](docs/design/quoting.md): permalinks, `Site_Url`, hiding quote links, sending
- [Threads](docs/design/threads.md): what the server does, opening a thread, replying, the thread pane, Qt side, the list of a room's threads
- [GUI testing contract](docs/design/gui-testing.md): Spix protocol subset, element paths, harness environment
- [Performance tests](docs/design/performance-tests.md): `rc_gui_perf`, `perf stat` runs of micro scenarios against both apps, the scroll scenario, the JSON of a run
- [Third-party dependencies](docs/design/dependencies.md): what each dependency is for
