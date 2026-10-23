#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Builds and runs the Slint frontend inside a terminal (ratatui-slint). Usage: see `usage` below,
# or run it with --help.

set -euo pipefail

usage() {
    cat <<'EOF'
Builds and runs the Slint frontend inside a terminal (ratatui-slint). The terminal must speak
the kitty graphics protocol (kitty, WezTerm, Ghostty...).

Usage: scripts/run_tui.sh [--dev] [--quadrant] [--scale <factor>]
  --dev               fill the login page in for the local test server
  --quadrant          the app in the top-left corner (65% of the width and height), with the
                      keys and the log around it
  --scale <factor>    how many terminal pixels a logical pixel takes (default: the cell height
                      / 16; smaller is smaller, e.g. 1 or 0.8)
  -h, --help          show this and exit

Environment:
  RC_SECRET_STORE  where the session token goes (default memory: not the OS keyring)
  RC_TUI_LOG       the log file (default: next to the executable)
EOF
}

# --scale is ratatui-slint's RATATUI_SLINT_SCALE; the rest goes to the app.
args=()
while (($#)); do
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        --scale)
            [[ $# -ge 2 ]] || { echo "--scale needs a factor" >&2; exit 2; }
            export RATATUI_SLINT_SCALE="$2"
            shift 2
            ;;
        *)
            args+=("$1")
            shift
            ;;
    esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release -p rc_tui --features developer-mode

export RATATUI_SLINT_PROTOCOL=kitty
export RC_SECRET_STORE="${RC_SECRET_STORE:-memory}"

exec "$root/target/release/rocketchat_tui" "${args[@]}"
