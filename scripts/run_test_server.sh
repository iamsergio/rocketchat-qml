#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Local Rocket.Chat instance for development and contract tests.
# Mirrors the official setup (github.com/RocketChat/rocketchat-compose):
# Rocket.Chat + MongoDB single-node replica set + NATS, on one container network.
#
# Usage: scripts/run_test_server.sh [start|fresh|stop|reset|status|logs]
#   start   create/start the containers and wait until the server answers (default)
#   fresh   reset, then start: a clean server with only the admin user
#   stop    stop the containers, keep the data
#   reset   remove containers and data
#   status  show container state
#   logs [rocketchat|mongo|nats]  follow a container's log (default rocketchat)
#
# Environment:
#   RC_ENGINE      docker (default) or podman
#   RC_RELEASE     Rocket.Chat image tag (default 8.0.1). Match the production server.
#   RC_PORT        host port, bound to 127.0.0.1 (default 3000)
#   RC_ADMIN_USER  / RC_ADMIN_PASS  admin created on an empty database (default admin / admin).
#                  Changing them later needs 'fresh'.
#   MONGODB_TAG    mongodb/mongodb-community-server image tag
#   NATS_TAG       nats image tag
#
# Image tags are exact builds at least two weeks old; floating tags (7.0-ubi8) are rebuilt daily.

set -euo pipefail

ENGINE=${RC_ENGINE:-docker}
RC_RELEASE=${RC_RELEASE:-8.0.1}
RC_PORT=${RC_PORT:-3000}
RC_ADMIN_USER=${RC_ADMIN_USER:-admin}
RC_ADMIN_PASS=${RC_ADMIN_PASS:-admin}
# Not 8.x: its tcmalloc breaks on Linux >= 6.19 and the image refuses to start (SERVER-121912).
# Rocket.Chat 8 accepts 7.0 with a deprecation warning. Rocket.Chat 9 will require 8.x.
MONGODB_TAG=${MONGODB_TAG:-7.0.43-ubi8-20261005T064351Z}
NATS_TAG=${NATS_TAG:-2.11.17-alpine}

PREFIX=rc-test
NETWORK=$PREFIX
VOLUME=$PREFIX-mongo
MONGO=$PREFIX-mongo
NATS=$PREFIX-nats
RC=$PREFIX-rocketchat
URL=http://127.0.0.1:$RC_PORT

e() { "$ENGINE" "$@"; }

exists() { e container inspect "$1" >/dev/null 2>&1; }

fail_with_log() {
    echo "$1. Last lines of the $2 log:" >&2
    e logs --tail 30 "$2" >&2 || true
    exit 1
}

# wait_for <what> <timeout> <container> <cmd...>: retries cmd until it succeeds. Fails early if the
# container stops.
wait_for() {
    local what=$1 timeout=$2 container=$3
    shift 3
    local start=$SECONDS
    until "$@" >/dev/null 2>&1; do
        if [ "$(e container inspect -f '{{.State.Running}}' "$container")" != true ]; then
            fail_with_log "$container stopped while waiting for $what" "$container"
        fi
        if ((SECONDS - start > timeout)); then
            fail_with_log "Timed out after ${timeout}s waiting for $what" "$container"
        fi
        sleep 2
    done
}

# ensure <name> <run args...>: starts the container, creating it if missing. The run args are hashed
# into a label; when they change (new RC_RELEASE, new setting...) the container is recreated so the
# change takes effect. Data lives in the volume, so recreating a container doesn't lose it.
ensure() {
    local name=$1
    shift
    local hash
    hash=$(printf '%s\0' "$@" | sha256sum | cut -c1-16)
    if exists "$name" &&
        [ "$(e container inspect -f '{{index .Config.Labels "rc-test.config"}}' "$name")" != "$hash" ]; then
        echo "  $name configuration changed, recreating it"
        e rm -f "$name" >/dev/null
    fi
    if exists "$name"; then
        e start "$name" >/dev/null
    else
        e run -d --name "$name" --network $NETWORK --label rc-test.config="$hash" "$@" >/dev/null
    fi
}

start_mongo() {
    local image=docker.io/mongodb/mongodb-community-server:"$MONGODB_TAG"
    # Runs as the image's own user: an empty named volume gets the image's /data/db, owner included.
    ensure $MONGO -v $VOLUME:/data/db "$image" mongod --replSet rs0 --bind_ip_all
    wait_for MongoDB 120 $MONGO e exec $MONGO mongosh --quiet --eval 'db.adminCommand("ping").ok'
    e exec $MONGO mongosh --quiet --eval \
        "try { rs.status().ok } catch (e) { rs.initiate({_id: 'rs0', members: [{_id: 0, host: '$MONGO:27017'}]}).ok }" >/dev/null
    wait_for "MongoDB primary" 60 $MONGO e exec $MONGO mongosh --quiet --eval \
        'if (!db.hello().isWritablePrimary) quit(1)'
}

start_nats() {
    ensure $NATS docker.io/nats:"$NATS_TAG"
}

start_rocketchat() {
    # OVERWRITE_SETTING_* are reapplied on every start. They skip the setup wizard and turn off
    # features that get in the way of automated tests (email 2FA, rate limiters). Tests log in with
    # wrong passwords on purpose, and Rocket.Chat would otherwise block the user and the IP.
    # ADMIN_* only take effect while no admin exists, i.e. on an empty database.
    ensure $RC \
        -p 127.0.0.1:"$RC_PORT":3000 \
        -e ROOT_URL="$URL" \
        -e PORT=3000 \
        -e DEPLOY_METHOD=docker \
        -e MONGO_URL="mongodb://$MONGO:27017/rocketchat?replicaSet=rs0" \
        -e TRANSPORTER="monolith+nats://$NATS:4222" \
        -e ADMIN_USERNAME="$RC_ADMIN_USER" \
        -e ADMIN_PASS="$RC_ADMIN_PASS" \
        -e ADMIN_EMAIL="$RC_ADMIN_USER@example.com" \
        -e OVERWRITE_SETTING_Show_Setup_Wizard=completed \
        -e OVERWRITE_SETTING_Accounts_TwoFactorAuthentication_By_Email_Enabled=false \
        -e OVERWRITE_SETTING_API_Enable_Rate_Limiter=false \
        -e OVERWRITE_SETTING_DDP_Rate_Limit_IP_Enabled=false \
        -e OVERWRITE_SETTING_DDP_Rate_Limit_User_Enabled=false \
        -e OVERWRITE_SETTING_DDP_Rate_Limit_Connection_Enabled=false \
        -e OVERWRITE_SETTING_DDP_Rate_Limit_User_By_Method_Enabled=false \
        -e OVERWRITE_SETTING_DDP_Rate_Limit_Connection_By_Method_Enabled=false \
        -e OVERWRITE_SETTING_Block_Multiple_Failed_Logins_Enabled=false \
        registry.rocket.chat/rocketchat/rocket.chat:"$RC_RELEASE"
}

cmd_start() {
    e network inspect $NETWORK >/dev/null 2>&1 || e network create $NETWORK >/dev/null
    echo "Starting MongoDB..."
    start_mongo
    echo "Starting NATS..."
    start_nats
    echo "Starting Rocket.Chat $RC_RELEASE (the first start takes a few minutes)..."
    start_rocketchat
    wait_for "Rocket.Chat at $URL" 600 $RC curl -fsS --max-time 10 "$URL/api/info"
    wait_for "admin login" 60 $RC curl -fsS --max-time 10 -H 'Content-Type: application/json' \
        -d "{\"user\":\"$RC_ADMIN_USER\",\"password\":\"$RC_ADMIN_PASS\"}" "$URL/api/v1/login"
    echo "Rocket.Chat is up at $URL (admin: $RC_ADMIN_USER / $RC_ADMIN_PASS)"
}

cmd_stop() {
    for c in $RC $NATS $MONGO; do
        if exists "$c"; then e stop "$c" >/dev/null; fi
    done
    echo "Stopped (data kept, '$0 reset' to wipe)"
}

cmd_reset() {
    for c in $RC $NATS $MONGO; do
        if exists "$c"; then e rm -f "$c" >/dev/null; fi
    done
    e volume rm -f $VOLUME >/dev/null 2>&1 || true
    e network rm $NETWORK >/dev/null 2>&1 || true
    echo "Removed containers and data"
}

cmd_status() {
    e ps -a --filter name=$PREFIX- --format '{{.Names}}\t{{.Status}}'
}

case ${1:-start} in
start) cmd_start ;;
fresh)
    cmd_reset
    cmd_start
    ;;
stop) cmd_stop ;;
reset) cmd_reset ;;
status) cmd_status ;;
logs) e logs -f "$PREFIX-${2:-rocketchat}" ;;
help | -h | --help) echo "Usage: $0 [start|fresh|stop|reset|status|logs [rocketchat|mongo|nats]]" ;;
*)
    echo "Usage: $0 [start|fresh|stop|reset|status|logs [rocketchat|mongo|nats]]" >&2
    exit 1
    ;;
esac
