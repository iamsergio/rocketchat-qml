# RocketChatQml

A Rocket.Chat client with a Rust core and a QML GUI, connected through cxx-qt.

## Building

    git submodule update --init
    scripts/build.sh

Requires Qt ≥ 6.12 (its `bin` directory on `PATH`), CMake ≥ 3.24, Ninja, a C++23 compiler and
rustup. The Rust toolchain version is pinned in `rust-toolchain.toml`; rustup installs it on first
use. The first configure bootstraps vcpkg (`3rdparty/vcpkg`), which needs network access and curl,
zip, unzip and tar. On Linux it also needs the libdbus development package (`libdbus-1-dev` on
Debian/Ubuntu, `dbus` on Arch) and pkg-config able to find `dbus-1`: the session is kept in the
Secret Service (the desktop's keyring) through the system libdbus.

`scripts/build.sh` uses the `dev` preset (`build-dev/`): a debug build with the Spix UI test server,
whose AnyRPC dependency comes from vcpkg. `cmake --preset=rel && cmake --build build-rel` is a
release build without Spix, which installs nothing from vcpkg.

The Slint frontend is pure Rust and needs neither Qt nor vcpkg: `cmake --preset=dev-slint && cmake
--build build-dev-slint` (or `rel-slint`) builds `rocketchat_slint` into `build-dev-slint/bin`. Those
presets turn `ROCKETCHATQML_QT` off and `ROCKETCHATQML_SLINT` on; both can be on together. `cargo
build -p rc_slint` works too.

The app logs to stderr (`RUST_LOG` sets the filter, `info` by default). Developer builds also append
to `build-dev/bin/rocketchat_qml.log`.

To try a change against the local test server, `build-dev/bin/rocketchat_qml --dev` starts with the
login page filled in for it (`http://127.0.0.1:3000`, `admin` / `admin`; `RC_DEV_SERVER`,
`RC_DEV_USER` and `RC_DEV_PASS` override them). Only developer builds have `--dev`.

## Testing

    ctest --test-dir build-dev --output-on-failure

That runs `cargo test --workspace` and the GUI tests (`crates/rc_gui_tests`), which start the built
app offscreen and drive it through Spix. The GUI tests log in to the local test server, so ctest needs
it: start it with `scripts/run_test_server.sh` and set `RC_TEST_SERVER_URL`:

    RC_TEST_SERVER_URL=http://127.0.0.1:3000 ctest --test-dir build-dev --output-on-failure

In the Slint presets (no Qt) ctest runs `cargo test --workspace --exclude rc_qt`; they have no GUI
tests yet.

A plain `cargo test` skips the GUI tests; to run them by hand:

    RC_GUI_TEST_APP=$PWD/build-dev/bin/rocketchat_qml RC_TEST_SERVER_URL=http://127.0.0.1:3000 \
        cargo test -p rc_gui_tests -- --ignored

The app under test keeps its session in memory or in a temporary file, never in your keyring
(`RC_SECRET_STORE`, see [ARCHITECTURE.md](ARCHITECTURE.md)).

The `rc_api` and `rc_core` contract tests run against a real Rocket.Chat server and are skipped the
same way. Start one with `scripts/run_test_server.sh`, then:

    RC_TEST_SERVER_URL=http://127.0.0.1:3000 cargo test -p rc_api --test contract -- --ignored
    RC_TEST_SERVER_URL=http://127.0.0.1:3000 cargo test -p rc_core --test contract -- --ignored

`scripts/check.sh` runs these with short output, one line per step and only the failing lines of a
step that fails: `quick -p <crate> -g <gui test file>` while working, `full` before a commit (what
CI runs on the `dev` preset).

The GUI tests and these create their own users and rooms (`crates/rc_test_server`), so they can run
repeatedly against the same server. The admin login comes from `RC_ADMIN_USER` / `RC_ADMIN_PASS` (default `admin` / `admin`).

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

The runner joins the `rc-test` network, and CI runs the `rc_api` and `rc_core` contract tests against
the test server. It has to be running on the same host (`scripts/run_test_server.sh`). After changing
`scripts/run_ci_runner.sh`, restart the runner with it.

More runners run more jobs at once. Each one is a container of its own, numbered from 1, and they all
share the one test server:

    RC_RUNNER=2 scripts/run_ci_runner.sh
