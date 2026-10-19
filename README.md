# RocketChatQml

A Rocket.Chat client with a Rust core and a QML GUI, connected through cxx-qt.

## Building

    git submodule update --init
    scripts/build.sh

Requires Qt ≥ 6.12 (its `bin` directory on `PATH`), CMake ≥ 3.24, Ninja, a C++23 compiler and
rustup. The Rust toolchain version is pinned in `rust-toolchain.toml`; rustup installs it on first
use. The first configure bootstraps vcpkg (`3rdparty/vcpkg`), which needs network access and curl,
zip, unzip and tar.

`scripts/build.sh` uses the `dev` preset (`build-dev/`): a debug build with the Spix UI test server,
whose AnyRPC dependency comes from vcpkg. `cmake --preset=rel && cmake --build build-rel` is a
release build without Spix, which installs nothing from vcpkg.

The app logs to stderr (`RUST_LOG` sets the filter, `info` by default). Developer builds also append
to `build-dev/bin/rocketchat_qml.log`.

## Testing

    ctest --test-dir build-dev --output-on-failure

That runs `cargo test --workspace` and the GUI tests (`crates/rc_gui_tests`), which start the built
app offscreen and drive it through Spix. A plain `cargo test` skips the GUI tests; to run them by
hand:

    RC_GUI_TEST_APP=$PWD/build-dev/bin/rocketchat_qml cargo test -p rc_gui_tests -- --ignored

See [PLAN.md](PLAN.md) for the roadmap, [ARCHITECTURE.md](ARCHITECTURE.md) for the design and
[TRACEABILITY.md](TRACEABILITY.md) for the features and their tests.

## Agents

`.mcp.json` starts QtCommander, an MCP server to inspect and drive the running app. It needs
[uv](https://docs.astral.sh/uv) and the private submodule:

    git submodule update --init --checkout 3rdparty/QtCommander

## CI runner

CI runs on a self-hosted runner in a container (`.github/Dockerfile`). Start it on a host with Docker
(or `RC_ENGINE=podman`) and the GitHub CLI logged in as a repository admin:

    scripts/run_ci_runner.sh
