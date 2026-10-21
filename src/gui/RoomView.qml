// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

// The open room: its name, its messages, oldest at the top, and the box to write in.
ColumnLayout {
    spacing: 0

    ToolBar {
        Layout.fillWidth: true
        visible: Room.roomId !== ""

        Label {
            objectName: "roomTitle"
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            font.bold: true
            text: Room.name
            textFormat: Text.PlainText
        }
    }

    Label {
        objectName: "roomPlaceholder"
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: Room.roomId === ""
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        color: palette.placeholderText
        text: qsTr("Select a room")
    }

    Label {
        objectName: "roomError"
        Layout.fillWidth: true
        Layout.margins: 12
        visible: text !== ""
        text: Room.errorText
        color: "#c62828"
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
            height: startLabel.visible || olderBusy.visible ? 40 : 0

            Label {
                id: startLabel

                objectName: "historyStart"
                anchors.centerIn: parent
                visible: Room.reachedStart
                color: palette.placeholderText
                text: qsTr("Start of the conversation")
            }

            BusyIndicator {
                id: olderBusy

                objectName: "loadingOlder"
                anchors.centerIn: parent
                height: 32
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
            visible: Room.loading
            running: visible
        }
    }

    // Why the last edit or deletion failed, until it is dismissed or the user tries again.
    RowLayout {
        objectName: "roomActionError"
        Layout.fillWidth: true
        Layout.margins: 8
        visible: Room.actionError !== ""

        Label {
            objectName: "roomActionErrorText"
            Layout.fillWidth: true
            color: "#c62828"
            text: Room.actionError
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        ToolButton {
            objectName: "roomActionErrorDismiss"
            text: qsTr("Dismiss")
            onClicked: Room.dismissActionError()
        }
    }

    Composer {
        id: composer

        Layout.fillWidth: true
        Layout.margins: 8
        visible: Room.roomId !== ""
    }
}
