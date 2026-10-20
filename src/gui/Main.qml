// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import RocketChat.Bridge

ApplicationWindow {
    id: root

    objectName: "mainWindow"
    width: 900
    height: 600
    visible: true
    title: qsTr("RocketChatQml")

    // One page per session state. A Loader, so that a page starts empty each time it shows: the
    // login fields don't keep the password after a logout. While logging in the state is
    // Connecting, which keeps the login page and its fields.
    Loader {
        anchors.fill: parent
        source: {
            switch (Session.state) {
            case Session.LoggedIn:
                return "RoomPage.qml";
            case Session.Unknown:
                return "StartupPage.qml";
            default:
                return "LoginPage.qml";
            }
        }
    }

    footer: ToolBar {
        Label {
            objectName: "versionLabel"
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: 12
            text: qsTr("Rocket.Chat %1").arg(AppInfo.version)
        }
    }
}
