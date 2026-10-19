# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

cmake_minimum_required(VERSION 3.24)

project(rocketchat_qml LANGUAGES CXX)

option(ROCKETCHATQML_DEVELOPER_MODE "Build with developer options" OFF)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)

set(QT_QML_GENERATE_QMLLS_INI TRUE)

include(cmake/CompilerFlags.cmake)

find_package(Qt6 6.12 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2)

foreach(submodule corrosion cxx-qt-cmake)
  if(NOT EXISTS ${PROJECT_SOURCE_DIR}/3rdparty/${submodule}/CMakeLists.txt)
    message(FATAL_ERROR "3rdparty/${submodule} is missing. Run: git submodule update --init")
  endif()
endforeach()

# Rust crates are built by Corrosion, through cxx-qt-cmake. Both come from 3rdparty/ instead of being
# downloaded at configure time: the first FetchContent declaration of a name wins over cxx-qt-cmake's.
include(FetchContent)
set(CMAKE_DISABLE_FIND_PACKAGE_Corrosion ON)
FetchContent_Declare(Corrosion SOURCE_DIR ${PROJECT_SOURCE_DIR}/3rdparty/corrosion)
FetchContent_Declare(CxxQt SOURCE_DIR ${PROJECT_SOURCE_DIR}/3rdparty/cxx-qt-cmake)
FetchContent_MakeAvailable(CxxQt)

cxx_qt_import_crate(
  MANIFEST_PATH
  Cargo.toml
  CRATES
  rc_qt
  LOCKED
  QT_MODULES
  Qt6::Core
  Qt6::Gui
  Qt6::Qml)
# Must match the QmlModule in crates/rc_qt/build.rs.
set(RC_QT_QML_URI RocketChat.Bridge)
cxx_qt_import_qml_module(rc_qt_qml_module URI ${RC_QT_QML_URI} SOURCE_CRATE rc_qt)
# Qt looks for a target named after the module's plugin and warns if there is none. The module is
# already linked through rc_qt_qml_module, so point the plugin name at it.
string(REPLACE "." "_" rc_qt_qml_plugin ${RC_QT_QML_URI})
add_library(${rc_qt_qml_plugin} ALIAS rc_qt_qml_module)

add_subdirectory(src)
