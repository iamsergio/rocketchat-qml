// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

// One row of the room list. Named by room id: room names aren't unique.
ItemDelegate {
    id: root

    required property string roomId
    required property string name
    required property string kind
    required property int unread
    required property int userMentions
    required property bool alert

    objectName: "room_" + roomId
    width: ListView.view.width
    highlighted: Room.roomId === roomId
    onClicked: Room.open(roomId)

    contentItem: RowLayout {
        spacing: 12

        // The icon is named after the room kind: icons/channel.svg, private.svg, direct.svg. Hidden
        // until it has loaded, so that a missing SVG plugin shows in the GUI tests.
        Image {
            objectName: "roomKind_" + root.kind
            Layout.preferredWidth: 24
            fillMode: Image.Pad
            source: "icons/" + root.kind + ".svg"
            sourceSize: Qt.size(16, 16)
            visible: status === Image.Ready

            onStatusChanged: {
                if (status === Image.Error)
                    console.warn("Can't load the room icon", source, "(is the Qt SVG image plugin installed?)");
            }
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
