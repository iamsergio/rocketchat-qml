// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls

ApplicationWindow {
    id: root

    width: 640
    height: 480
    visible: true
    title: qsTr("RocketChatQml")

    Label {
        anchors.centerIn: parent
        text: qsTr("Hello, Rocket.Chat")
    }
}
