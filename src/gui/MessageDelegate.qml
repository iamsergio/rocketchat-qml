// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// One message of the open room, or of the open thread (`inThreadPane`). Named by message id, as the
// room rows are by room id. Its parts are named like the model roles.
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
    // As the parent of a thread: how many replies it has, and when the last came (a time of day, or
    // a day; empty without replies).
    required property int threadReplies
    required property string threadLastReply
    // As a reply: the id of its parent, else empty. A reply also sent to the room (`inThreadShown`)
    // shows a line that leads to its thread, with the start of the parent's text, if the room holds
    // it.
    required property string threadId
    required property bool inThreadShown
    required property string threadParentText
    // Whether the message can start a thread: it is a sent text message that is not a reply.
    required property bool canStartThread

    // The message is in the thread pane and not in the room: it has no thread of its own to start
    // or to show.
    property bool inThreadPane: false

    // The user chose to edit the message: the composer takes over.
    signal editRequested(string messageId)
    // The user chose to quote the message: the composer takes over.
    signal quoteRequested(string messageId)
    // The user wants to add a reaction to the message: the emoji picker opens.
    signal reactRequested(string messageId)
    // The user right-clicked a link, at `x`, `y` in `origin`: the menu of the link opens.
    signal linkMenuRequested(string url, Item origin, real x, real y)

    // The thread opens from this message, here or in the pane.
    readonly property bool threadStartable: canStartThread && !inThreadPane
    // The message is the parent of the thread beside the room.
    readonly property bool threadOpen: !inThreadPane && Thread.isOpen && Thread.parentId === messageId
    readonly property bool hasMenu: threadStartable || canQuote || canReact || canEdit || canDelete

    objectName: "msg_" + messageId
    width: ListView.view.width
    implicitHeight: column.implicitHeight
    // The row that holds the message itself, below the heading of its day.
    readonly property Item rowItem: isSystem ? systemRow : messageRow

    HoverHandler {
        id: rowHover
    }

    // Opens the message's menu at `x`, `y` in `origin`, if it has anything to offer.
    function openMenu(origin, x, y) {
        if (hasMenu)
            menu.popup(origin, x, y);
    }

    // Right-click opens the same menu as the button, and double-clicking opens the emoji picker
    // like its "Add reaction" does. The texts with links, the chips and the buttons take the
    // clicks on them.
    MouseArea {
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        anchors.fill: parent
        enabled: root.hasMenu
        onClicked: mouse => {
            if (mouse.button === Qt.RightButton)
                menu.popup(root, mouse.x, mouse.y);
        }
        onDoubleClicked: mouse => {
            if (mouse.button === Qt.LeftButton && root.canReact)
                root.reactRequested(root.messageId);
        }
    }

    // The popups are parented to the window's overlay while open, so Spix finds the items inside
    // them by name from the window (mainWindow/msgEditAction), and only while they are open.
    Menu {
        id: menu

        // Keeps the menu inside the window when it opens near an edge.
        margins: Theme.spacingL

        MenuItem {
            objectName: "msgThreadAction"
            enabled: root.threadStartable
            height: visible ? implicitHeight : Theme.spacingNone
            text: qsTr("Reply in thread")
            visible: root.threadStartable
            onTriggered: Thread.open(root.messageId)
        }

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
            PrimaryButton {
                objectName: "deleteConfirm"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                text: qsTr("Delete")
            }

            FlatButton {
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

    // The tint of the row under the pointer. Like the mark below, it covers the message and not
    // the heading of its day.
    Rectangle {
        color: Theme.rowHover
        height: root.rowItem.height + 2 * Theme.messagePaddingV
        visible: rowHover.hovered
        width: parent.width
        y: column.y + root.rowItem.y - Theme.messagePaddingV
    }

    // Marks the message whose thread is open beside the room: a faint tint, and no edge, which
    // means "about you".
    Rectangle {
        objectName: "msgThreadOpen"
        color: Theme.ownTint
        height: root.rowItem.height + 2 * Theme.messagePaddingV
        visible: root.threadOpen
        width: parent.width
        y: column.y + root.rowItem.y - Theme.messagePaddingV
    }

    // Marks a message that mentions the user: the row washed in the colour of mentions, and an edge
    // at its left.
    Rectangle {
        objectName: "msgMentionsMe"
        color: Theme.signalWash
        height: root.rowItem.height + 2 * Theme.messagePaddingV
        visible: root.mentionsMe
        width: parent.width
        y: column.y + root.rowItem.y - Theme.messagePaddingV

        Rectangle {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.top: parent.top
            color: Theme.signal
            width: Theme.accentBarWidth
        }
    }

    ColumnLayout {
        id: column

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Theme.spacingXXXL
        anchors.rightMargin: Theme.spacingXXXL
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingXS

        // A heading above the first message of each day.
        RowLayout {
            Layout.fillWidth: true
            Layout.bottomMargin: Theme.spacingL
            Layout.topMargin: Theme.spacingXXXL
            visible: root.firstOfDay
            spacing: Theme.spacingXXL

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: Theme.separatorWidth
                color: Theme.line
            }

            Label {
                objectName: "msgDaySeparator"
                text: root.dayLabel
                color: Theme.textMuted
                font.pointSize: Theme.fontSizeSmall
                font.weight: Theme.fontWeightSemibold
            }

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: Theme.separatorWidth
                color: Theme.line
            }
        }

        RowLayout {
            id: messageRow

            Layout.bottomMargin: Theme.messagePaddingV
            Layout.fillWidth: true
            Layout.topMargin: Theme.messagePaddingV
            visible: !root.isSystem
            spacing: Theme.spacingXXL

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
                        // The layout rounds the width down, so the ceiling keeps a name that fits
                        // from eliding; a longer name still gives way to the time and the menu.
                        Layout.fillWidth: true
                        Layout.maximumWidth: Math.ceil(implicitWidth)
                        elide: Text.ElideRight
                        font.weight: Theme.fontWeightSemibold
                        text: root.author
                        textFormat: Text.PlainText
                    }

                    Label {
                        id: timeLabel

                        objectName: "msgTime"
                        color: Theme.textFaint
                        font.pointSize: Theme.fontSizeTiny
                        text: root.time
                    }

                    Label {
                        objectName: "msgEdited"
                        color: Theme.textFaint
                        font.pointSize: Theme.fontSizeTiny
                        text: qsTr("(edited)")
                        visible: root.edited
                    }

                    Item {
                        Layout.fillWidth: true
                    }

                    // Shown when the pointer is over the message, if there is anything to do with it.
                    // It keeps its place the rest of the time, so the rows don't change height.
                    ToolButton {
                        id: menuButton

                        objectName: "msgMenuButton"
                        Layout.preferredHeight: Theme.menuButtonHeight
                        Layout.preferredWidth: Theme.menuButtonWidth
                        opacity: rowHover.hovered || menu.opened ? Theme.opaque : Theme.clear
                        padding: Theme.spacingNone
                        text: "\u22EF"
                        visible: root.hasMenu
                        onClicked: menu.popup(this, 0, height)

                        background: Rectangle {
                            border.color: Theme.raisedHover
                            border.width: Theme.borderWidth
                            color: Theme.raised
                            radius: Theme.radiusControl
                        }

                        contentItem: Text {
                            color: menuButton.hovered ? Theme.text : Theme.textMuted
                            font: menuButton.font
                            horizontalAlignment: Text.AlignHCenter
                            text: menuButton.text
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    // These exist only while the message is in that state, so that a test can find the one
                    // unsent message without knowing its id: the client picks it.
                    Loader {
                        active: root.delivery === "pending"

                        sourceComponent: Label {
                            objectName: "msgPending"
                            color: Theme.textFaint
                            font.pointSize: Theme.fontSizeTiny
                            text: qsTr("Sending…")
                        }
                    }

                    Loader {
                        active: root.delivery === "failed"

                        sourceComponent: Label {
                            objectName: "msgFailed"
                            color: Theme.error
                            font.pointSize: Theme.fontSizeTiny
                            text: qsTr("Not sent")
                        }
                    }

                    Loader {
                        active: root.delivery === "failed"

                        sourceComponent: FlatButton {
                            objectName: "msgRetry"
                            text: qsTr("Retry")
                            onClicked: Room.retry(root.messageId)
                        }
                    }
                }

                // A reply that was also sent to the room says where it belongs, and leads there. The
                // room shows it; the pane is the thread already. `text` is what is shown, which the
                // tests read.
                Item {
                    id: inThread

                    property string text: root.threadParentText !== "" ? qsTr("Replied in a thread: %1").arg(root.threadParentText) : qsTr("Replied in a thread")

                    objectName: "msgInThread"
                    Layout.fillWidth: true
                    implicitHeight: inThreadRow.implicitHeight
                    visible: root.inThreadShown && !root.inThreadPane

                    RowLayout {
                        id: inThreadRow

                        anchors.left: parent.left
                        anchors.right: parent.right
                        spacing: Theme.spacingM

                        Icon {
                            Layout.preferredHeight: Theme.inThreadIconSize
                            Layout.preferredWidth: Theme.inThreadIconSize
                            source: "icons/corner.svg"
                            tint: inThreadArea.containsMouse ? Theme.own : Theme.textFaint
                        }

                        Label {
                            Layout.fillWidth: true
                            color: inThreadArea.containsMouse ? Theme.own : Theme.textFaint
                            elide: Text.ElideRight
                            font.pointSize: Theme.fontSizeSmall
                            text: inThread.text
                            textFormat: Text.PlainText
                        }
                    }

                    MouseArea {
                        id: inThreadArea

                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        hoverEnabled: true
                        onClicked: Thread.open(root.threadId)
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
                        spacing: Theme.spacingXXL

                        Rectangle {
                            Layout.fillHeight: true
                            color: Theme.raisedHover
                            implicitWidth: Theme.quoteBarWidth
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacingXXS

                            RowLayout {
                                spacing: Theme.spacingM

                                Label {
                                    objectName: "quoteAuthor"
                                    font.pointSize: Theme.fontSizeSmall
                                    font.weight: Theme.fontWeightSemibold
                                    text: quote.modelData.author
                                    textFormat: Text.PlainText
                                }

                                Label {
                                    objectName: "quoteTime"
                                    color: Theme.textFaint
                                    font.pointSize: Theme.fontSizeTiny
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
                                    color: Theme.textMuted
                                    font.pointSize: Theme.fontSizeSmall
                                    text: Theme.richTextStyle + quote.modelData.bodyHtml
                                    textFormat: Text.RichText
                                    wrapMode: Text.Wrap
                                    onLinkActivated: link => Qt.openUrlExternally(link)

                                    LinkArea {
                                        label: quoteBodyText
                                        onMenuRequested: (url, origin, x, y) => root.linkMenuRequested(url, origin, x, y)
                                        onElsewhereClicked: (origin, x, y) => root.openMenu(origin, x, y)
                                    }
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
                        text: Theme.richTextStyle + root.html
                        textFormat: Text.RichText
                        wrapMode: Text.Wrap
                        onLinkActivated: link => Qt.openUrlExternally(link)

                        LinkArea {
                            label: bodyText
                            onMenuRequested: (url, origin, x, y) => root.linkMenuRequested(url, origin, x, y)
                            onElsewhereClicked: (origin, x, y) => root.openMenu(origin, x, y)
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
                                color: Theme.raised
                                radius: Theme.radiusAttachment
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

                            // Right-click: the menu of the link. A left-click goes to the TapHandler.
                            MouseArea {
                                id: pictureMenuArea

                                acceptedButtons: Qt.RightButton
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: mouse => root.linkMenuRequested(attachment.modelData.link, pictureMenuArea, mouse.x, mouse.y)
                            }
                        }

                        // A chip with the file's icon, its title and its size.
                        Rectangle {
                            // The address the file opens, which is what the tests read: Spix reads text.
                            property string text: attachment.modelData.link

                            objectName: "attachmentLink"
                            Layout.preferredHeight: fileRow.implicitHeight + 2 * Theme.spacingL
                            Layout.preferredWidth: Math.min(fileRow.implicitWidth + 2 * Theme.spacingXL, Theme.attachmentFileMaxWidth)
                            color: Theme.raised
                            radius: Theme.radiusAttachment
                            visible: !attachment.modelData.isImage

                            RowLayout {
                                id: fileRow

                                anchors.fill: parent
                                anchors.margins: Theme.spacingL
                                anchors.leftMargin: Theme.spacingXL
                                anchors.rightMargin: Theme.spacingXL
                                spacing: Theme.spacingXL

                                Icon {
                                    Layout.preferredHeight: Theme.attachmentFileIconSize
                                    Layout.preferredWidth: Theme.attachmentFileIconSize
                                    source: "icons/file.svg"
                                    tint: Theme.textMuted
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: Theme.spacingNone

                                    Label {
                                        objectName: "attachmentTitle"
                                        Layout.fillWidth: true
                                        color: Theme.own
                                        elide: Text.ElideMiddle
                                        font.pointSize: Theme.fontSizeSmall
                                        font.weight: Theme.fontWeightSemibold
                                        text: attachment.modelData.title
                                        textFormat: Text.PlainText
                                    }

                                    Label {
                                        objectName: "attachmentSize"
                                        color: Theme.textFaint
                                        font.pointSize: Theme.fontSizeTiny
                                        text: attachment.modelData.size
                                        textFormat: Text.PlainText
                                    }
                                }
                            }

                            TapHandler {
                                onTapped: Qt.openUrlExternally(attachment.modelData.link)
                            }

                            // Right-click: the menu of the link. A left-click goes to the TapHandler.
                            MouseArea {
                                id: fileMenuArea

                                acceptedButtons: Qt.RightButton
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: mouse => root.linkMenuRequested(attachment.modelData.link, fileMenuArea, mouse.x, mouse.y)
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
                            border.color: modelData.mine ? Theme.ownBorder : hover.hovered ? Theme.raisedHover : Theme.transparent
                            border.width: Theme.borderWidth
                            color: modelData.mine ? Theme.ownWash : Theme.raised
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
                                    color: chip.modelData.mine ? Theme.own : Theme.textMuted
                                    font.pointSize: Theme.fontSizeSmall
                                    font.weight: Theme.fontWeightSemibold
                                    font.features: ({
                                            "tnum": 1
                                        })
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

                    // Another emoji of the user's choosing. Shown when the pointer is over the message,
                    // like the menu button; it keeps its place the rest of the time, so the chips
                    // don't move.
                    Rectangle {
                        objectName: "reactionAdd"
                        opacity: rowHover.hovered ? Theme.opaque : Theme.clear
                        border.color: addArea.containsMouse ? Theme.raisedHover : Theme.transparent
                        border.width: Theme.borderWidth
                        color: Theme.raised
                        implicitHeight: Theme.chipHeight
                        implicitWidth: Theme.chipAddWidth
                        radius: height / 2
                        visible: root.canReact

                        Label {
                            anchors.centerIn: parent
                            color: Theme.textMuted
                            text: "+"
                            textFormat: Text.PlainText
                        }

                        MouseArea {
                            id: addArea

                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.reactRequested(root.messageId)
                        }
                    }
                }

                // The thread this message starts: how many replies it has and when the last came.
                // Clicking opens it beside the room.
                Rectangle {
                    id: summary

                    objectName: "msgThreadSummary"
                    Layout.topMargin: Theme.spacingM
                    border.color: root.threadOpen ? Theme.ownBorder : summaryHover.hovered ? Theme.raisedHover : Theme.transparent
                    border.width: Theme.borderWidth
                    color: root.threadOpen ? Theme.ownWash : summaryHover.hovered ? Theme.raised : Theme.transparent
                    implicitHeight: Theme.threadSummaryHeight
                    implicitWidth: summaryRow.implicitWidth + 2 * Theme.spacingXL
                    radius: Theme.radiusControl
                    visible: root.threadReplies > 0 && !root.inThreadPane

                    RowLayout {
                        id: summaryRow

                        anchors.centerIn: parent
                        spacing: Theme.spacingL

                        Icon {
                            Layout.preferredHeight: Theme.threadSummaryIconSize
                            Layout.preferredWidth: Theme.threadSummaryIconSize
                            source: "icons/thread.svg"
                            tint: Theme.own
                        }

                        Label {
                            objectName: "msgThreadCount"
                            color: Theme.own
                            font.pointSize: Theme.fontSizeSmall
                            font.weight: Theme.fontWeightBold
                            font.features: ({
                                    "tnum": 1
                                })
                            text: root.threadReplies === 1 ? qsTr("1 reply") : qsTr("%1 replies").arg(root.threadReplies)
                        }

                        Label {
                            objectName: "msgThreadLastReply"
                            color: Theme.textFaint
                            font.pointSize: Theme.fontSizeTiny
                            text: qsTr("Last reply %1").arg(root.threadLastReply)
                            textFormat: Text.PlainText
                            visible: root.threadLastReply !== ""
                        }
                    }

                    HoverHandler {
                        id: summaryHover
                    }

                    // Not a TapHandler, which Spix clicks don't trigger.
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Thread.open(root.messageId)
                    }
                }
            }
        }

        RowLayout {
            id: systemRow

            Layout.bottomMargin: Theme.messagePaddingV
            Layout.fillWidth: true
            Layout.topMargin: Theme.messagePaddingV
            visible: root.isSystem
            spacing: Theme.spacingL

            Label {
                objectName: "msgSystem"
                Layout.fillWidth: true
                color: Theme.textMuted
                font.pointSize: Theme.fontSizeSmall
                text: root.systemText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            Label {
                color: Theme.textFaint
                font.pointSize: Theme.fontSizeTiny
                text: root.time
            }
        }

        // Under the parent in the thread pane: how many replies follow.
        RowLayout {
            Layout.bottomMargin: Theme.spacingXS
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacingXXL
            spacing: Theme.spacingXXL
            visible: root.inThreadPane && root.messageId === Thread.parentId && root.threadReplies > 0

            Label {
                objectName: "threadReplies"
                color: Theme.textMuted
                font.pointSize: Theme.fontSizeSmall
                font.weight: Theme.fontWeightSemibold
                font.features: ({
                        "tnum": 1
                    })
                text: root.threadReplies === 1 ? qsTr("1 reply") : qsTr("%1 replies").arg(root.threadReplies)
            }

            Rectangle {
                Layout.fillWidth: true
                color: Theme.line
                implicitHeight: Theme.separatorWidth
            }
        }
    }
}
