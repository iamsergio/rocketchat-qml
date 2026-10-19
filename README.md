# RocketChatQml

A Rocket.Chat client with a Rust core and a QML GUI, connected through cxx-qt.

## Building

    git submodule update --init 3rdparty/corrosion 3rdparty/cxx-qt-cmake
    scripts/build.sh

Requires Qt ≥ 6.12 (found through `PATH` or `CMAKE_PREFIX_PATH`), CMake ≥ 3.24, Ninja and rustup.
The Rust toolchain version is pinned in `rust-toolchain.toml`; rustup installs it on first use.

See [PLAN.md](PLAN.md) for the roadmap and [ARCHITECTURE.md](ARCHITECTURE.md) for the design.
