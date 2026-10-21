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
    required property string avatar
    required property string avatarInitials
    required property string avatarColor
    required property int unread
    required property int userMentions
    required property bool alert
    required property bool favorite
    required property bool hasUnread
    required property bool canLeave

    objectName: "room_" + roomId
    width: ListView.view.width
    highlighted: Room.roomId === roomId
    padding: Theme.delegatePadding
    onClicked: Room.open(roomId)

    // Right-click opens the menu of the room. A MouseArea, as Spix clicks don't reach a TapHandler.
    MouseArea {
        acceptedButtons: Qt.RightButton
        anchors.fill: parent
        onClicked: mouse => menu.popup(root, mouse.x, mouse.y)
    }

    // The popups are parented to the window's overlay while open, so Spix finds the items inside
    // them by name from the window (mainWindow/roomLeaveAction), and only while they are open.
    Menu {
        id: menu

        MenuItem {
            objectName: "roomFavoriteAction"
            text: root.favorite ? qsTr("Remove from favorites") : qsTr("Add to favorites")
            onTriggered: RoomList.setFavorite(root.roomId, !root.favorite)
        }

        MenuItem {
            objectName: "roomMarkReadAction"
            height: visible ? implicitHeight : 0
            text: qsTr("Mark as read")
            visible: root.hasUnread
            onTriggered: RoomList.markRead(root.roomId)
        }

        // The open room stays read, so it can't be marked unread.
        MenuItem {
            objectName: "roomMarkUnreadAction"
            height: visible ? implicitHeight : 0
            text: qsTr("Mark as unread")
            visible: !root.hasUnread && !root.highlighted
            onTriggered: RoomList.markUnread(root.roomId)
        }

        // Direct messages can't be left.
        MenuItem {
            objectName: "roomLeaveAction"
            enabled: root.canLeave
            height: visible ? implicitHeight : 0
            text: qsTr("Leave")
            visible: root.canLeave
            onTriggered: leaveDialog.open()
        }
    }

    Dialog {
        id: leaveDialog

        anchors.centerIn: Overlay.overlay
        modal: true
        padding: Theme.dialogPadding
        parent: Overlay.overlay
        title: qsTr("Leave room")
        // A width of its own: the wrapping label can't size the dialog it is sized by.
        width: Theme.dialogWidth

        footer: DialogButtonBox {
            Button {
                objectName: "roomLeaveConfirm"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                text: qsTr("Leave")
            }

            Button {
                objectName: "roomLeaveCancel"
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                text: qsTr("Cancel")
            }
        }

        onAccepted: RoomList.leave(root.roomId)

        // Named, because Spix finds items and a Dialog is not one.
        contentItem: Label {
            objectName: "roomLeaveDialog"
            text: qsTr("Leave %1?").arg(root.name)
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }

    contentItem: RowLayout {
        spacing: Theme.spacingXXL

        Avatar {
            Layout.preferredWidth: Theme.roomIconSize
            Layout.preferredHeight: Theme.roomIconSize
            url: root.avatar
            initials: root.avatarInitials
            fillColor: root.avatarColor
            imageName: "roomAvatar"
            placeholderName: "roomAvatarPlaceholder"
            initialsName: "roomAvatarInitials"
        }

        // The icon is named after the room kind: icons/channel.svg, private.svg, direct.svg. Hidden
        // until it has loaded, so that a missing SVG plugin shows in the GUI tests.
        Image {
            objectName: "roomKind_" + root.kind
            Layout.preferredWidth: Theme.roomIconSize
            fillMode: Image.Pad
            source: "icons/" + root.kind + ".svg"
            sourceSize: Qt.size(Theme.roomIconSourceSize, Theme.roomIconSourceSize)
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

        Label {
            objectName: "roomFavorite"
            text: "\u2605"
            visible: root.favorite
        }

        // Mentions of the user.
        Rectangle {
            visible: root.userMentions > 0
            implicitWidth: mentionRow.implicitWidth + Theme.badgePaddingH
            implicitHeight: mentionRow.implicitHeight + Theme.badgePaddingV
            radius: height / 2
            color: root.palette.highlight

            Row {
                id: mentionRow

                anchors.centerIn: parent
                spacing: Theme.spacingNone

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
            implicitWidth: Math.max(unreadLabel.implicitWidth + Theme.badgePaddingH, height)
            implicitHeight: unreadLabel.implicitHeight + Theme.badgePaddingV
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
