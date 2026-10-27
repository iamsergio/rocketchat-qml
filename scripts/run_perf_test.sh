#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Runs the performance scenarios against the Qt app and the Slint app with each of its renderers
# and writes an HTML report.
# See docs/design/performance-tests.md.
#
# Usage:
#   scripts/run_perf_test.sh --list
#       Every scenario once per variant (qt.scroll_main_channel,
#       slint-femtovg.drag_main_channel, slint-skia-vulkan.drag_main_channel,
#       slint-software.drag_main_channel, ...).
#   scripts/run_perf_test.sh [options] [scenario...]
#       A full name (slint-software.scroll_main_channel) runs that one, slint.<scenario> runs it
#       in every Slint variant, a bare name in all variants, no name runs everything. Options:
#         --platform wayland|xcb|offscreen   wayland by default; offscreen is a smoke run only,
#                                            its numbers mean nothing
#         --runs <n>          runs per scenario and variant (default 1)
#         --qt <binary>       use this Qt app instead of build-perf/bin/rocketchat_qml (no build
#         --slint <binary>    of that app), likewise build-perf-slint/bin/rocketchat_slint, for
#                             every Slint variant
#         --no-build          don't configure or build anything
#         --record            sample the measured part with `perf record` (user space, frame
#                             pointer call graphs) and write <variant>.<scenario>.<run>.perf.data, .folded and
#                             .svg (a flame graph) instead of JSON files and a report; sampling
#                             skews the counters. Open the .perf.data in hotspot.
#         --save-baseline     afterwards make this run the baseline: target/perf/baseline becomes
#                             a symlink to the run's directory
#
# What the names select is built first: the `perf` preset for qt, `perf-slint` for any Slint
# variant, and rc_gui_perf in release mode. The JSON files and report.html go to
# target/perf/<UTC timestamp>-<platform>/; the report compares with target/perf/baseline if there
# is one. The apps run visibly, so run it on a display, with nothing else busy.
#
# The scenarios need the Rocket.Chat test server (scripts/run_test_server.sh), at
# RC_TEST_SERVER_URL (default http://127.0.0.1:3000). The apps never use the keyring.

set -u

cd "$(dirname "$0")/.." || exit 1

export RC_TEST_SERVER_URL=${RC_TEST_SERVER_URL:-http://127.0.0.1:3000}
export RC_SECRET_STORE=memory

PERF_ROOT=target/perf
RUNNER=target/release/rc_gui_perf

platform=wayland
runs=
build=1
save_baseline=0
record=0
qt_app=${RC_PERF_QT_APP:-}
slint_app=${RC_PERF_SLINT_APP:-}
names=()

die() {
    echo "run_perf_test.sh: $*" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case $1 in
    --list)
        cargo build -q --release -p rc_gui_perf || exit 1
        exec "$RUNNER" --list
        ;;
    --platform) platform=${2:?--platform needs a value}; shift ;;
    --runs) runs=${2:?--runs needs a value}; shift ;;
    --qt) qt_app=${2:?--qt needs a value}; shift ;;
    --slint) slint_app=${2:?--slint needs a value}; shift ;;
    --no-build) build=0 ;;
    --record) record=1 ;;
    --save-baseline) save_baseline=1 ;;
    -h | --help) sed -n '8,37p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) die "unknown option $1" ;;
    *) names+=("$1") ;;
    esac
    shift
done
[ "$record" = 1 ] && [ "$save_baseline" = 1 ] && die "--record writes no JSON, so it can't be a baseline"

case $platform in
wayland | xcb | offscreen) ;;
*) die "unknown platform $platform: wayland, xcb or offscreen" ;;
esac

# Which apps the names select: qt.* needs Qt, slint.* and slint-<renderer>.* Slint, a bare name or
# none both.
need_qt=0
need_slint=0
if [ ${#names[@]} -eq 0 ]; then
    need_qt=1
    need_slint=1
fi
for name in "${names[@]}"; do
    case $name in
    qt.*) need_qt=1 ;;
    slint.* | slint-*) need_slint=1 ;;
    *) need_qt=1; need_slint=1 ;;
    esac
done

# Configures a preset if its build directory isn't, then builds it.
build_app() {
    local preset=$1
    [ -f "build-$preset/CMakeCache.txt" ] || cmake --preset "$preset" || die "can't configure $preset"
    cmake --build "build-$preset" || die "can't build $preset"
}

if [ "$build" = 1 ]; then
    # An app given by --qt or --slint is used as it is.
    [ "$need_qt" = 1 ] && [ -z "$qt_app" ] && build_app perf
    [ "$need_slint" = 1 ] && [ -z "$slint_app" ] && build_app perf-slint
    cargo build -q --release -p rc_gui_perf || die "can't build rc_gui_perf"
fi
[ -x "$RUNNER" ] || die "$RUNNER is missing: run without --no-build"

[ "$need_qt" = 1 ] && qt_app=${qt_app:-$PWD/build-perf/bin/rocketchat_qml}
[ "$need_slint" = 1 ] && slint_app=${slint_app:-$PWD/build-perf-slint/bin/rocketchat_slint}

out=$PERF_ROOT/$(date -u +%Y%m%dT%H%M%SZ)-$platform
mkdir -p "$out"

args=(--platform "$platform" --out "$out")
[ "$record" = 1 ] && args+=(--record)
[ -n "$runs" ] && args+=(--runs "$runs")
[ -n "$qt_app" ] && args+=(--qt "$qt_app")
[ -n "$slint_app" ] && args+=(--slint "$slint_app")

"$RUNNER" "${args[@]}" "${names[@]}" || die "the run failed, what was written is in $out"

if [ "$record" = 1 ]; then
    # No JSON, so no report: the flame graphs are the result.
    echo "flame graphs:"
    ls "$PWD/$out"/*.svg
    echo "the .perf.data files in $PWD/$out open in hotspot"
    exit 0
fi

report_args=(report "$out")
[ -e "$PERF_ROOT/baseline" ] && report_args+=(--baseline "$PERF_ROOT/baseline")
"$RUNNER" "${report_args[@]}" || die "can't write the report"

if [ "$save_baseline" = 1 ]; then
    ln -sfn "$(basename "$out")" "$PERF_ROOT/baseline"
    echo "baseline: $PERF_ROOT/baseline -> $(basename "$out")"
fi
