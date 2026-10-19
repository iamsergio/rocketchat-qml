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
- **rc_core**: domain types (rooms, messages, users, with ID newtypes), the session state machine, the sync engine and the use cases (log in, open a room, send...). Frontends drive it through a `Client` handle that accepts commands and emits events. It defines ports (traits) for what it needs from outside: secret storage, persistence and a cache directory. Adapters implement them, with in-memory versions for tests.
- **rc_qt**: cxx-qt QObjects and `QAbstractListModel`s that turn core events into Qt properties, signals and incremental model updates. It is the only crate that uses `unsafe` (cxx-qt bridge code).
- **QML**: presentation only. It binds to view models and calls their invokables. It has no business logic and makes no network calls.
- **C++ host** (`src/gui/main.cpp`): creates `QGuiApplication`, registers the Rust QML module, loads `Main.qml`, and, in builds with `ROCKETCHATQML_SPIX`, starts the Spix test server when given `--spix-port <port>`. It does nothing else.

Adapter crates (added when needed): `rc_store_sqlite` (persistence) and a keyring-based secret store.

Test-only crates:

- **rc_fake_server**: a scriptable in-process fake Rocket.Chat (REST and DDP). Used by core integration tests and GUI tests, so neither needs a real server.
- **rc_gui_tests**: GUI tests that drive the running app through Spix's XML-RPC interface. They identify elements by path only, so the same tests can run against the Slint frontend.

## Threading

- **GUI thread**: runs the Qt event loop and owns every QObject and model. Nothing on it blocks, and it never waits on the network.
- **Tokio runtime** (multi-threaded, few workers), owned by `rc_core`:
  - **Session actor**: a single task that owns the authoritative client state, so the state needs no mutexes. It processes commands one at a time.
  - **I/O tasks**: the DDP connection (read loop, ping, reconnect with backoff), REST requests and avatar/file downloads. They report back to the actor through channels.
- **Commands (GUI → core)**: sent on a non-blocking `mpsc` channel.
- **Events (core → GUI)**: a forwarder task receives core events and hands each one to the GUI thread through `CxxQtThread::queue`, where the view model applies it as a diff (insert, update or remove rows). Qt objects are never touched from a Tokio thread.

## GUI testing contract

There is one GUI test suite, `rc_gui_tests`, and it runs unchanged against both frontends. It is a Spix XML-RPC client and never links to either GUI. Each frontend embeds a server that speaks the same protocol:

- **QML frontend:** C++ Spix (`Spix::QtQuick`), started by the C++ host.
- **Slint frontend:** a Rust implementation of the Spix RPC server built on Slint's Rust testing API (`ElementHandle`, `query_descendants()`). The C++ `Spix::Slint` backend can't be used there: it needs C++-generated Slint components, and a Rust Slint app has a different Slint runtime.

Paths are kept shallow and decoupled from layout: `<window>/<name>`, where `name` is unique per window (for example `mainWindow/loginServerField`). List rows are addressed with Spix's `"text"` selector. In QML the name is the `objectName`; in Slint it is the `accessible-id`. The path constants live in `rc_gui_tests`, which is the single source of truth.

Tests use only the part of the Spix protocol that both servers implement: clicks, key and text input, `existsAndVisible`, get/set of `text`, `checked`, `enabled`, `count` and geometry, `wait` and `quit`.

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
| serde / serde_json | Wire-format (de)serialization |
| thiserror | Typed errors in library crates |
| tracing | Structured logging |
| secrecy | Keeps passwords and tokens out of logs and `Debug` output |
| keyring | OS credential storage for the session token |
| 3rdparty/spix (fork) | UI test server inside the app, with Qt Quick and Slint backends and XML-RPC control. Needs AnyRPC |
| 3rdparty/vcpkg (fork) | Provides the C++ dependencies Qt doesn't ship (AnyRPC for Spix). Pinned to the same commit as Spix's copy |
| 3rdparty/QtCommander | Development tool only: an MCP server so agents can inspect and drive the running app. Not linked into the product |
