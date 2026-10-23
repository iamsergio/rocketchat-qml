// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// One row of the room list. Named by room id: room names aren't unique.
ItemDelegate {
    id: root

    required property int index
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

    // The room list's rows are `room_<id>`; another list of the same rows names them differently.
    property string namePrefix: "room_"
    // Whether the keyboard cursor is on the row, as in a list of search results: it looks like the
    // pointer over it, and the open room's look wins over it.
    property bool cursor: false
    // Whether a right-click opens the menu of the room.
    property bool menuEnabled: true

    // Whether the room has something new for the user, which its name shows.
    readonly property bool emphasised: alert || hasUnread || unread > 0

    objectName: namePrefix + roomId
    width: ListView.view.width
    implicitHeight: Theme.roomRowHeight
    highlighted: Room.roomId === roomId
    padding: Theme.spacingNone
    leftPadding: Theme.roomRowInset + Theme.rowPaddingH
    rightPadding: Theme.roomRowInset + Theme.rowPaddingH
    onClicked: Room.open(roomId)

    // The row is as wide as the list, as the view sets its position; the background floats inside.
    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: Theme.roomRowInset
            anchors.rightMargin: Theme.roomRowInset
            color: root.highlighted ? Theme.ownWash : (root.hovered || root.cursor) ? Theme.surface : Theme.transparent
            radius: Theme.radiusControl
        }
    }

    // Right-click opens the menu of the room. A MouseArea, as Spix clicks don't reach a TapHandler.
    MouseArea {
        acceptedButtons: Qt.RightButton
        anchors.fill: parent
        enabled: root.menuEnabled
        onClicked: mouse => menu.popup(root, mouse.x, mouse.y)
    }

    // The popups are parented to the window's overlay while open, so Spix finds the items inside
    // them by name from the window (mainWindow/roomLeaveAction), and only while they are open.
    Menu {
        id: menu

        // Keeps the menu inside the window when it opens near an edge.
        margins: Theme.spacingL

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
            PrimaryButton {
                objectName: "roomLeaveConfirm"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                text: qsTr("Leave")
            }

            FlatButton {
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
        spacing: Theme.roomRowSpacing

        Avatar {
            Layout.preferredWidth: Theme.roomAvatarSize
            Layout.preferredHeight: Theme.roomAvatarSize
            url: root.avatar
            initials: root.avatarInitials
            fillColor: root.avatarColor
            imageName: "roomAvatar"
            placeholderName: "roomAvatarPlaceholder"
            initialsName: "roomAvatarInitials"
        }

        Label {
            objectName: "roomName"
            Layout.fillWidth: true
            color: root.highlighted || root.emphasised ? Theme.text : Theme.textMuted
            elide: Text.ElideRight
            font.weight: root.emphasised ? Theme.fontWeightSemibold : Theme.fontWeightNormal
            text: root.name
        }

        // The icon is named after the room kind: icons/channel.svg, private.svg, direct.svg. Hidden
        // until it has loaded, so that a missing SVG plugin shows in the GUI tests.
        Icon {
            objectName: "roomKind_" + root.kind
            Layout.preferredHeight: Theme.roomIconSize
            Layout.preferredWidth: Theme.roomIconSize
            source: "icons/" + root.kind + ".svg"
            tint: root.highlighted ? Theme.own : Theme.textFaint
            visible: status === Image.Ready

            onStatusChanged: {
                if (status === Image.Error)
                    console.warn("Can't load the room icon", source, "(is the Qt SVG image plugin installed?)");
            }
        }

        // Mentions of the user.
        Rectangle {
            visible: root.userMentions > 0
            implicitWidth: Math.max(mentionRow.implicitWidth + Theme.badgePaddingH, Theme.unreadBadgeMinWidth)
            implicitHeight: Theme.unreadBadgeHeight
            radius: height / 2
            color: Theme.signal

            Row {
                id: mentionRow

                anchors.centerIn: parent
                spacing: Theme.spacingNone

                Label {
                    text: "@"
                    color: Theme.textOnSignal
                    font.pointSize: Theme.fontSizeTiny
                    font.weight: Theme.fontWeightBold
                }

                Label {
                    objectName: "roomMentions"
                    text: root.userMentions
                    color: Theme.textOnSignal
                    font.features: ({
                            "tnum": 1
                        })
                    font.pointSize: Theme.fontSizeTiny
                    font.weight: Theme.fontWeightBold
                }
            }
        }

        Rectangle {
            visible: root.unread > 0
            implicitWidth: Math.max(unreadLabel.implicitWidth + Theme.badgePaddingH, Theme.unreadBadgeMinWidth)
            implicitHeight: Theme.unreadBadgeHeight
            radius: height / 2
            color: Theme.raisedHover

            Label {
                id: unreadLabel

                objectName: "roomUnread"
                anchors.centerIn: parent
                color: Theme.text
                font.features: ({
                        "tnum": 1
                    })
                font.pointSize: Theme.fontSizeTiny
                font.weight: Theme.fontWeightBold
                text: root.unread
            }
        }
    }
}
