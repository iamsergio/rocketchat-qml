// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The open room: its name, its messages, oldest at the top, and the box to write in.
Rectangle {
    color: Theme.surface

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spacingNone

        // The kind of the open room, its name and under it its topic, over a hairline.
        Item {
            Layout.fillWidth: true
            implicitHeight: Theme.roomHeaderHeight
            visible: Room.roomId !== ""

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacingXXXL
                anchors.rightMargin: Theme.spacingXXXL
                spacing: Theme.spacingXL

                Icon {
                    Layout.preferredHeight: Theme.roomHeaderIconSize
                    Layout.preferredWidth: Theme.roomHeaderIconSize
                    source: Room.kind !== "" ? "icons/" + Room.kind + ".svg" : ""
                    tint: Theme.own
                }

                // Centred, so that the name of a room without a topic is in the middle.
                ColumnLayout {
                    Layout.alignment: Qt.AlignVCenter
                    Layout.fillWidth: true
                    spacing: Theme.spacingXXS

                    Label {
                        objectName: "roomTitle"
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeHeading
                        font.weight: Theme.fontWeightBold
                        text: Room.name
                        textFormat: Text.PlainText
                    }

                    // One line, clipped with a fade where it goes on: a Text of rich text can't elide.
                    Item {
                        id: topicLine

                        // The topic is wider than the room has for it.
                        readonly property bool overflowing: topic.implicitWidth > width

                        Layout.fillWidth: true
                        clip: true
                        implicitHeight: topic.implicitHeight
                        visible: Room.topicHtml !== ""

                        Label {
                            id: topic

                            objectName: "roomTopic"
                            color: Theme.textMuted
                            font.pointSize: Theme.fontSizeSmall
                            text: Theme.richTextStyle + Room.topicHtml
                            textFormat: Text.RichText
                            wrapMode: Text.NoWrap
                            onLinkActivated: link => Qt.openUrlExternally(link)

                            LinkArea {
                                label: topic
                                onMenuRequested: (url, origin, x, y) => linkMenu.openFor(url, origin, x, y)
                            }
                        }

                        Rectangle {
                            anchors.bottom: parent.bottom
                            anchors.right: parent.right
                            anchors.top: parent.top
                            visible: topicLine.overflowing
                            width: Theme.roomTopicFadeWidth

                            gradient: Gradient {
                                orientation: Gradient.Horizontal

                                GradientStop {
                                    color: Theme.surfaceClear
                                    position: Theme.gradientStart
                                }

                                GradientStop {
                                    color: Theme.surface
                                    position: Theme.gradientEnd
                                }
                            }
                        }

                        HoverHandler {
                            id: topicHover
                        }

                        // The whole topic, if it doesn't fit.
                        ToolTip {
                            delay: Theme.toolTipDelay
                            visible: topicHover.hovered && topicLine.overflowing
                            width: Math.min(implicitWidth, Theme.roomTopicTipMaxWidth)

                            contentItem: Label {
                                objectName: "roomTopicTip"
                                text: Theme.richTextStyle + Room.topicHtml
                                textFormat: Text.RichText
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }
            }

            Rectangle {
                anchors.bottom: parent.bottom
                color: Theme.line
                height: Theme.separatorWidth
                width: parent.width
            }
        }

        Label {
            objectName: "roomPlaceholder"
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Room.roomId === ""
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: Theme.textFaint
            text: qsTr("Select a room")
        }

        Label {
            objectName: "roomError"
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXXL
            visible: text !== ""
            text: Room.errorText
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            wrapMode: Text.Wrap
        }

        // The model is newest first and the list runs bottom to top: row 0, the newest message, is at
        // the bottom, where a room opens. Older messages are added after the last row, above everything
        // on screen, so the view keeps its place without any help.
        ListView {
            id: messages

            objectName: "messageList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Room.roomId !== ""
            clip: true
            focus: true
            model: MessageList
            delegate: MessageDelegate {
                onEditRequested: messageId => composer.beginEdit(messageId)
                onQuoteRequested: messageId => composer.beginQuote(messageId)
                onReactRequested: messageId => picker.openFor(messageId)
                onLinkMenuRequested: (url, origin, x, y) => linkMenu.openFor(url, origin, x, y)
            }
            verticalLayoutDirection: ListView.BottomToTop
            boundsBehavior: Flickable.StopAtBounds

            ScrollBar.vertical: ScrollBar {}

            // Above the oldest message, which in this direction is the footer: the start of the
            // conversation, or a spinner while older messages load.
            footer: Item {
                // The list's content item, as wide as the list. Not ListView.view.width, which qmllint
                // rejects here (attached-property-reuse).
                width: parent?.width ?? 0
                height: startLabel.visible || olderBusy.visible ? Theme.historyFooterHeight : 0

                Label {
                    id: startLabel

                    objectName: "historyStart"
                    anchors.centerIn: parent
                    visible: Room.reachedStart
                    color: Theme.textFaint
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("Start of the conversation")
                }

                BusyIndicator {
                    id: olderBusy

                    objectName: "loadingOlder"
                    anchors.centerIn: parent
                    palette.dark: Theme.textMuted
                    height: Theme.busyIndicatorSize
                    visible: Room.loadingOlder
                    running: visible
                }
            }

            // Reaching the top asks for older messages; the core ignores the request when there is
            // nothing to load or a load is in progress. Whatever the layout direction, the beginning
            // of the content is its top in y.
            onAtYBeginningChanged: {
                if (atYBeginning && count > 0)
                    Room.loadOlder();
            }

            Keys.onPressed: event => {
                switch (event.key) {
                case Qt.Key_Home:
                    positionViewAtEnd();
                    break;
                case Qt.Key_End:
                    positionViewAtBeginning();
                    break;
                case Qt.Key_PageUp:
                    contentY = Math.max(contentY - height, originY);
                    break;
                case Qt.Key_PageDown:
                    contentY = Math.min(contentY + height, originY + contentHeight - height);
                    break;
                default:
                    return;
                }
                event.accepted = true;
            }

            BusyIndicator {
                objectName: "roomLoading"
                anchors.centerIn: parent
                implicitHeight: Theme.busyIndicatorLargeSize
                implicitWidth: Theme.busyIndicatorLargeSize
                palette.dark: Theme.textMuted
                visible: Room.loading
                running: visible
            }
        }

        // Why the last edit, deletion or reaction failed, until it is dismissed or the user tries
        // again.
        RowLayout {
            objectName: "roomActionError"
            Layout.fillWidth: true
            Layout.margins: Theme.spacingL
            spacing: Theme.spacingL
            visible: Room.actionError !== ""

            Label {
                objectName: "roomActionErrorText"
                Layout.fillWidth: true
                color: Theme.error
                text: Room.actionError
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            FlatButton {
                objectName: "roomActionErrorDismiss"
                text: qsTr("Dismiss")
                onClicked: Room.dismissActionError()
            }
        }

        // What to do with the link that was right-clicked, in a message or in the topic.
        LinkMenu {
            id: linkMenu
        }

        // The emoji to react with, for the message whose menu or "+" chip asked.
        EmojiPicker {
            id: picker

            property string messageId

            function openFor(id) {
                messageId = id;
                open();
            }

            onPicked: shortcode => Room.react(messageId, shortcode, true)
        }

        // Who is typing in the room. Empty most of the time, but its height is always taken.
        Label {
            objectName: "roomTyping"
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingXXXL
            Layout.preferredHeight: Theme.typingLineHeight
            Layout.rightMargin: Theme.spacingXXXL
            color: Theme.textMuted
            elide: Text.ElideRight
            font.pointSize: Theme.fontSizeSmall
            text: Room.typing
            textFormat: Text.PlainText
            verticalAlignment: Text.AlignVCenter
            visible: Room.roomId !== ""
        }

        Composer {
            id: composer

            Layout.fillWidth: true
            Layout.bottomMargin: Theme.spacingXXXL
            Layout.leftMargin: Theme.spacingXXXL
            Layout.rightMargin: Theme.spacingXXXL
            Layout.topMargin: Theme.spacingM
            visible: Room.roomId !== ""
        }
    }
}
