#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# The checks of a change, with short output: one line per step, and for a failing step only the
# lines that say what failed. Each step's whole log is kept in target/check-logs/<step>.log.
#
# Usage:
#   scripts/check.sh quick [-p <crate>]... [-g <gui test file>]... [-s <gui test file>]...
#       While working: fmt, then clippy and the unit tests of the given crates (the workspace if
#       none), then the given GUI test files (crates/rc_gui_tests/tests/<name>.rs) against the
#       Qt app in build-dev (-g) or, for the tests of the file listed in
#       crates/rc_gui_tests/slint.txt, the Slint app in build-dev-slint (-s). The build is
#       rebuilt first.
#   scripts/check.sh full
#       Before a commit, what CI runs on the dev and dev-slint presets: fmt, clippy (also with
#       Slint's Spix), cargo deny, the workspace's tests, the contract tests, both builds, qmllint,
#       every GUI test against the Qt app and those of slint.txt against the Slint app, carrying on
#       past a failing GUI test file.
#
# The contract and GUI tests need the Rocket.Chat test server (scripts/run_test_server.sh), at
# RC_TEST_SERVER_URL (default http://127.0.0.1:3000). The app under test never uses the keyring.

set -u

cd "$(dirname "$0")/.." || exit 1

export RC_TEST_SERVER_URL=${RC_TEST_SERVER_URL:-http://127.0.0.1:3000}
export RC_SECRET_STORE=memory
QT_APP=$PWD/build-dev/bin/rocketchat_qml
SLINT_APP=$PWD/build-dev-slint/bin/rocketchat_slint
SLINT_TESTS=crates/rc_gui_tests/slint.txt

LOGS=target/check-logs
mkdir -p "$LOGS"
failed=()

# Runs a step quietly: `step <name> <command...>`.
step() {
    local name=$1
    shift
    local log="$LOGS/$name.log"
    local start=$SECONDS
    if "$@" >"$log" 2>&1; then
        echo "ok    $name ($((SECONDS - start))s)"
    else
        echo "FAIL  $name ($((SECONDS - start))s), log: $log"
        grep -E '^(error|warning)(\[|:)|FAILED|panicked at|^  (left|right):|Timeout \{|^Diff in|^Warning: |Error:' "$log" |
            grep -v '^error: test failed, to rerun' | head -n 40 | sed 's/^/      /'
        failed+=("$name")
    fi
}

server_up() {
    if ! curl -fsS --max-time 5 "$RC_TEST_SERVER_URL/api/info" >/dev/null; then
        echo "FAIL  the test server at $RC_TEST_SERVER_URL doesn't answer: start it with scripts/run_test_server.sh"
        exit 1
    fi
}

# Builds a preset's build directory, configuring it first if it isn't yet: `build_preset <preset>`.
build_preset() {
    if [ ! -f "build-$1/build.ninja" ]; then
        cmake --preset "$1" || return
    fi
    cmake --build "build-$1"
}

gui_tests() {
    RC_GUI_TEST_APP=$QT_APP cargo test -p rc_gui_tests --locked "$@" -- --ignored
}

# The test files that have tests in slint.txt, one per line.
slint_files() {
    grep -Ev '^ *(#|$)' "$SLINT_TESTS" | sed 's/::.*//' | uniq
}

# Runs the tests of one file listed in slint.txt against the Slint app: `slint_gui_tests <file>`.
# The same as the ctest entries of CMakeLists.txt.
slint_gui_tests() {
    local names
    mapfile -t names < <(grep -E "^ *$1::" "$SLINT_TESTS" | sed 's/^ *[^:]*:://; s/ *$//')
    if [ ${#names[@]} -eq 0 ]; then
        echo "Error: $SLINT_TESTS lists no tests of $1"
        return 1
    fi
    RC_GUI_TEST_APP=$SLINT_APP cargo test -p rc_gui_tests --test "$1" --locked -- --ignored --exact "${names[@]}"
}

mode=${1:-}
shift || true

case $mode in
quick)
    crates=()
    guis=()
    slint_guis=()
    while [ $# -gt 0 ]; do
        case $1 in
        -p) crates+=(-p "$2"); shift 2 ;;
        -g) guis+=("$2"); shift 2 ;;
        -s) slint_guis+=("$2"); shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
        esac
    done
    [ ${#crates[@]} -eq 0 ] && crates=(--workspace)
    step fmt cargo fmt --all --check
    step clippy cargo clippy "${crates[@]}" --all-targets --locked -- -D warnings
    step test cargo test "${crates[@]}" --locked
    if [ ${#guis[@]} -gt 0 ] || [ ${#slint_guis[@]} -gt 0 ]; then
        server_up
    fi
    if [ ${#guis[@]} -gt 0 ]; then
        step build build_preset dev
        for gui in "${guis[@]}"; do
            step "gui-$gui" gui_tests --test "$gui"
        done
    fi
    if [ ${#slint_guis[@]} -gt 0 ]; then
        step build-slint build_preset dev-slint
        for gui in "${slint_guis[@]}"; do
            step "gui-slint-$gui" slint_gui_tests "$gui"
        done
    fi
    ;;
full)
    server_up
    step fmt cargo fmt --all --check
    step clippy cargo clippy --workspace --all-targets --locked -- -D warnings
    step clippy-spix cargo clippy --workspace --all-targets --locked --features rc_slint/spix -- -D warnings
    step deny cargo deny check
    step test cargo test --workspace --locked
    step contract-api cargo test -p rc_api --test contract --locked -- --ignored --test-threads=6
    step contract-core cargo test -p rc_core --test contract --locked -- --ignored --test-threads=6
    step build build_preset dev
    step build-slint build_preset dev-slint
    step qmllint ninja -C build-dev all_qmllint
    step gui gui_tests --no-fail-fast
    for gui in $(slint_files); do
        step "gui-slint-$gui" slint_gui_tests "$gui"
    done
    ;;
*)
    sed -n '/^# Usage:/,/^# The contract/p' "$0" | sed 's/^# \{0,1\}//' | head -n -1
    exit 2
    ;;
esac

if [ ${#failed[@]} -gt 0 ]; then
    echo "failed: ${failed[*]}"
    exit 1
fi
echo "all passed"
