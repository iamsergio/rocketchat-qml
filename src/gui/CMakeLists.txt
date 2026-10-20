# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

qt_policy(SET QTP0001 NEW)

qt_add_executable(rocketchat_qml main.cpp)

set_target_properties(rocketchat_qml PROPERTIES AUTOMOC ON)

# Lets qmllint and qmlcachegen see the Rust QML module (RocketChat.Bridge).
get_target_property(rc_qt_export_dir rc_qt CXX_QT_EXPORT_DIR)

qt_add_qml_module(
  rocketchat_qml
  URI
  rocketchat
  VERSION
  1.0
  QML_FILES
  Avatar.qml
  Main.qml
  Composer.qml
  LoginPage.qml
  MessageDelegate.qml
  RoomDelegate.qml
  RoomPage.qml
  RoomView.qml
  StartupPage.qml
  RESOURCES
  icons/channel.svg
  icons/direct.svg
  icons/private.svg
  IMPORT_PATH
  ${rc_qt_export_dir}/qml_modules)

target_link_libraries(rocketchat_qml PRIVATE Qt6::Core Qt6::Gui Qt6::Quick Qt6::QuickControls2 rc_qt_qml_module)

# rc_core keeps the session in the Secret Service through the system libdbus (the Rust side builds
# against it with pkg-config but doesn't pass it on to this link). Link the very library QtDBus
# loads, so the process has only one libdbus.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  find_package(PkgConfig REQUIRED)
  pkg_check_modules(DBus1 REQUIRED IMPORTED_TARGET dbus-1)
  target_link_libraries(rocketchat_qml PRIVATE PkgConfig::DBus1)
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
