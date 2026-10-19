# RocketChatQml

A Rocket.Chat client with a Rust core and a QML GUI, connected through cxx-qt.

## Building

    git submodule update --init
    scripts/build.sh

Requires Qt ≥ 6.12 (found through `PATH` or `CMAKE_PREFIX_PATH`), CMake ≥ 3.24, Ninja and rustup.
The Rust toolchain version is pinned in `rust-toolchain.toml`; rustup installs it on first use.
The first configure bootstraps vcpkg (`3rdparty/vcpkg`), which needs network access and curl, zip,
unzip and tar.

See [PLAN.md](PLAN.md) for the roadmap and [ARCHITECTURE.md](ARCHITECTURE.md) for the design.

## CI runner

CI runs on a self-hosted runner in a container (`.github/Dockerfile`). Start it on a host with Docker
(or `RC_ENGINE=podman`) and the GitHub CLI logged in as a repository admin:

    scripts/run_ci_runner.sh
