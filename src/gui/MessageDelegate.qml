// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One message of the open room. Named by message id, as the room rows are by room id. Its parts
// are named like the model roles.
Item {
    id: root

    required property string messageId
    required property string author
    required property string time
    required property string body
    required property bool isSystem
    required property string systemText
    required property bool firstOfDay
    required property string dayLabel

    objectName: "msg_" + messageId
    width: ListView.view.width
    implicitHeight: column.implicitHeight + 8

    ColumnLayout {
        id: column

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2

        // A heading above the first message of each day.
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 6
            visible: root.firstOfDay
            spacing: 8

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 1
                color: root.palette.mid
            }

            Label {
                objectName: "msgDaySeparator"
                text: root.dayLabel
                color: root.palette.placeholderText
                font.bold: true
            }

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 1
                color: root.palette.mid
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: !root.isSystem
            spacing: 8

            Label {
                objectName: "msgAuthor"
                font.bold: true
                text: root.author
                textFormat: Text.PlainText
            }

            Label {
                objectName: "msgTime"
                color: root.palette.placeholderText
                text: root.time
            }
        }

        Label {
            objectName: "msgBody"
            Layout.fillWidth: true
            visible: !root.isSystem
            text: root.body
            // Messages are plain text for now; never let Qt guess that one is HTML.
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.isSystem
            spacing: 8

            Label {
                objectName: "msgSystem"
                Layout.fillWidth: true
                color: root.palette.placeholderText
                font.italic: true
                text: root.systemText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            Label {
                color: root.palette.placeholderText
                text: root.time
            }
        }
    }
}
