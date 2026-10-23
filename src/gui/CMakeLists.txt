# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

qt_policy(SET QTP0001 NEW)

qt_add_executable(rocketchat_qml main.cpp)

set_target_properties(rocketchat_qml PROPERTIES AUTOMOC ON)

# Lets qmllint and qmlcachegen see the Rust QML module (RocketChat.Bridge).
get_target_property(rc_qt_export_dir rc_qt CXX_QT_EXPORT_DIR)

set_source_files_properties(Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_qml_module(
  rocketchat_qml
  URI
  rocketchat
  VERSION
  1.0
  SOURCES
  clipboard.h
  clipboard.cpp
  debugtools.h
  debugtools.cpp
  QML_FILES
  Avatar.qml
  Main.qml
  Composer.qml
  EmojiPicker.qml
  FieldLabel.qml
  FlatButton.qml
  Icon.qml
  InputField.qml
  LinkArea.qml
  LinkMenu.qml
  LoginPage.qml
  MessageDelegate.qml
  PrimaryButton.qml
  RailTip.qml
  RoomDelegate.qml
  RoomPage.qml
  RoomView.qml
  ServerRail.qml
  ServerTile.qml
  StartupPage.qml
  ThreadListPage.qml
  ThreadListRow.qml
  ThreadPane.qml
  Theme.qml
  VersionLabel.qml
  RESOURCES
  fonts/InstrumentSans.ttf
  fonts/InstrumentSans-Italic.ttf
  icons/channel.svg
  icons/chevron.svg
  icons/check.svg
  icons/clock.svg
  icons/close.svg
  icons/corner.svg
  icons/direct.svg
  icons/file.svg
  icons/plus.svg
  icons/private.svg
  icons/search.svg
  icons/settings.svg
  icons/thread.svg
  IMPORT_PATH
  ${rc_qt_export_dir}/qml_modules)

target_link_libraries(rocketchat_qml PRIVATE Qt6::Core Qt6::Gui Qt6::Quick Qt6::QuickControls2 rc_qt_qml_module)
target_include_directories(rocketchat_qml PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})

# KDToolBox's dpiinfo.h asks Windows for the process' DPI awareness.
if(WIN32)
  target_link_libraries(rocketchat_qml PRIVATE Shcore)
endif()

# rc_core keeps the session in the Secret Service through the system libdbus (the Rust side builds
# against it with pkg-config but doesn't pass it on to this link). Link the very library QtDBus
# loads, so the process has only one libdbus.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  find_package(PkgConfig REQUIRED)
  pkg_check_modules(DBus1 REQUIRED IMPORTED_TARGET dbus-1)
  target_link_libraries(rocketchat_qml PRIVATE PkgConfig::DBus1)
endif()

if(ROCKETCHATQML_COVERAGE)
  # rustc links the profiler runtime (what writes the profile at exit) into executables and
  # dylibs, but not into a staticlib, so the instrumented rc_qt leaves its symbols undefined.
  # Link the toolchain's runtime, a plain archive inside an rlib, after the staticlib.
  get_target_property(rustc Rust::Rustc IMPORTED_LOCATION)
  execute_process(COMMAND ${rustc} --print sysroot OUTPUT_VARIABLE rust_sysroot OUTPUT_STRIP_TRAILING_WHITESPACE COMMAND_ERROR_IS_FATAL ANY)
  file(GLOB rust_profiler_runtime ${rust_sysroot}/lib/rustlib/*/lib/libprofiler_builtins-*.rlib)
  list(LENGTH rust_profiler_runtime rust_profiler_runtime_count)
  if(NOT rust_profiler_runtime_count EQUAL 1)
    message(FATAL_ERROR "Expected one profiler_builtins rlib in ${rust_sysroot}, found: ${rust_profiler_runtime}")
  endif()
  # Nothing in the staticlib refers to the runtime, so ask for the member that registers the
  # write at exit.
  target_link_libraries(rocketchat_qml PRIVATE -Wl,-u,__llvm_profile_runtime ${rust_profiler_runtime})
endif()

if(ROCKETCHATQML_SPIX)
  target_link_libraries(rocketchat_qml PRIVATE Spix::QtQuick)
  target_compile_definitions(rocketchat_qml PRIVATE ROCKETCHATQML_SPIX)
endif()

if(ROCKETCHATQML_DEVELOPER_MODE)
  target_compile_definitions(rocketchat_qml PRIVATE QT_QML_DEBUG ROCKETCHATQML_DEVELOPER_MODE)
endif()

rocketchatqml_set_compiler_flags(rocketchat_qml ${ROCKETCHATQML_DEVELOPER_MODE})

install(TARGETS rocketchat_qml RUNTIME DESTINATION bin)
