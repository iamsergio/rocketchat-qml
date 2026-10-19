# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

cmake_minimum_required(VERSION 3.21)
project(rocketchat_qml LANGUAGES CXX)

set(ROCKETCHATQML_VERSION "1.0.0")
option(ROCKETCHATQML_DEVELOPER_MODE "Build with developer options" OFF)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)

set(QT_QML_GENERATE_QMLLS_INI TRUE)

include(cmake/CompilerFlags.cmake)

add_subdirectory(src)
