// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// One thread of the list: who started it and when it last had a reply, the start of what they
// wrote, and how many replies it has. Named by the parent's id, as the messages are. Clicking it
// opens the thread in the pane.
ItemDelegate {
    id: root

    required property string threadId
    required property string author
    required property string lastReply
    required property string body
    required property string summary
    required property string avatar
    required property string avatarInitials
    required property string avatarColor

    objectName: "threadListRow_" + threadId
    width: ListView.view.width
    // The thread shown beside the list stands out in it.
    highlighted: Thread.parentId === threadId
    padding: Theme.spacingNone
    leftPadding: Theme.spacingXXXL
    rightPadding: Theme.spacingXXXL
    topPadding: Theme.spacingXL
    bottomPadding: Theme.spacingXL
    onClicked: Thread.open(threadId)

    background: Rectangle {
        color: root.highlighted ? Theme.ownWash : root.hovered ? Theme.raised : Theme.transparent
    }

    contentItem: RowLayout {
        spacing: Theme.spacingXL

        Avatar {
            Layout.alignment: Qt.AlignTop
            Layout.preferredHeight: Theme.threadRowAvatarSize
            Layout.preferredWidth: Theme.threadRowAvatarSize
            fillColor: root.avatarColor
            imageName: "threadListAvatar"
            initials: root.avatarInitials
            initialsName: "threadListAvatarInitials"
            placeholderName: "threadListAvatarPlaceholder"
            url: root.avatar
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingS

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingL

                Label {
                    objectName: "threadListAuthor"
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.weight: Theme.fontWeightSemibold
                    text: root.author
                    textFormat: Text.PlainText
                }

                Label {
                    objectName: "threadListTime"
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: root.lastReply
                    textFormat: Text.PlainText
                }
            }

            Label {
                objectName: "threadListText"
                Layout.fillWidth: true
                color: Theme.textMuted
                elide: Text.ElideRight
                maximumLineCount: Theme.threadRowTextLines
                text: root.body
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingM

                Icon {
                    Layout.preferredHeight: Theme.threadSummaryIconSize
                    Layout.preferredWidth: Theme.threadSummaryIconSize
                    source: "icons/thread.svg"
                    tint: Theme.own
                }

                Label {
                    objectName: "threadListSummary"
                    Layout.fillWidth: true
                    color: Theme.own
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeSmall
                    text: root.summary
                    textFormat: Text.PlainText
                }
            }
        }
    }
}
