// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// A person or a channel the server's directory found, in the room search. It looks like a room's
// row, and says at its right what a click does while the pointer or the keyboard cursor is on it.
// Named by username for a person and by room id for a channel: names aren't unique.
ItemDelegate {
    id: root

    required property string avatar
    required property string avatarColor
    required property string avatarInitials
    required property bool busy
    required property int index
    required property int members
    required property string name
    required property string roomId
    required property string rowType
    required property string username

    // Whether the keyboard cursor is on the row: it looks like the pointer over it.
    property bool cursor: false
    readonly property bool person: rowType === "person"
    // The pointer or the cursor is on the row, and nothing is under way to hide it.
    readonly property bool pointed: (hovered || cursor) && !busy

    implicitHeight: Theme.roomRowHeight
    leftPadding: Theme.roomRowInset + Theme.rowPaddingH
    objectName: (person ? "directoryPerson_" + username : "directoryChannel_" + roomId)
    padding: Theme.spacingNone
    rightPadding: Theme.roomRowInset + Theme.rowPaddingH
    width: ListView.view.width

    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: Theme.roomRowInset
            anchors.rightMargin: Theme.roomRowInset
            color: root.hovered || root.cursor ? Theme.surface : Theme.transparent
            radius: Theme.radiusControl
        }
    }

    contentItem: RowLayout {
        spacing: Theme.roomRowSpacing

        Avatar {
            Layout.preferredHeight: Theme.roomAvatarSize
            Layout.preferredWidth: Theme.roomAvatarSize
            fillColor: root.avatarColor
            imageName: "roomAvatar"
            initials: root.avatarInitials
            initialsName: "roomAvatarInitials"
            placeholderName: "roomAvatarPlaceholder"
            url: root.avatar
        }

        // The name and, faint after it, the username of a person whose name is another; elided
        // together.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.directoryNameGap

            Label {
                objectName: "directoryName"
                Layout.fillWidth: true
                Layout.maximumWidth: implicitWidth
                color: Theme.textMuted
                elide: Text.ElideRight
                text: root.name
                textFormat: Text.PlainText
            }

            Label {
                id: userLabel

                objectName: "directoryUsername"
                Layout.fillWidth: true
                color: Theme.textFaint
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeTiny
                text: root.username
                textFormat: Text.PlainText
                visible: root.person && root.username !== root.name
            }
        }

        // What a click does.
        Label {
            objectName: "directoryHint"
            color: Theme.own
            font.pointSize: Theme.fontSizeTiny
            font.weight: Theme.fontWeightSemibold
            text: root.person ? qsTr("Message") : qsTr("View")
            visible: root.pointed
        }

        Label {
            objectName: "directoryMembers"
            color: Theme.textFaint
            font.features: ({
                    "tnum": 1
                })
            font.pointSize: Theme.fontSizeTiny
            text: root.members === 1 ? qsTr("1 member") : qsTr("%1 members").arg(root.members)
            visible: !root.person && !root.pointed && !root.busy
        }

        // Until the server has answered; the row can't be acted on again meanwhile.
        Label {
            objectName: "directoryBusy"
            color: Theme.textMuted
            font.pointSize: Theme.fontSizeTiny
            text: qsTr("Opening…")
            visible: root.busy
        }
    }
}
