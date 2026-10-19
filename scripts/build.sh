#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

set -e
cd "$(dirname "$0")/.."
cmake --preset=dev
cmake --build build-dev
