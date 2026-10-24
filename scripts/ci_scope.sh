#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Which of CI's checks a change needs, from the paths it touches. Reads the changed paths on stdin,
# one per line, and prints one `<scope>=true|false` line per scope, the format of $GITHUB_OUTPUT:
#   rust      clippy, cargo deny and the workspace's tests
#   contract  the contract tests against the test server
#   qt        the Qt app's build, qmllint and its GUI tests
#   slint     the Slint app's build and the GUI tests of slint.txt
# Formatting and REUSE always run: they take a second.
#
# A path it doesn't know needs everything, and so does no path at all: a run with nothing to compare
# against (workflow_dispatch, the nightly run) checks everything. The first pattern that matches a
# path decides. Tested by scripts/ci_scope_test.sh.

set -u

rust=false
contract=false
qt=false
slint=false

everything() {
    rust=true
    contract=true
    qt=true
    slint=true
}

any=false
while IFS= read -r path; do
    [ -n "$path" ] || continue
    any=true
    case $path in
    # INFRA-005, a unit test, compares these two; slint.txt also says which Slint GUI tests run.
    TRACEABILITY.md) rust=true ;;
    crates/rc_gui_tests/slint.txt) rust=true slint=true ;;
    # The classifier decides what the rest of the run checks, so a change to it checks everything.
    scripts/ci_scope.sh | scripts/ci_scope_test.sh) everything ;;
    # Read by no build and no test. CI runs none of the scripts but the two above.
    *.md | docs/* | LICENSES/* | LICENSE | REUSE.toml | .claude/* | scripts/* | \
        .github/dependabot.yml | .gitignore | .mcp.json | code.code-workspace | \
        .clang-format | .clang-tidy | .clangd | .cmake-format.yaml) ;;
    # The Slint app takes the fonts and icons from here too.
    src/gui/fonts/* | src/gui/icons/*) qt=true slint=true ;;
    src/* | .qmllint.ini) qt=true ;;
    crates/rc_qt/*) rust=true qt=true ;;
    crates/rc_slint/*) rust=true slint=true ;;
    crates/rc_tui/*) rust=true ;;
    crates/rc_view/* | crates/rc_gui_tests/*) rust=true qt=true slint=true ;;
    # rc_api, rc_core and rc_test_server, the build, the toolchain, the submodules, the workflows.
    *) everything ;;
    esac
done
$any || everything

echo "rust=$rust"
echo "contract=$contract"
echo "qt=$qt"
echo "slint=$slint"
