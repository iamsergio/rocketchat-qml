// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import RocketChat.Bridge

ApplicationWindow {
    id: root

    objectName: "mainWindow"
    width: 640
    height: 480
    visible: true
    title: qsTr("RocketChatQml")

    Label {
        objectName: "versionLabel"
        anchors.centerIn: parent
        text: qsTr("Rocket.Chat %1").arg(AppInfo.version)
    }
}
