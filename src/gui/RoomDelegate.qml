// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One row of the room list. Named by room id: room names aren't unique.
ItemDelegate {
    id: root

    required property string roomId
    required property string name
    required property string kindMarker
    required property int unread
    required property int userMentions
    required property bool alert

    objectName: "room_" + roomId
    width: ListView.view.width

    contentItem: RowLayout {
        spacing: 12

        Label {
            objectName: "roomKind"
            Layout.preferredWidth: 24
            horizontalAlignment: Text.AlignHCenter
            text: root.kindMarker
        }

        Label {
            objectName: "roomName"
            Layout.fillWidth: true
            elide: Text.ElideRight
            font.bold: root.alert
            text: root.name
        }

        // Mentions of the user.
        Rectangle {
            visible: root.userMentions > 0
            implicitWidth: mentionRow.implicitWidth + 12
            implicitHeight: mentionRow.implicitHeight + 4
            radius: height / 2
            color: root.palette.highlight

            Row {
                id: mentionRow

                anchors.centerIn: parent

                Label {
                    text: "@"
                    color: root.palette.highlightedText
                }

                Label {
                    objectName: "roomMentions"
                    text: root.userMentions
                    color: root.palette.highlightedText
                }
            }
        }

        Rectangle {
            visible: root.unread > 0
            implicitWidth: Math.max(unreadLabel.implicitWidth + 12, height)
            implicitHeight: unreadLabel.implicitHeight + 4
            radius: height / 2
            color: root.palette.mid

            Label {
                id: unreadLabel

                objectName: "roomUnread"
                anchors.centerIn: parent
                text: root.unread
            }
        }
    }
}
