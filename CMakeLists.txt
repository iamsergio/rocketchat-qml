# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

cmake_minimum_required(VERSION 3.24)

# Before project(): that's where the vcpkg toolchain installs the manifest's features.
option(ROCKETCHATQML_SPIX "Build the Spix UI test server into the app, for the GUI tests" OFF)
if(ROCKETCHATQML_SPIX)
  list(APPEND VCPKG_MANIFEST_FEATURES spix)
endif()

project(rocketchat_qml LANGUAGES CXX)

option(ROCKETCHATQML_COVERAGE "Instrument the app's Rust code for coverage (scripts/coverage.sh)" OFF)

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

set(submodules corrosion cxx-qt-cmake)
if(ROCKETCHATQML_SPIX)
  list(APPEND submodules spix)
endif()
foreach(submodule ${submodules})
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

# test-file-store keeps the session token in a plain file (RC_SECRET_STORE=file:<path>), for the GUI
# tests that restart the app. Never in a build without Spix.
set(rc_qt_feature_list)
if(ROCKETCHATQML_DEVELOPER_MODE)
  list(APPEND rc_qt_feature_list developer-mode)
endif()
if(ROCKETCHATQML_SPIX)
  list(APPEND rc_qt_feature_list test-file-store)
endif()
set(rc_qt_features)
if(rc_qt_feature_list)
  set(rc_qt_features FEATURES ${rc_qt_feature_list})
endif()
cxx_qt_import_crate(
  MANIFEST_PATH
  Cargo.toml
  CRATES
  rc_qt
  LOCKED
  ${rc_qt_features}
  QT_MODULES
  Qt6::Core
  Qt6::Gui
  Qt6::Qml)
if(ROCKETCHATQML_COVERAGE)
  # For rc_qt and every crate it builds. The profiler runtime is linked in src/gui/CMakeLists.txt.
  # The app writes its profile (LLVM_PROFILE_FILE) when it exits normally.
  corrosion_add_target_rustflags(rc_qt -Cinstrument-coverage)
endif()
# Must match the QmlModule in crates/rc_qt/build.rs.
set(RC_QT_QML_URI RocketChat.Bridge)
cxx_qt_import_qml_module(rc_qt_qml_module URI ${RC_QT_QML_URI} SOURCE_CRATE rc_qt)
# Qt looks for a target named after the module's plugin and warns if there is none. The module is
# already linked through rc_qt_qml_module, so point the plugin name at it.
string(REPLACE "." "_" rc_qt_qml_plugin ${RC_QT_QML_URI})
add_library(${rc_qt_qml_plugin} ALIAS rc_qt_qml_module)

if(ROCKETCHATQML_SPIX)
  # Only the Qt Quick scene. Spix finds AnyRPC through vcpkg.
  set(SPIX_BUILD_EXAMPLES OFF)
  set(SPIX_BUILD_TESTS OFF)
  set(SPIX_BUILD_QTQUICK ON)
  set(SPIX_BUILD_QTWIDGETS OFF)
  set(SPIX_BUILD_SLINT OFF)
  add_subdirectory(3rdparty/spix)
endif()

add_subdirectory(src)

# Tests run through cargo, from the source tree, into Cargo's own target directory. Like a plain
# cargo test, rc_qt's build script finds Qt's qmake on PATH. Rust::Cargo is the toolchain's own cargo,
# not rustup's proxy, so it is told which rustc to use, as Corrosion does for builds: otherwise
# rustup's rustc proxy picks a toolchain from the directory of each crate it compiles, and fails for
# crates outside this tree when there is no default toolchain.
enable_testing()
set(cargo_test_env "CARGO_BUILD_RUSTC=$<TARGET_FILE:Rust::Rustc>")
add_test(
  NAME rust_tests
  COMMAND $<TARGET_FILE:Rust::Cargo> test --workspace --locked
  WORKING_DIRECTORY ${PROJECT_SOURCE_DIR})
set_tests_properties(rust_tests PROPERTIES ENVIRONMENT "${cargo_test_env}")
if(ROCKETCHATQML_SPIX)
  # The GUI tests are #[ignore]d so that plain cargo test, which has no app, skips them.
  add_test(
    NAME gui_tests
    COMMAND $<TARGET_FILE:Rust::Cargo> test --package rc_gui_tests --locked -- --ignored
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR})
  set_tests_properties(gui_tests PROPERTIES ENVIRONMENT "${cargo_test_env};RC_GUI_TEST_APP=$<TARGET_FILE:rocketchat_qml>")
endif()
