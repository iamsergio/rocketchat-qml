// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

// One message of the open room. Named by message id, as the room rows are by room id. Its parts
// are named like the model roles.
Item {
    id: root

    required property string messageId
    required property string author
    required property string time
    required property string body
    required property string html
    required property bool mentionsMe
    required property bool isSystem
    required property string systemText
    required property bool firstOfDay
    required property string dayLabel
    required property string delivery

    objectName: "msg_" + messageId
    width: ListView.view.width
    implicitHeight: column.implicitHeight + 8

    // Marks a message that mentions the user.
    Rectangle {
        objectName: "msgMentionsMe"
        anchors.fill: parent
        visible: root.mentionsMe
        color: root.palette.highlight
        opacity: 0.18
    }

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

            // These exist only while the message is in that state, so that a test can find the one
            // unsent message without knowing its id: the client picks it.
            Loader {
                active: root.delivery === "pending"

                sourceComponent: Label {
                    objectName: "msgPending"
                    color: root.palette.placeholderText
                    font.italic: true
                    text: qsTr("Sending…")
                }
            }

            Loader {
                active: root.delivery === "failed"

                sourceComponent: Label {
                    objectName: "msgFailed"
                    color: "#c62828"
                    text: qsTr("Not sent")
                }
            }

            Loader {
                active: root.delivery === "failed"

                sourceComponent: ToolButton {
                    objectName: "msgRetry"
                    text: qsTr("Retry")
                    onClicked: Room.retry(root.messageId)
                }
            }
        }

        // `text` is the message as written, which the tests read; `msgBodyHtml` is what is shown.
        Item {
            id: bodyItem

            objectName: "msgBody"
            Layout.fillWidth: true
            implicitHeight: bodyText.implicitHeight
            visible: !root.isSystem
            opacity: root.delivery === "sent" ? 1 : 0.6
            property string text: root.body

            Label {
                id: bodyText

                objectName: "msgBodyHtml"
                width: parent.width
                text: root.html
                textFormat: Text.RichText
                wrapMode: Text.Wrap
                linkColor: root.palette.link
                onLinkActivated: link => Qt.openUrlExternally(link)

                HoverHandler {
                    cursorShape: bodyText.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                }
            }
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
