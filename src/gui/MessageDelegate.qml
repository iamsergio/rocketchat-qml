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
    required property string avatar
    required property string avatarInitials
    required property string avatarColor
    required property string time
    required property string body
    required property string html
    required property bool mentionsMe
    required property bool isSystem
    required property string systemText
    required property bool firstOfDay
    required property string dayLabel
    required property string delivery
    // A list of maps: title, description, size, link, isImage, image (a file URL, empty until it is
    // downloaded), width and height.
    required property var attachments
    required property bool edited
    // A list of maps, in the server's order: emoji (the shortcode), glyph (Unicode text, or empty),
    // image (the URL of a custom emoji, or empty), count, mine (the user reacted) and users (who
    // reacted, for a tooltip).
    required property var reactions
    // Whether the user may change the message now: it is theirs, the server has it, and the
    // server's settings allow it.
    required property bool canEdit
    required property bool canDelete

    // The user chose to edit the message: the composer takes over.
    signal editRequested(string messageId)

    objectName: "msg_" + messageId
    width: ListView.view.width
    implicitHeight: column.implicitHeight + 8

    HoverHandler {
        id: rowHover
    }

    // Right-click opens the same menu as the button.
    MouseArea {
        acceptedButtons: Qt.RightButton
        anchors.fill: parent
        enabled: root.canEdit || root.canDelete
        onClicked: mouse => menu.popup(root, mouse.x, mouse.y)
    }

    // The popups are parented to the window's overlay while open, so Spix finds the items inside
    // them by name from the window (mainWindow/msgEditAction), and only while they are open.
    Menu {
        id: menu

        MenuItem {
            objectName: "msgEditAction"
            enabled: root.canEdit
            height: visible ? implicitHeight : 0
            text: qsTr("Edit")
            visible: root.canEdit
            onTriggered: root.editRequested(root.messageId)
        }

        MenuItem {
            objectName: "msgDeleteAction"
            enabled: root.canDelete
            height: visible ? implicitHeight : 0
            text: qsTr("Delete")
            visible: root.canDelete
            onTriggered: deleteDialog.open()
        }
    }

    Dialog {
        id: deleteDialog

        anchors.centerIn: Overlay.overlay
        modal: true
        parent: Overlay.overlay
        title: qsTr("Delete message")

        footer: DialogButtonBox {
            Button {
                objectName: "deleteConfirm"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                text: qsTr("Delete")
            }

            Button {
                objectName: "deleteCancel"
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                text: qsTr("Cancel")
            }
        }

        onAccepted: Room.remove(root.messageId)

        // Named, because Spix finds items and a Dialog is not one.
        contentItem: Label {
            objectName: "deleteDialog"
            text: qsTr("Delete this message for everyone? This can't be undone.")
            wrapMode: Text.Wrap
        }
    }

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
            spacing: 10

            Avatar {
                Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                url: root.avatar
                initials: root.avatarInitials
                fillColor: root.avatarColor
                imageName: "msgAvatar"
                placeholderName: "msgAvatarPlaceholder"
                initialsName: "msgAvatarInitials"
            }

            ColumnLayout {
                id: content

                Layout.fillWidth: true
                spacing: 2

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Label {
                        objectName: "msgAuthor"
                        font.bold: true
                        text: root.author
                        textFormat: Text.PlainText
                    }

                    Label {
                        id: timeLabel

                        objectName: "msgTime"
                        color: root.palette.placeholderText
                        text: root.time
                    }

                    Label {
                        objectName: "msgEdited"
                        color: root.palette.placeholderText
                        font.italic: true
                        font.pointSize: timeLabel.font.pointSize * 0.85
                        text: qsTr("(edited)")
                        visible: root.edited
                    }

                    Item {
                        Layout.fillWidth: true
                    }

                    // Shown when the pointer is over the message, if there is anything to do with it.
                    // It keeps its place the rest of the time, so the rows don't change height.
                    ToolButton {
                        objectName: "msgMenuButton"
                        Layout.preferredHeight: 22
                        Layout.preferredWidth: 28
                        opacity: rowHover.hovered || menu.opened ? 1 : 0
                        padding: 0
                        text: "\u22EF"
                        visible: root.canEdit || root.canDelete
                        onClicked: menu.popup(this, 0, height)
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
                    // A message of only files has no text to leave a blank line for.
                    visible: root.body !== "" || root.attachments.length === 0
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

                // The files of the message: images inline, other files as links. They open on the
                // server's page in the browser, as the app can't hand the browser its login.
                Repeater {
                    model: root.attachments

                    delegate: ColumnLayout {
                        id: attachment

                        required property int index
                        required property var modelData
                        // The image is shown at its size, at most this wide and tall.
                        readonly property real naturalWidth: modelData.width > 0 ? modelData.width : 360
                        readonly property real naturalHeight: modelData.height > 0 ? modelData.height : 240
                        readonly property real fit: Math.min(1, Math.min(360, content.width) / naturalWidth, 480 / naturalHeight)

                        objectName: "attachment" + index
                        Layout.fillWidth: true
                        spacing: 2

                        Item {
                            Layout.preferredWidth: attachment.naturalWidth * attachment.fit
                            Layout.preferredHeight: attachment.naturalHeight * attachment.fit
                            visible: attachment.modelData.isImage

                            Rectangle {
                                objectName: "attachmentPlaceholder"
                                anchors.fill: parent
                                color: root.palette.mid
                                opacity: 0.3
                                visible: picture.status !== Image.Ready
                            }

                            Image {
                                id: picture

                                // The URL, which is what the tests read: Spix reads text.
                                property string text: attachment.modelData.image

                                objectName: "attachmentImage"
                                anchors.fill: parent
                                asynchronous: true
                                fillMode: Image.PreserveAspectFit
                                source: attachment.modelData.image
                                sourceSize.width: width
                                visible: status === Image.Ready
                            }

                            TapHandler {
                                onTapped: Qt.openUrlExternally(attachment.modelData.link)
                            }

                            HoverHandler {
                                cursorShape: Qt.PointingHandCursor
                            }
                        }

                        RowLayout {
                            // The address the file opens, which is what the tests read: Spix reads text.
                            property string text: attachment.modelData.link

                            objectName: "attachmentLink"
                            Layout.fillWidth: true
                            visible: !attachment.modelData.isImage
                            spacing: 8

                            Label {
                                objectName: "attachmentTitle"
                                Layout.maximumWidth: content.width
                                color: root.palette.link
                                elide: Text.ElideMiddle
                                font.underline: true
                                text: attachment.modelData.title
                                textFormat: Text.PlainText
                            }

                            Label {
                                objectName: "attachmentSize"
                                color: root.palette.placeholderText
                                text: attachment.modelData.size
                                textFormat: Text.PlainText
                            }

                            TapHandler {
                                onTapped: Qt.openUrlExternally(attachment.modelData.link)
                            }

                            HoverHandler {
                                cursorShape: Qt.PointingHandCursor
                            }
                        }

                        Label {
                            objectName: "attachmentDescription"
                            Layout.fillWidth: true
                            text: attachment.modelData.description
                            textFormat: Text.PlainText
                            visible: attachment.modelData.description !== ""
                            wrapMode: Text.Wrap
                        }
                    }
                }

                // What users reacted with: a chip for each emoji, with its count, marked when the
                // user is one of them.
                Flow {
                    objectName: "msgReactions"
                    Layout.fillWidth: true
                    spacing: 6
                    visible: root.reactions.length > 0

                    Repeater {
                        model: root.reactions

                        delegate: Rectangle {
                            id: chip

                            required property int index
                            required property var modelData
                            // Whether the user reacted, which is what the tests read.
                            property bool checked: modelData.mine

                            objectName: "reaction" + index
                            border.color: modelData.mine ? root.palette.highlight : root.palette.mid
                            border.width: 1
                            color: modelData.mine ? Qt.alpha(root.palette.highlight, 0.2) : "transparent"
                            implicitHeight: 24
                            implicitWidth: chipRow.implicitWidth + 14
                            radius: height / 2

                            Row {
                                id: chipRow

                                anchors.centerIn: parent
                                spacing: 5

                                Item {
                                    // What is shown, which is what the tests read: the glyph, else the
                                    // address of the custom emoji's image, else the shortcode.
                                    property string text: chip.modelData.glyph !== "" ? chip.modelData.glyph : chip.modelData.image !== "" ? chip.modelData.image : chip.modelData.emoji

                                    objectName: "reactionEmoji"
                                    height: 18
                                    width: chip.modelData.image !== "" ? 18 : shortcode.implicitWidth

                                    Label {
                                        id: shortcode

                                        anchors.verticalCenter: parent.verticalCenter
                                        text: chip.modelData.glyph !== "" ? chip.modelData.glyph : chip.modelData.emoji
                                        textFormat: Text.PlainText
                                        visible: chip.modelData.image === ""
                                    }

                                    Image {
                                        anchors.fill: parent
                                        asynchronous: true
                                        fillMode: Image.PreserveAspectFit
                                        source: chip.modelData.image
                                        sourceSize.height: 18
                                        visible: chip.modelData.image !== ""
                                    }
                                }

                                Label {
                                    objectName: "reactionCount"
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: chip.modelData.count
                                }
                            }

                            HoverHandler {
                                id: hover
                            }

                            ToolTip.delay: 400
                            ToolTip.text: chip.modelData.users
                            ToolTip.visible: hover.hovered
                        }
                    }
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
