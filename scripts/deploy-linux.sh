#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Builds a release build and installs it to /pub_data/installed/rocketchat_qml/ (the binary ends up in bin/).

set -e
cd "$(dirname "$0")/.."
prefix=/pub_data/installed/rocketchat_qml
cmake --preset=rel -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$prefix -DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON
cmake --build build-rel
cmake --install build-rel
