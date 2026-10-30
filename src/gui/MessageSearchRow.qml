// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// A message the search found: who wrote it and when, the start of its text with the words searched
// for marked, and, for a reply in a thread, a line that says so. Named by the message's id, as the
// messages are. A click puts the cursor on it; showing it in the room comes with MSG-008's next
// step.
ItemDelegate {
    id: root

    required property int index
    required property string messageId
    required property string author
    required property string time
    // Escaped, with <b> around the words searched for.
    required property string body
    required property bool inThread
    required property string avatar
    required property string avatarInitials
    required property string avatarColor
    // The message the user chose to be shown in the room.
    required property bool picked

    // The cursor is on this row (tests read it as `checked`).
    checked: index === MessageSearch.cursor
    objectName: "messageSearchRow_" + messageId
    width: ListView.view.width
    padding: Theme.spacingNone
    leftPadding: Theme.spacingXXXL
    rightPadding: Theme.spacingXXXL
    topPadding: Theme.spacingXL
    bottomPadding: Theme.spacingXL
    onClicked: MessageSearch.pick(index)
    // The pointer moves the cursor, as the keys do, so that only one row is lit.
    onHoveredChanged: {
        if (hovered)
            MessageSearch.setCursor(index);
    }

    background: Rectangle {
        color: root.picked ? Theme.ownWash : root.checked ? Theme.raised : Theme.transparent
    }

    contentItem: RowLayout {
        spacing: Theme.spacingXL

        Avatar {
            Layout.alignment: Qt.AlignTop
            Layout.preferredHeight: Theme.threadRowAvatarSize
            Layout.preferredWidth: Theme.threadRowAvatarSize
            fillColor: root.avatarColor
            imageName: "messageSearchAvatar"
            initials: root.avatarInitials
            initialsName: "messageSearchAvatarInitials"
            placeholderName: "messageSearchAvatarPlaceholder"
            url: root.avatar
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingS

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingL

                Label {
                    objectName: "messageSearchAuthor"
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.weight: Theme.fontWeightSemibold
                    text: root.author
                    textFormat: Text.PlainText
                }

                Label {
                    objectName: "messageSearchTime"
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: root.time
                    textFormat: Text.PlainText
                }
            }

            Label {
                objectName: "messageSearchText"
                Layout.fillWidth: true
                color: Theme.textMuted
                elide: Text.ElideRight
                maximumLineCount: Theme.messageSearchTextLines
                text: Theme.matchStyled(root.body)
                textFormat: Text.StyledText
                wrapMode: Text.Wrap
            }

            RowLayout {
                objectName: "messageSearchThread"
                Layout.fillWidth: true
                spacing: Theme.spacingM
                visible: root.inThread

                Icon {
                    Layout.preferredHeight: Theme.threadSummaryIconSize
                    Layout.preferredWidth: Theme.threadSummaryIconSize
                    source: "icons/thread.svg"
                    tint: Theme.own
                }

                Label {
                    Layout.fillWidth: true
                    color: Theme.own
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("In a thread")
                    textFormat: Text.PlainText
                }
            }
        }
    }
}
