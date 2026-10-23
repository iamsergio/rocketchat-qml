#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# The checks of a change, with short output: one line per step, and for a failing step only the
# lines that say what failed. Each step's whole log is kept in target/check-logs/<step>.log.
#
# Usage:
#   scripts/check.sh quick [-p <crate>]... [-g <gui test file>]...
#       While working: fmt, then clippy and the unit tests of the given crates (the workspace if
#       none), then the given GUI test files (crates/rc_gui_tests/tests/<name>.rs) against the
#       app in build-dev, which is rebuilt first.
#   scripts/check.sh full
#       Before a commit, what CI runs on the dev preset: fmt, clippy (also with Slint's Spix),
#       cargo deny, the workspace's tests, the contract tests, the build, qmllint and every GUI
#       test, carrying on past a failing GUI test file.
#
# The contract and GUI tests need the Rocket.Chat test server (scripts/run_test_server.sh), at
# RC_TEST_SERVER_URL (default http://127.0.0.1:3000). The app under test never uses the keyring.

set -u

cd "$(dirname "$0")/.." || exit 1

export RC_TEST_SERVER_URL=${RC_TEST_SERVER_URL:-http://127.0.0.1:3000}
export RC_SECRET_STORE=memory
export RC_GUI_TEST_APP=${RC_GUI_TEST_APP:-$PWD/build-dev/bin/rocketchat_qml}

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

gui_tests() {
    cargo test -p rc_gui_tests --locked "$@" -- --ignored
}

mode=${1:-}
shift || true

case $mode in
quick)
    crates=()
    guis=()
    while [ $# -gt 0 ]; do
        case $1 in
        -p) crates+=(-p "$2"); shift 2 ;;
        -g) guis+=("$2"); shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
        esac
    done
    [ ${#crates[@]} -eq 0 ] && crates=(--workspace)
    step fmt cargo fmt --all --check
    step clippy cargo clippy "${crates[@]}" --all-targets --locked -- -D warnings
    step test cargo test "${crates[@]}" --locked
    if [ ${#guis[@]} -gt 0 ]; then
        server_up
        step build cmake --build build-dev
        for gui in "${guis[@]}"; do
            step "gui-$gui" gui_tests --test "$gui"
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
    step contract-api cargo test -p rc_api --test contract --locked -- --ignored
    step contract-core cargo test -p rc_core --test contract --locked -- --ignored
    step build cmake --build build-dev
    step qmllint ninja -C build-dev all_qmllint
    step gui gui_tests --no-fail-fast
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
