#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# INFRA-006
# Tests scripts/ci_scope.sh: `expect "<scopes that are true>" <changed path>...`.

set -u

cd "$(dirname "$0")/.." || exit 1

failures=0

expect() {
    local want=$1
    shift
    local got
    got=$(printf '%s\n' "$@" | scripts/ci_scope.sh | sed -n 's/=true$//p' | xargs)
    if [ "$got" != "$want" ]; then
        echo "FAIL  $*: want \"$want\", got \"$got\""
        failures=$((failures + 1))
    fi
}

all="rust contract qt slint"

# Nothing to compare against.
expect "$all"

# Docs and tools.
expect "" PLAN.md docs/design/threads.md .claude/skills/handoff/SKILL.md LICENSES/MIT.txt
expect "" scripts/check.sh .github/dependabot.yml crates/rc_qt/README.md
expect "rust" TRACEABILITY.md
expect "rust slint" crates/rc_gui_tests/slint.txt
expect "$all" scripts/ci_scope.sh

# One frontend.
expect "qt" src/gui/RoomView.qml
expect "qt" src/gui/main.cpp .qmllint.ini
expect "qt slint" src/gui/icons/plus.svg
expect "rust qt" crates/rc_qt/src/room_list.rs
expect "rust slint" crates/rc_slint/ui/main.slint
expect "rust" crates/rc_tui/src/main.rs
expect "rust" crates/rc_gui_perf/src/main.rs

# Both frontends.
expect "rust qt slint" crates/rc_view/src/room_view.rs
expect "rust qt slint" crates/rc_gui_tests/tests/messages.rs
expect "rust qt slint" src/gui/RoomView.qml crates/rc_slint/ui/main.slint

# Everything.
expect "$all" crates/rc_core/src/client.rs
expect "$all" crates/rc_api/tests/contract.rs
expect "$all" crates/rc_test_server/src/lib.rs
expect "$all" Cargo.lock
expect "$all" CMakeLists.txt
expect "$all" .github/workflows/build.yml
expect "$all" version.txt
expect "$all" 3rdparty/spix
expect "$all" a/path/nobody/knows
expect "$all" PLAN.md crates/rc_core/src/client.rs

if [ $failures -gt 0 ]; then
    echo "$failures failed"
    exit 1
fi
echo "all passed"
