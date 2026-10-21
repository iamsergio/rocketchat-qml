// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The open thread, beside the room: the message it belongs to, the replies under it, and the box to
// reply in. RoomPage shows it only while a thread is open.
Rectangle {
    id: root

    objectName: "threadPane"
    color: Theme.surface
    onVisibleChanged: {
        if (visible)
            composer.focusField();
    }

    // The hairline between the room and the pane.
    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.top: parent.top
        color: Theme.line
        width: Theme.separatorWidth
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.separatorWidth
        spacing: Theme.spacingNone

        // The title and where the thread is, over a hairline.
        Item {
            Layout.fillWidth: true
            implicitHeight: Theme.roomHeaderHeight

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacingXXXL
                anchors.rightMargin: Theme.spacingXL
                spacing: Theme.spacingXL

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingL

                    Label {
                        objectName: "threadTitle"
                        font.pointSize: Theme.fontSizeHeading
                        font.weight: Theme.fontWeightBold
                        text: qsTr("Thread")
                    }

                    Label {
                        Layout.fillWidth: true
                        color: Theme.textFaint
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeSmall
                        text: "# " + Thread.roomName
                        textFormat: Text.PlainText
                    }
                }

                ToolButton {
                    id: close

                    objectName: "threadClose"
                    // Only the tool tip and the accessibility name show it.
                    display: AbstractButton.IconOnly
                    implicitHeight: Theme.iconButtonSize
                    implicitWidth: Theme.iconButtonSize
                    padding: Theme.spacingNone
                    text: qsTr("Close the thread")
                    onClicked: Thread.close()

                    ToolTip.delay: Theme.toolTipDelay
                    ToolTip.text: text
                    ToolTip.visible: hovered

                    background: Rectangle {
                        color: close.hovered ? Theme.raised : Theme.transparent
                        radius: Theme.radiusControl
                    }

                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            height: Theme.iconButtonIconSize
                            source: "icons/close.svg"
                            tint: close.hovered ? Theme.text : Theme.textMuted
                            width: Theme.iconButtonIconSize
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
            objectName: "threadError"
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXXL
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            text: Thread.errorText
            visible: text !== ""
            wrapMode: Text.Wrap
        }

        // As the room's list: newest first, bottom to top. The parent is the oldest row, at the
        // top, and the replies follow it.
        ListView {
            id: replies

            objectName: "threadMessageList"
            Layout.fillHeight: true
            Layout.fillWidth: true
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            model: ThreadMessageList
            verticalLayoutDirection: ListView.BottomToTop

            delegate: MessageDelegate {
                inThreadPane: true
                onEditRequested: messageId => composer.beginEdit(messageId)
                onQuoteRequested: messageId => composer.beginQuote(messageId)
                onReactRequested: messageId => picker.openFor(messageId)
            }

            ScrollBar.vertical: ScrollBar {}

            // Above the oldest row, which in this direction is the footer: a spinner while older
            // replies load.
            footer: Item {
                // The list's content item, as wide as the list (see RoomView).
                width: parent?.width ?? 0
                height: olderBusy.visible ? Theme.historyFooterHeight : Theme.spacingNone

                BusyIndicator {
                    id: olderBusy

                    objectName: "threadLoadingOlder"
                    anchors.centerIn: parent
                    height: Theme.busyIndicatorSize
                    palette.dark: Theme.textMuted
                    running: visible
                    visible: Thread.loadingOlder
                }
            }

            // Reaching the top asks for older replies; the core ignores the request when there is
            // nothing to load or a load is in progress.
            onAtYBeginningChanged: {
                if (atYBeginning && count > 0)
                    Thread.loadOlder();
            }

            BusyIndicator {
                objectName: "threadLoading"
                anchors.centerIn: parent
                implicitHeight: Theme.busyIndicatorLargeSize
                implicitWidth: Theme.busyIndicatorLargeSize
                palette.dark: Theme.textMuted
                running: visible
                visible: Thread.loading
            }
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

        Composer {
            id: composer

            objectName: "threadComposer"
            Layout.bottomMargin: Theme.spacingXXXL
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingXXXL
            Layout.rightMargin: Theme.spacingXXXL
            Layout.topMargin: Theme.spacingM
            forThread: true
        }
    }
}
