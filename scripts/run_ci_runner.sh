#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Self-hosted GitHub Actions runner for this repository's CI (.github/workflows/build.yml).
# Builds .github/Dockerfile and runs it as a container that registers itself with the repository.
# Run it on the host, not inside a container.
#
# Usage: scripts/run_ci_runner.sh [start|stop|logs|diag|status]
#   start   build the image, fetch a fresh registration token and (re)start the runner, detached
#           (default). The runner restarts with the Docker daemon, which needs no new token: the
#           registration lives in the container.
#   stop    stop the runner container, keep it and the caches
#   logs    follow the runner's log
#   diag    follow the latest job's detailed runner log (repo, ref, commit, each step)
#   status  show the container state
#
# Needs the GitHub CLI (gh), logged in as someone with admin rights on the repository.
#
# Environment:
#   RC_ENGINE  docker (default) or podman
#   RC_REPO    owner/repo to register with (default: taken from the 'origin' git remote)
#
# Caches live in named volumes (rc-ci-*) and survive 'start': Cargo registry and git checkouts,
# rustup toolchains, vcpkg downloads and binary cache, and the runner's work directory.
# 'docker volume rm' them to start cold.

set -euo pipefail

ENGINE=${RC_ENGINE:-docker}

IMAGE=rc-ci-runner
CONTAINER=rc-ci-runner
LABELS=self-hosted,linux,x64,rocketchat-qml

cd "$(dirname "$0")/.."

e() { "$ENGINE" "$@"; }

die() {
    echo "Error: $*" >&2
    exit 1
}

refuse_inside_container() {
    if [ -e /.dockerenv ] || [ -e /run/.containerenv ]; then
        die "this is running inside a container; run it on the host"
    fi
    if command -v systemd-detect-virt >/dev/null 2>&1 && systemd-detect-virt --container --quiet; then
        die "this is running inside a container ($(systemd-detect-virt --container)); run it on the host"
    fi
}

# Prints owner/repo from RC_REPO or the origin remote (git@github.com:o/r.git, https://github.com/o/r).
repo_slug() {
    if [ -n "${RC_REPO:-}" ]; then
        echo "$RC_REPO"
        return
    fi
    local url slug
    url=$(git remote get-url origin) || die "no 'origin' git remote; set RC_REPO=owner/repo"
    slug=$(printf '%s\n' "$url" | sed -E 's#^(git@|ssh://git@|https?://)[^/:]+[:/]##; s#(\.git)?/?$##')
    [[ $slug =~ ^[^/]+/[^/]+$ ]] || die "cannot get owner/repo from remote '$url'; set RC_REPO=owner/repo"
    echo "$slug"
}

cmd_start() {
    command -v gh >/dev/null 2>&1 || die "the GitHub CLI (gh) is needed to get a registration token"
    local slug token
    slug=$(repo_slug)

    echo "Building the image..."
    e build -t $IMAGE -f .github/Dockerfile .github

    # Tokens expire after an hour and are only used for the first registration.
    echo "Requesting a registration token for $slug..."
    token=$(gh api -X POST "repos/$slug/actions/runners/registration-token" --jq .token)

    if e container inspect $CONTAINER >/dev/null 2>&1; then
        e rm -f $CONTAINER >/dev/null
    fi
    # The token goes through the environment, not the command line: 'ps' and 'inspect' of the
    # container still show it, but it is useless after an hour and after registering.
    e run -d --name $CONTAINER \
        --restart unless-stopped \
        --stop-timeout 60 \
        -e RUNNER_REPO_URL="https://github.com/$slug" \
        -e RUNNER_TOKEN="$token" \
        -e RUNNER_NAME="rocketchat-qml-$(uname -n)" \
        -e RUNNER_LABELS=$LABELS \
        -v rc-ci-cargo-registry:/home/runner/.cargo/registry \
        -v rc-ci-cargo-git:/home/runner/.cargo/git \
        -v rc-ci-rustup:/home/runner/.rustup \
        -v rc-ci-vcpkg:/home/runner/.cache/vcpkg \
        -v rc-ci-work:/opt/actions-runner/_work \
        $IMAGE >/dev/null
    echo "Started. Check 'https://github.com/$slug/settings/actions/runners' or '$0 logs'."
}

cmd_stop() {
    if e container inspect $CONTAINER >/dev/null 2>&1; then
        e stop $CONTAINER >/dev/null
        echo "Stopped (the runner shows as offline on GitHub until the next start)"
    else
        echo "Not running"
    fi
}

usage="Usage: $0 [start|stop|logs|diag|status]"

refuse_inside_container

case ${1:-start} in
start) cmd_start ;;
stop) cmd_stop ;;
logs) e logs -f $CONTAINER ;;
diag)
    # The runner writes one Worker_*.log per job under _diag (the container's working directory).
    e exec $CONTAINER bash -c 'f=$(ls -t _diag/Worker_*.log 2>/dev/null | head -n 1)
        [ -n "$f" ] || { echo "No job has run since this container started" >&2; exit 1; }
        echo "== $f"; exec tail -n +1 -f "$f"'
    ;;
status) e ps -a --filter name=$CONTAINER --format '{{.Names}}\t{{.Status}}' ;;
help | -h | --help) echo "$usage" ;;
*)
    echo "$usage" >&2
    exit 1
    ;;
esac
