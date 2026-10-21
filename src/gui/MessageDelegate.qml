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
    // Whether the message can be quoted: it is text and the server has it, whoever wrote it.
    required property bool canQuote
    // Whether the user can react to the message: it is text and the server has it, whoever wrote
    // it.
    required property bool canReact
    // The messages this one quotes, in order: a list of maps with author (the username), bodyHtml,
    // body (as written) and time (empty if unknown).
    required property var quotes

    // The user chose to edit the message: the composer takes over.
    signal editRequested(string messageId)
    // The user chose to quote the message: the composer takes over.
    signal quoteRequested(string messageId)
    // The user wants to add a reaction to the message: the emoji picker opens.
    signal reactRequested(string messageId)

    objectName: "msg_" + messageId
    width: ListView.view.width
    implicitHeight: column.implicitHeight + Theme.spacingL

    HoverHandler {
        id: rowHover
    }

    // Right-click opens the same menu as the button.
    MouseArea {
        acceptedButtons: Qt.RightButton
        anchors.fill: parent
        enabled: root.canQuote || root.canReact || root.canEdit || root.canDelete
        onClicked: mouse => menu.popup(root, mouse.x, mouse.y)
    }

    // The popups are parented to the window's overlay while open, so Spix finds the items inside
    // them by name from the window (mainWindow/msgEditAction), and only while they are open.
    Menu {
        id: menu

        MenuItem {
            objectName: "msgQuoteAction"
            enabled: root.canQuote
            height: visible ? implicitHeight : 0
            text: qsTr("Quote")
            visible: root.canQuote
            onTriggered: root.quoteRequested(root.messageId)
        }

        MenuItem {
            objectName: "msgReactAction"
            enabled: root.canReact
            height: visible ? implicitHeight : 0
            text: qsTr("Add reaction")
            visible: root.canReact
            onTriggered: root.reactRequested(root.messageId)
        }

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
        padding: Theme.dialogPadding
        parent: Overlay.overlay
        title: qsTr("Delete message")
        // A width of its own: the wrapping label can't size the dialog it is sized by.
        width: Theme.dialogWidth

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
        opacity: Theme.highlightOpacity
    }

    ColumnLayout {
        id: column

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacingXXL
        anchors.rightMargin: Theme.spacingXXL
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingXS

        // A heading above the first message of each day.
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacingM
            visible: root.firstOfDay
            spacing: Theme.spacingL

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: Theme.separatorWidth
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
                implicitHeight: Theme.separatorWidth
                color: root.palette.mid
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: !root.isSystem
            spacing: Theme.spacingXL

            Avatar {
                Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: Theme.avatarSize
                Layout.preferredHeight: Theme.avatarSize
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
                spacing: Theme.spacingXS

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingL

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
                        font.pointSize: Theme.fontSizeSmall
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
                        Layout.preferredHeight: Theme.menuButtonHeight
                        Layout.preferredWidth: Theme.menuButtonWidth
                        opacity: rowHover.hovered || menu.opened ? 1 : 0
                        padding: Theme.menuButtonPadding
                        text: "\u22EF"
                        visible: root.canQuote || root.canReact || root.canEdit || root.canDelete
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
                            color: Theme.error
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

                // The messages this one quotes, above what it says: a bar down the left, who said it
                // and when, and what it said. `quoteBody` has the text as written, which the tests
                // read; `quoteBodyHtml` is what is shown.
                Repeater {
                    model: root.quotes

                    delegate: RowLayout {
                        id: quote

                        required property int index
                        required property var modelData

                        objectName: "quote" + index
                        Layout.fillHeight: false
                        Layout.fillWidth: true
                        spacing: Theme.spacingL

                        Rectangle {
                            Layout.fillHeight: true
                            color: root.palette.mid
                            implicitWidth: Theme.accentBarWidth
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacingXXS

                            RowLayout {
                                spacing: Theme.spacingM

                                Label {
                                    objectName: "quoteAuthor"
                                    font.bold: true
                                    text: quote.modelData.author
                                    textFormat: Text.PlainText
                                }

                                Label {
                                    objectName: "quoteTime"
                                    color: root.palette.placeholderText
                                    text: quote.modelData.time
                                    textFormat: Text.PlainText
                                }
                            }

                            Item {
                                objectName: "quoteBody"
                                Layout.fillWidth: true
                                implicitHeight: quoteBodyText.implicitHeight
                                property string text: quote.modelData.body

                                Label {
                                    id: quoteBodyText

                                    objectName: "quoteBodyHtml"
                                    width: parent.width
                                    color: root.palette.placeholderText
                                    linkColor: root.palette.link
                                    text: quote.modelData.bodyHtml
                                    textFormat: Text.RichText
                                    wrapMode: Text.Wrap
                                    onLinkActivated: link => Qt.openUrlExternally(link)
                                }
                            }
                        }
                    }
                }

                // `text` is the message as written, which the tests read; `msgBodyHtml` is what is shown.
                Item {
                    id: bodyItem

                    objectName: "msgBody"
                    Layout.fillWidth: true
                    implicitHeight: bodyText.implicitHeight
                    // A message of only files or quotes has no text to leave a blank line for.
                    visible: root.body !== "" || (root.attachments.length === 0 && root.quotes.length === 0)
                    opacity: root.delivery === "sent" ? 1 : Theme.disabledOpacity
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
                        readonly property real naturalWidth: modelData.width > 0 ? modelData.width : Theme.attachmentFallbackWidth
                        readonly property real naturalHeight: modelData.height > 0 ? modelData.height : Theme.attachmentFallbackHeight
                        readonly property real fit: Math.min(1, Math.min(Theme.attachmentMaxWidth, content.width) / naturalWidth, Theme.attachmentMaxHeight / naturalHeight)

                        objectName: "attachment" + index
                        Layout.fillWidth: true
                        spacing: Theme.spacingXS

                        Item {
                            Layout.preferredWidth: attachment.naturalWidth * attachment.fit
                            Layout.preferredHeight: attachment.naturalHeight * attachment.fit
                            visible: attachment.modelData.isImage

                            Rectangle {
                                objectName: "attachmentPlaceholder"
                                anchors.fill: parent
                                color: root.palette.mid
                                opacity: Theme.placeholderOpacity
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
                            spacing: Theme.spacingL

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
                    spacing: Theme.spacingM
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
                            border.width: Theme.borderWidth
                            color: modelData.mine ? Qt.alpha(root.palette.highlight, Theme.highlightOpacity) : "transparent"
                            implicitHeight: Theme.chipHeight
                            implicitWidth: chipRow.implicitWidth + Theme.chipPadding
                            radius: height / 2

                            Row {
                                id: chipRow

                                anchors.centerIn: parent
                                spacing: Theme.spacingS

                                Item {
                                    // What is shown, which is what the tests read: the glyph, else the
                                    // address of the custom emoji's image, else the shortcode.
                                    property string text: chip.modelData.glyph !== "" ? chip.modelData.glyph : chip.modelData.image !== "" ? chip.modelData.image : chip.modelData.emoji

                                    objectName: "reactionEmoji"
                                    height: Theme.chipEmojiSize
                                    width: chip.modelData.image !== "" ? Theme.chipEmojiSize : shortcode.implicitWidth

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
                                        sourceSize.height: Theme.chipEmojiSize
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

                            // Takes the user's reaction back, or joins the others in it. Not a
                            // TapHandler, which Spix clicks don't trigger.
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: root.canReact ? Qt.PointingHandCursor : Qt.ArrowCursor
                                enabled: root.canReact
                                onClicked: Room.react(root.messageId, chip.modelData.emoji,
                                                      !chip.modelData.mine)
                            }

                            ToolTip.delay: Theme.toolTipDelay
                            ToolTip.text: chip.modelData.users
                            ToolTip.visible: hover.hovered
                        }
                    }

                    // Another emoji of the user's choosing.
                    Rectangle {
                        objectName: "reactionAdd"
                        border.color: root.palette.mid
                        border.width: Theme.borderWidth
                        color: "transparent"
                        implicitHeight: Theme.chipHeight
                        implicitWidth: Theme.chipAddWidth
                        radius: height / 2
                        visible: root.canReact

                        Label {
                            anchors.centerIn: parent
                            text: "+"
                            textFormat: Text.PlainText
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.reactRequested(root.messageId)
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: root.isSystem
            spacing: Theme.spacingL

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
