# Third-party dependencies

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
| emojis | Emoji shortcodes (gemoji names) to Unicode, in `rc_core::emoji` |
| thiserror | Typed errors in library crates |
| tracing | Structured logging |
| ureq, roxmltree | HTTP and XML for the small XML-RPC client in `rc_gui_tests` (Spix uses a handful of value types, so no XML-RPC crate) |
| wiremock | Dev only: mock HTTP server for the `rc_api` tests |
| secrecy | Keeps passwords and tokens out of logs and `Debug` output |
| keyring-core + native stores | OS credential storage for the session token (on Linux the Secret Service through the system libdbus, which Qt loads too, so it needs `libdbus-1-dev` to build) |
| 3rdparty/spix (fork) | UI test server inside the app, with Qt Quick and Slint backends and XML-RPC control. Needs AnyRPC |
| 3rdparty/vcpkg (fork) | Provides the C++ dependencies Qt doesn't ship (AnyRPC for Spix). Pinned to the same commit as Spix's copy |
| 3rdparty/QtCommander | Development tool only: an MCP server so agents can inspect and drive the running app. Not linked into the product |
