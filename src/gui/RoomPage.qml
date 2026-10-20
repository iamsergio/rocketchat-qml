// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

ColumnLayout {
    spacing: 0

    ToolBar {
        Layout.fillWidth: true

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12

            Label {
                objectName: "currentUser"
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: qsTr("%1 on %2").arg(Session.username).arg(Session.server)
            }

            ToolButton {
                objectName: "logoutButton"
                text: qsTr("Log out")
                onClicked: Session.logout()
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 0

        ListView {
            objectName: "roomList"
            Layout.preferredWidth: 260
            Layout.fillHeight: true
            clip: true
            model: RoomList
            delegate: RoomDelegate {}
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: 1
            color: palette.mid
        }

        RoomView {
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
    }
}
