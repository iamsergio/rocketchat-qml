// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

// The open room: its name and its messages, oldest at the top.
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

    ListView {
        id: messages

        objectName: "messageList"
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: Room.roomId !== ""
        clip: true
        focus: true
        model: MessageList
        delegate: MessageDelegate {}
        boundsBehavior: Flickable.StopAtBounds

        // Where the first message was on screen before older ones were inserted above it.
        property real anchorOffset: NaN
        // The index that message has now, or -1 when there is nothing to put back.
        property int anchorIndex: -1

        function keepAnchor() {
            if (anchorIndex < 0)
                return;
            forceLayout();
            const item = itemAtIndex(anchorIndex);
            if (item)
                contentY = item.y - anchorOffset;
            else
                positionViewAtIndex(anchorIndex, ListView.Beginning);
            anchorIndex = -1;
        }

        ScrollBar.vertical: ScrollBar {}

        // Above the first message: the start of the conversation, or a spinner while older
        // messages load.
        header: Item {
            width: ListView.view.width
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
        // nothing to load or a load is in progress.
        onAtYBeginningChanged: {
            if (atYBeginning && count > 0)
                Room.loadOlder();
        }

        Keys.onPressed: event => {
            switch (event.key) {
            case Qt.Key_Home:
                positionViewAtBeginning();
                break;
            case Qt.Key_End:
                positionViewAtEnd();
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

        Connections {
            target: MessageList

            // A new room, or the first messages of one: show the newest.
            function onModelReset() {
                messages.positionViewAtEnd();
                Qt.callLater(messages.positionViewAtEnd);
            }

            // Older messages go in above the first one. The view is at the top then, and would
            // keep showing the top of the new ones: remember where the first message is on
            // screen, and put it back there once the list has taken the new rows in.
            function onRowsAboutToBeInserted(parent, first, last) {
                const top = messages.itemAtIndex(0);
                messages.anchorOffset = first === 0 && top ? top.y - messages.contentY : NaN;
            }

            function onRowsInserted(parent, first, last) {
                if (first !== 0 || isNaN(messages.anchorOffset))
                    return;
                // The list handles the same signal after this one, so wait for it.
                messages.anchorIndex = last + 1;
                Qt.callLater(messages.keepAnchor);
            }
        }

        BusyIndicator {
            objectName: "roomLoading"
            anchors.centerIn: parent
            visible: Room.loading
            running: visible
        }
    }
}
