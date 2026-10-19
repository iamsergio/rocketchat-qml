# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

find_package(Qt6 REQUIRED COMPONENTS Core Gui Quick QuickControls2)

qt_policy(SET QTP0001 NEW)

qt_add_executable(rocketchat_qml main.cpp)

set_target_properties(rocketchat_qml PROPERTIES AUTOMOC ON)

qt_add_qml_module(
  rocketchat_qml
  URI
  rocketchat
  VERSION
  1.0
  QML_FILES
  Main.qml)

target_link_libraries(rocketchat_qml PRIVATE Qt6::Core Qt6::Gui Qt6::Quick Qt6::QuickControls2)

if(ROCKETCHATQML_DEVELOPER_MODE)
  target_compile_definitions(rocketchat_qml PRIVATE QT_QML_DEBUG ROCKETCHATQML_DEVELOPER_MODE)
endif()

rocketchatqml_set_compiler_flags(rocketchat_qml ${ROCKETCHATQML_DEVELOPER_MODE})
