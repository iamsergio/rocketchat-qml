#!/bin/bash
# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

# Builds release builds of the Qt and the Slint app and installs them to
# /pub_data/installed/rocketchat_qml/ and /pub_data/installed/rocketchat_slint/ (the binaries end up in bin/).

set -e
cd "$(dirname "$0")/.."
prefix=/pub_data/installed/rocketchat_qml
cmake --preset=rel -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$prefix -DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON
cmake --build build-rel
cmake --install build-rel

prefix=/pub_data/installed/rocketchat_slint
cmake --preset=rel-slint -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$prefix
cmake --build build-rel-slint
cmake --install build-rel-slint
