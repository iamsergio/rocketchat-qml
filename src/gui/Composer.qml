// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

// The box under the messages of the open room. Enter sends, Shift+Enter starts a new line.
RowLayout {
    id: root

    spacing: 8

    function send() {
        if (Room.send(field.text))
            field.clear();
    }

    ScrollView {
        Layout.fillWidth: true
        Layout.maximumHeight: 120

        TextArea {
            id: field

            objectName: "composerField"
            placeholderText: qsTr("Message")
            wrapMode: TextEdit.Wrap

            Keys.onPressed: event => {
                if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                    root.send();
                    event.accepted = true;
                }
            }
        }
    }

    Button {
        objectName: "composerSend"
        Layout.alignment: Qt.AlignBottom
        text: qsTr("Send")
        enabled: field.length > 0
        onClicked: root.send()
    }
}
