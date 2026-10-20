# Architecture

A Rocket.Chat desktop client. The logic is in Rust and doesn't depend on any GUI. The first frontend is QML, connected to Rust through cxx-qt. A second frontend in Slint will come later and reuse everything below the GUI layer.

## Layers

```
┌───────────────────────────┐   ┌──────────────────────┐
│ QML (src/gui)             │   │ Slint (later)        │   views only, no logic
├───────────────────────────┤   ├──────────────────────┤
│ rc_qt  (cxx-qt bridge)    │   │ rc_slint (later)     │   view models, thread hop
├───────────────────────────┴───┴──────────────────────┤
│ rc_core  (application + domain)                      │   state, use cases, sync
├──────────────────────────────────────────────────────┤
│ rc_api   (Rocket.Chat protocol)                      │   REST + DDP, wire types
└──────────────────────────────────────────────────────┘
```

Dependencies only point downwards. Nothing in `rc_core` or `rc_api` knows about Qt or Slint.

- **rc_api**: a typed client for the Rocket.Chat REST API and the realtime DDP protocol over WebSocket. It keeps no application state. Wire (serde) types stay here, and `rc_core` maps them to domain types so that protocol quirks don't leak upwards.
- **rc_core**: domain types (rooms, messages, users, with ID newtypes), the session state machine, the sync engine and the use cases (log in, open a room, send...). Frontends drive it through a `Client` handle that accepts commands and emits events. It keeps lists such as the rooms in display order and the open room's messages (a `Timeline`, which merges history pages, de-duplicates and orders them) and emits index-based changes (insert, update, move, remove for rooms; reset, prepend, append, insert, update, move for messages), which view models apply one to one. It defines ports (traits) for what it needs from outside: the server API, secret storage, persistence (a `MessageStore` holds the messages loaded per room; in memory for now) and a cache directory. Adapters implement them, with in-memory versions for tests.
- **rc_qt**: cxx-qt QObjects and `QAbstractListModel`s that turn core events into Qt properties, signals and incremental model updates. It is the only crate that uses `unsafe` (cxx-qt bridge code).
- **QML**: presentation only. It binds to view models and calls their invokables. It has no business logic and makes no network calls.
- **C++ host** (`src/gui/main.cpp`): sets up logging through `rc_qt` (`rc_core::logging`: stderr, plus `<executable>.log` in developer-mode builds, filtered by `RUST_LOG`), creates `QGuiApplication`, starts the core (`rc_qt::start_core()`; `RC_SECRET_STORE` picks where the session is kept, see below), registers the Rust QML module, loads `Main.qml`, and, in builds with `ROCKETCHATQML_SPIX`, starts the Spix test server when given `--spix-port <port>`. In developer-mode builds, `--dev` fills in the login page for the local test server (`rc_qt::enable_dev_login()`; `RC_DEV_SERVER`, `RC_DEV_USER` and `RC_DEV_PASS` override it). Release builds compile neither the option (`#ifdef ROCKETCHATQML_DEVELOPER_MODE`) nor the Rust side (`developer-mode` feature). After the event loop ends it stops the core (`rc_qt::stop_core()`). It does nothing else.

`RC_SECRET_STORE` chooses the secret store at startup (`store_choice` in `crates/rc_qt/src/hub.rs`):

- unset, or any other value: the OS keyring, with memory as the fallback if there is none;
- `memory`: kept in memory only, forgotten at exit;
- `file:<path>`: a plain JSON file (`rc_core::adapters::FileStore`), so a GUI test can restart the app and resume. It writes the token in clear text, so it only exists in builds with the `test-file-store` cargo feature, which CMake turns on with `ROCKETCHATQML_SPIX`. Without the feature the value logs a warning and uses memory, never the keyring.

Adapter crates (added when needed): `rc_store_sqlite` (persistence) and a keyring-based secret store.

Test-only crates:

- **rc_gui_tests**: GUI tests that drive the running app through Spix's XML-RPC interface. They identify elements by path only, so the same tests can run against the Slint frontend. They run against the local test server.
- **rc_test_server**: blocking helpers that create users, rooms and messages on the local test server through its admin API (`Admin`, `unique()`), and `Proxy`, a TCP proxy in front of the server (std threads) that tests point the client at: `drop_connections()` cuts every open connection, and `set_accepting(false)` makes new ones close at once, so that a test can keep the client away for a while. The contract tests and the GUI tests share them. It may panic, since a failed call should fail the test.

There is no fake Rocket.Chat. Contract and GUI tests use the real local test server (`scripts/run_test_server.sh`), which CI shares. Failures it can't produce on demand are tested with port fakes in `rc_core`, wiremock in `rc_api` for REST, and for the DDP transport (silent servers, dropped connections, malformed frames) an in-process scripted WebSocket peer on an in-memory pipe, the WebSocket analogue of wiremock.

## Threading

- **GUI thread**: runs the Qt event loop and owns every QObject and model. Nothing on it blocks, and it never waits on the network.
- **Tokio runtime** (multi-threaded, few workers), owned by `rc_core`:
  - **Session actor**: a single task that owns the authoritative client state (session, room list, message store and the open room's timeline), so the state needs no mutexes. It processes commands one at a time. Each history fetch carries an attempt id, and its answer is dropped if the room was closed or another fetch began meanwhile.
  - **Store worker**: runs the secret-store calls (keyring or memory), which block and may wait for the user to unlock the keyring. It takes them in order, one at a time, on blocking threads, so a save and a later clear never swap and the async threads never stall.
  - **I/O tasks**: the DDP connection (read loop, ping), REST requests and avatar/file downloads. They report back to the actor through channels, tagged with the login attempt (or fetch) they belong to, and the actor drops what is stale. The sync engine is the actor plus one such task per connection, which opens the realtime connection (`ServerApi::realtime`, a receiver of pushed events that closes the connection when dropped) and forwards what it pushes. It works in cycles, one per connection: open the connection, fetch the rooms, and if a room is open fetch its newest page. The connection opens before the rooms are fetched, and what arrives until the list is set waits in a backlog and is replayed on top of it, so nothing between the snapshot and the subscription is lost. Subscription changes upsert or remove rooms; a pushed message joins the open room's timeline, and is ignored for any other room. A cycle ends when the connection ends or fails to open, or when its rooms fetch fails (an expired token ends the session instead): the actor drops the connection and starts a timer task that waits (`Backoff`: 1 s doubling to 60 s, cut randomly to between half and all of it, forgotten only when a cycle that opened the connection and listed the rooms then stayed connected for a minute, so a server that accepts and drops at once is not hammered), then starts the next cycle. The frontend is told with `Event::Connection` (`Waiting` with the wait, `Reconnecting` while a cycle tries, `Connected` once the rooms are listed again). Reports and timers carry the cycle number, and logout, expiry and stopping abort the timer, so a stale one does nothing. Only the first listing of a session resets the room list; later ones are merged with `RoomList::sync` into inserts, updates, moves and removals, and a room that is gone is closed if it was open. The open room's newest page is merged by the timeline: it is appended if it touches the messages held, and replaces them if more than a page was missed.
- **Commands (GUI → core)**: sent on a non-blocking `mpsc` channel.
- **Events (core → GUI)**: the frontend gives `Client` an event callback, which the actor calls in order from a runtime thread. In `rc_qt` the callback belongs to a hub (`hub.rs`), the one owner of the `Client` in the process. It updates the hub's cache (the latest session state and a mirror of the room list and of the open room's messages) and hands each event to every registered view model through `CxxQtThread::queue`, where the view model applies it as a diff (insert, update, move or remove rows). A view model registers when it is created, on the GUI thread, and replays the cache under the same lock that queueing uses, so it misses nothing and applies nothing twice whenever QML creates it. Neither side waits for the other. Qt objects are never touched from a Tokio thread.
- **Shutdown**: dropping `Client` ends the actor, which first waits briefly (2 s at most) for a pending logout: the token revocation and the store clear. The GUI thread blocks for that bounded time only.

## Sending messages

`Client::send_message` picks the message's id itself (17 characters, like the server's), so that a retry can't post it twice and the server's echo can be matched to the local copy. The message goes into the room's messages at once as `Pending` (`Delivery`), in the open timeline or, if the room isn't open, in what the `MessageStore` holds of it, and `chat.sendMessage` runs in a task. The message becomes sent when the REST answer or the realtime echo arrives, whichever comes first, and the other finds nothing left to settle; it becomes `Failed` on any other error (an expired token ends the session instead). `retry_message` sends a failed message again under the same id. Switching rooms in between changes nothing: the answer is applied to the room it belongs to, shown or not.

The server stores a message once per id. Sending an id it holds already (the first attempt arrived, but its answer was lost) is refused with a 400 whose text is an internal error (`Cannot read properties of undefined (reading 'starred')`), which is nothing to recognise. `rc_api` therefore looks the message up by id after any 400 and, if it is in the same room and from the same user, takes it as the answer, so a caller may retry whenever it doesn't know whether an attempt arrived.

In the `Timeline` the messages the server confirmed come first, in display order, and the unsent ones after them in the order they were written: their time is the user's clock, which says nothing about where they fall among the server's. History pages, live messages and the catch-up after a reconnect therefore go in front of the unsent messages, which they never drop, and a page or a pushed message that carries the id of an unsent one settles it. Settling is an `Updated` change (same position) or, if the server's time puts the message elsewhere among the confirmed ones, a `Moved` one. The message list is never reset for it. A message confirmed while the room's newest page is still loading is also kept for that page, which may predate it.

## GUI testing contract

There is one GUI test suite, `rc_gui_tests`, and it runs unchanged against both frontends. It is a Spix XML-RPC client and never links to either GUI. Each frontend embeds a server that speaks the same protocol:

- **QML frontend:** C++ Spix (`Spix::QtQuick`), started by the C++ host.
- **Slint frontend:** a Rust implementation of the Spix RPC server built on Slint's Rust testing API (`ElementHandle`, `query_descendants()`). The C++ `Spix::Slint` backend can't be used there: it needs C++-generated Slint components, and a Rust Slint app has a different Slint runtime.

Paths are kept shallow and decoupled from layout: `<window>/<name>`, where `name` is unique per window (for example `mainWindow/loginServerField`). A list row is addressed by its room id, not by its text (Spix's `"text"` selector), because room names aren't unique: the row's name is `room_<id>` and its parts (`roomName`, `roomKind_<kind>` (an icon, named after the room kind: `channel`, `private` or `direct`), `roomUnread`, `roomMentions`) sit below it, as in `mainWindow/room_<id>/roomUnread`. A message row is `msg_<message id>` the same way, with `msgAuthor`, `msgTime` and `msgBody` (or `msgSystem` for a system message, which has no author or body) and `msgDaySeparator` when the message starts a day, as in `mainWindow/msg_<id>/msgBody`; an unsent message also has `msgPending`, or `msgFailed` and `msgRetry`, which exist only in that state, so that a test, which can't know the id the client picked, finds the one unsent message by name alone (`mainWindow/msgFailed`); the composer is `composerField` (Enter sends, Shift+Enter starts a line) and `composerSend`; the list is `messageList` (its `count` is the loaded messages, not those in view), with `historyStart` above the first message once nothing older exists. The tests scroll with Home, since Spix has no wheel. Spix searches depth-first, so the path doesn't have to name the `ListView` or the delegate's other ancestors; it finds only rows that exist, and a `ListView` creates the delegates in view, so a test of a list longer than the window has to scroll first. In QML the name is the `objectName`; in Slint it is the `accessible-id`. The path constants live in `rc_gui_tests`, which is the single source of truth.

Tests use only the part of the Spix protocol that both servers implement: clicks, key and text input, `existsAndVisible`, get/set of `text`, `checked`, `enabled`, `count` and geometry, `wait` and `quit`, plus `getErrors`, which the `Gui` client checks after every call so that a wrong path fails the test. The core works asynchronously, so tests never sleep: `Gui::wait_until` and its `wait_until_visible`, `wait_until_hidden` and `wait_until_text` poll with a timeout and name what they were waiting for. `App::quit_and_wait` calls `quit` and waits for the app to exit by itself, so that it stops its core (a pending logout or store write finishes) before a test starts it again.

The harness starts the binary named by `RC_GUI_TEST_APP` with `--spix-port <free port>`, `QT_QPA_PLATFORM=offscreen` and `RC_SECRET_STORE=memory` (the app starts the core at launch, and its default secret store is the OS keyring, which on a developer machine is the user's real wallet). A test can override these with `App::launch_with`, for example with `RC_SECRET_STORE=file:<path>` to keep the session between two runs. The GUI tests are `#[ignore]`d, so a plain `cargo test` skips them (it runs before the app is built in CI) and never reports them as passed. ctest runs them with `--ignored` and `RC_GUI_TEST_APP` set; without it they fail. The tests that log in create their own users and rooms on the test server through `rc_test_server`, so they also need `RC_TEST_SERVER_URL`.

## Third-party dependencies

| Dependency | Purpose |
|---|---|
| Qt ≥ 6.12 (system) | QML runtime and Qt Quick Controls |
| cxx-qt / cxx | Rust ⇄ C++/Qt bridge, QObjects and models defined in Rust |
| 3rdparty/corrosion | Builds the Rust crates from CMake and links them into the Qt executable |
| 3rdparty/cxx-qt-cmake | CMake functions that import cxx-qt crates and their QML modules (uses Corrosion) |
| tokio | Async runtime for networking and the session actor |
| reqwest (rustls) | REST client |
| tokio-tungstenite | WebSocket transport for DDP |
| rustls-platform-verifier | The OS trust store for the WebSocket's TLS, as for reqwest |
| serde / serde_json | Wire-format (de)serialization |
| thiserror | Typed errors in library crates |
| tracing | Structured logging |
| ureq, roxmltree | HTTP and XML for the small XML-RPC client in `rc_gui_tests` (Spix uses a handful of value types, so no XML-RPC crate) |
| wiremock | Dev only: mock HTTP server for the `rc_api` tests |
| secrecy | Keeps passwords and tokens out of logs and `Debug` output |
| keyring-core + native stores | OS credential storage for the session token (on Linux the Secret Service through the system libdbus, which Qt loads too, so it needs `libdbus-1-dev` to build) |
| 3rdparty/spix (fork) | UI test server inside the app, with Qt Quick and Slint backends and XML-RPC control. Needs AnyRPC |
| 3rdparty/vcpkg (fork) | Provides the C++ dependencies Qt doesn't ship (AnyRPC for Spix). Pinned to the same commit as Spix's copy |
| 3rdparty/QtCommander | Development tool only: an MCP server so agents can inspect and drive the running app. Not linked into the product |
