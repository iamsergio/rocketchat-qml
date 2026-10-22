#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Rust code coverage of rc_api, rc_core, rc_view and rc_qt, from every kind of test: the unit tests, the
# contract tests (they need the Rocket.Chat test server, see scripts/run_test_server.sh) and the GUI
# tests, which run the app built by CMake with coverage instrumentation. Needs cargo-llvm-cov and
# rustup's llvm-tools-preview component.
#
# Writes the HTML report to target/llvm-cov/html and a markdown summary to target/llvm-cov/summary.md.
# CI also appends the summary to $GITHUB_STEP_SUMMARY.
# Usage: scripts/coverage.sh [--no-gui]   (--no-gui: leave the GUI tests out, to compare)

set -euo pipefail
cd "$(dirname "$0")/.."

gui=1
[[ ${1:-} == --no-gui ]] && gui=0

# Not code to measure: dependencies, the standard library, generated code (under target/), the test
# crates and the tests/ directories of the others.
ignore='(/\.cargo/|/\.rustup/|/rustc/|/target/|/crates/(rc_gui_tests|rc_test_server)/|/tests/)'

# Before the instrumentation environment below: Corrosion must not see it. The app is instrumented
# by the 'coverage' preset instead, into its own build directory.
if ((gui)); then
    cmake --preset=coverage
    cmake --build build-coverage
    app=$PWD/build-coverage/bin/rocketchat_qml
fi

# Makes every cargo build and run below instrument the code and write profiles to target/. The
# app inherits LLVM_PROFILE_FILE, whose %p keeps each process to a file of its own.
source <(cargo llvm-cov show-env --sh)
cargo llvm-cov clean --workspace

cargo test --workspace --locked
# Only these two suites: --ignored on the whole workspace would also run the GUI tests and the OS
# keyring test, which must never run in CI.
cargo test -p rc_api --test contract --locked -- --ignored
cargo test -p rc_core --test contract --locked -- --ignored
if ((gui)); then
    RC_GUI_TEST_APP=$app cargo test -p rc_gui_tests --locked -- --ignored
fi

# cargo llvm-cov report can't take the app as an extra object, so the report is made with LLVM's
# own tools, from the objects of all the runs: the test binaries (cargo lists them) and the app.
llvm=$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin
objects=()
for exe in $(cargo test --workspace --locked --no-run --message-format=json |
        grep -o '"executable":"[^"]*"' | sed 's/"executable":"\(.*\)"/\1/'); do
    objects+=(-object "$exe")
done
((gui)) && objects+=(-object "$app")

out=target/llvm-cov
mkdir -p $out
# -sparse: drop the counters that stayed zero. The profiles of the different processes add up.
"$llvm/llvm-profdata" merge -sparse target/rocketchat-qml-*.profraw -o $out/all.profdata
cov=("$llvm/llvm-cov")
args=(-instr-profile=$out/all.profdata "${objects[@]}" -ignore-filename-regex "$ignore")
rm -rf $out/html
"${cov[@]}" show -format=html -output-dir=$out/html -show-line-counts-or-regions "${args[@]}"
"${cov[@]}" export -summary-only "${args[@]}" > $out/summary.json
python3 -I scripts/coverage_summary.py $out/summary.json "$PWD" > $out/summary.md
cat $out/summary.md
