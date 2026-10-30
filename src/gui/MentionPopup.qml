// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The members of the room that match the @word at the cursor of a composer, above it. The Mentions
// model decides whether it is open and what it lists; the composer forwards its text, its cursor
// and its keys. The pointer moves the current row and a click inserts it; the field keeps the
// focus, as nothing here takes it.
Rectangle {
    id: root

    // The composer that shows the popup: `room` or `thread`, as Mentions.owner says.
    required property string owner

    border.color: Theme.raisedHover
    border.width: Theme.borderWidth
    color: Theme.raised
    height: column.implicitHeight + 2 * (Theme.borderWidth + Theme.mentionPopupPadding)
    // Over the composer, its left edge on the composer's.
    objectName: "mentionPopup"
    radius: Theme.radiusPanel
    visible: Mentions.open && Mentions.owner === owner
    width: Theme.mentionPopupWidth
    y: -height - Theme.mentionPopupGap

    ColumnLayout {
        id: column

        anchors.fill: parent
        anchors.margins: Theme.borderWidth + Theme.mentionPopupPadding
        spacing: Theme.spacingNone

        Item {
            Layout.fillWidth: true
            implicitHeight: Theme.directoryLabelHeight

            Label {
                objectName: "mentionLabel"
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Theme.directoryLabelPaddingBottom
                anchors.left: parent.left
                anchors.leftMargin: Theme.rowPaddingH
                color: Theme.textFaint
                font.capitalization: Font.AllUppercase
                font.letterSpacing: Theme.directoryLabelLetterSpacing
                font.pointSize: Theme.fontSizeTiny
                font.weight: Theme.fontWeightSemibold
                text: Mentions.direct ? qsTr("In this conversation") : qsTr("Members")
            }
        }

        // The search runs and there is nothing to show yet.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.rowPaddingH
            Layout.preferredHeight: Theme.roomRowHeight
            Layout.rightMargin: Theme.rowPaddingH
            spacing: Theme.roomRowSpacing
            visible: Mentions.status === "searching"

            BusyIndicator {
                Layout.preferredHeight: Theme.directoryBusyIndicatorSize
                Layout.preferredWidth: Theme.directoryBusyIndicatorSize
                implicitHeight: Theme.directoryBusyIndicatorSize
                implicitWidth: Theme.directoryBusyIndicatorSize
                padding: Theme.spacingNone
                palette.dark: Theme.textMuted
                running: visible
            }

            Label {
                objectName: "mentionSearching"
                Layout.fillWidth: true
                color: Theme.textFaint
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeSmall
                text: qsTr("Searching members…")
            }
        }

        Label {
            objectName: "mentionError"
            Layout.fillWidth: true
            Layout.leftMargin: Theme.rowPaddingH
            Layout.preferredHeight: Theme.directoryMoreHeight
            Layout.rightMargin: Theme.rowPaddingH
            color: Theme.error
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: Mentions.failedText
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
            visible: Mentions.status === "failed"
        }

        Repeater {
            id: rows

            model: Mentions

            delegate: Item {
                id: entry

                required property string avatar
                required property string avatarColor
                required property string avatarInitials
                required property int count
                required property string description
                required property int index
                required property string name
                required property string nameStyled
                required property string rowType
                required property string username
                required property string usernameStyled

                // What the tests read: the username a row inserts.
                readonly property string text: rowType === "more" ? moreLabel.text : username

                Layout.fillWidth: true
                implicitHeight: rowType === "more" ? Theme.directoryMoreHeight : Theme.roomRowHeight
                objectName: rowType === "more" ? "mentionMore" : "mentionRow_" + username

                Rectangle {
                    anchors.fill: parent
                    color: Theme.raisedHover
                    radius: Theme.radiusControl
                    visible: Mentions.cursor === entry.index
                }

                MouseArea {
                    anchors.fill: parent
                    enabled: entry.rowType !== "more"
                    hoverEnabled: true
                    onClicked: Mentions.accept(entry.index)
                    onEntered: Mentions.setCursor(entry.index)
                }

                // A person.
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.rowPaddingH
                    anchors.rightMargin: Theme.rowPaddingH
                    spacing: Theme.roomRowSpacing
                    visible: entry.rowType === "person"

                    Avatar {
                        Layout.preferredHeight: Theme.roomAvatarSize
                        Layout.preferredWidth: Theme.roomAvatarSize
                        fillColor: entry.avatarColor
                        imageName: "mentionAvatar"
                        initials: entry.avatarInitials
                        initialsName: "mentionAvatarInitials"
                        placeholderName: "mentionAvatarPlaceholder"
                        url: entry.avatar
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.directoryNameGap

                        Label {
                            objectName: "mentionName"
                            // The name wins: it keeps its width and is elided only if it alone
                            // doesn't fit; the username takes what is left.
                            Layout.fillWidth: false
                            Layout.preferredWidth: implicitWidth
                            color: Theme.text
                            elide: Text.ElideRight
                            font.pointSize: Theme.fontSizeSmall
                            font.weight: Theme.fontWeightMedium
                            text: Theme.matchStyled(entry.nameStyled)
                            textFormat: Text.StyledText
                            visible: entry.name !== ""
                        }

                        Label {
                            objectName: "mentionUsername"
                            Layout.fillWidth: true
                            // The name keeps what it needs; the username takes the rest.
                            Layout.preferredWidth: Theme.spacingNone
                            color: Theme.textMuted
                            elide: Text.ElideRight
                            font.pointSize: Theme.fontSizeSmall
                            text: "@" + Theme.matchStyled(entry.usernameStyled)
                            textFormat: Text.StyledText
                        }
                    }
                }

                // @all or @here.
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.rowPaddingH
                    anchors.rightMargin: Theme.rowPaddingH
                    spacing: Theme.roomRowSpacing
                    visible: entry.rowType === "special"

                    Rectangle {
                        Layout.preferredHeight: Theme.roomAvatarSize
                        Layout.preferredWidth: Theme.roomAvatarSize
                        color: Theme.ownWash
                        radius: Theme.roomAvatarSize * Theme.avatarRadiusRatio

                        Icon {
                            anchors.centerIn: parent
                            height: Theme.mentionSpecialIconSize
                            source: "icons/people.svg"
                            tint: Theme.own
                            width: Theme.mentionSpecialIconSize
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.directoryNameGap

                        Label {
                            objectName: "mentionSpecialName"
                            color: Theme.text
                            font.pointSize: Theme.fontSizeSmall
                            font.weight: Theme.fontWeightMedium
                            text: "@" + Theme.matchStyled(entry.usernameStyled)
                            textFormat: Text.StyledText
                        }

                        Label {
                            objectName: "mentionDescription"
                            Layout.fillWidth: true
                            color: Theme.textMuted
                            elide: Text.ElideRight
                            font.pointSize: Theme.fontSizeSmall
                            text: entry.description
                            textFormat: Text.PlainText
                        }
                    }
                }

                Label {
                    id: moreLabel

                    anchors.left: parent.left
                    anchors.leftMargin: Theme.rowPaddingH + Theme.roomAvatarSize + Theme.roomRowSpacing
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.rowPaddingH
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textFaint
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeTiny
                    text: qsTr("%1 more, keep typing").arg(entry.count)
                    visible: entry.rowType === "more"
                }
            }
        }

        // Nobody matches and nothing else is listed.
        Label {
            objectName: "mentionNone"
            Layout.fillWidth: true
            Layout.leftMargin: Theme.rowPaddingH
            Layout.preferredHeight: Theme.roomRowHeight
            Layout.rightMargin: Theme.rowPaddingH
            color: Theme.textMuted
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: qsTr("No member matches “@%1”.").arg(Mentions.query)
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
            visible: Mentions.status === "idle" && rows.count === 0
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacingS
            color: Theme.raisedHover
            implicitHeight: Theme.separatorWidth
        }

        Flow {
            objectName: "mentionHint"
            Layout.fillWidth: true
            Layout.bottomMargin: Theme.mentionHintPaddingV
            Layout.leftMargin: Theme.rowPaddingH
            Layout.rightMargin: Theme.rowPaddingH
            Layout.topMargin: Theme.mentionHintPaddingV
            spacing: Theme.spacingXXL

            Row {
                spacing: Theme.mentionHintSpacing

                Keycap {
                    text: "↑"
                }

                Keycap {
                    text: "↓"
                }

                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: qsTr("move")
                }
            }

            Row {
                spacing: Theme.mentionHintSpacing

                Keycap {
                    text: qsTr("Enter")
                }

                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: qsTr("or")
                }

                Keycap {
                    text: qsTr("Tab")
                }

                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: qsTr("insert")
                }
            }

            Row {
                spacing: Theme.mentionHintSpacing

                Keycap {
                    text: qsTr("Esc")
                }

                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeTiny
                    text: qsTr("close")
                }
            }
        }
    }
}
