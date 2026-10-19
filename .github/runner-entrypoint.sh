#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Entry point of the CI runner image (.github/Dockerfile): registers the runner with the repository,
# runs it, and shuts it down cleanly on SIGTERM/SIGINT.
#
# Environment:
#   RUNNER_REPO_URL  repository URL, e.g. https://github.com/owner/repo (required on first start)
#   RUNNER_TOKEN     registration token (required on first start; they expire after an hour)
#   RUNNER_NAME      runner name (default: rocketchat-qml-<hostname>)
#   RUNNER_LABELS    labels (default: self-hosted,linux,x64,rocketchat-qml)
#
# The registration lives in the container's filesystem, so restarting the same container (restart
# policy, reboot) reuses it and needs no token. A new container registers again with --replace.

set -euo pipefail

cd /opt/actions-runner

if [ ! -f .runner ]; then
    : "${RUNNER_REPO_URL:?RUNNER_REPO_URL is not set}"
    : "${RUNNER_TOKEN:?RUNNER_TOKEN is not set}"
    ./config.sh --unattended --replace \
        --url "$RUNNER_REPO_URL" \
        --token "$RUNNER_TOKEN" \
        --name "${RUNNER_NAME:-rocketchat-qml-$(hostname)}" \
        --labels "${RUNNER_LABELS:-self-hosted,linux,x64,rocketchat-qml}" \
        --work _work
fi
# Don't leave the token in this process's environment, where jobs could not see it anyway.
unset RUNNER_TOKEN

./run.sh &
pid=$!

# run.sh forwards the signal to the listener, which finishes the current job step cleanly.
shutdown() {
    kill -TERM "$pid" 2>/dev/null || true
}
trap shutdown TERM INT

# A trapped signal interrupts the first wait, the second one collects the real exit status.
rc=0
wait "$pid" || rc=$?
if kill -0 "$pid" 2>/dev/null; then
    wait "$pid" || rc=$?
fi
exit "$rc"
