// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

// The box under the messages of the open room. Enter sends, Shift+Enter starts a new line.
//
// It also edits a message of the user's: beginEdit() puts the message's text in the field, a bar
// above it says so, and Enter saves instead of sending. Escape, the bar's button, or opening
// another room cancels. The draft that was in the field before editing comes back when editing
// ends, by saving or cancelling.
ColumnLayout {
    id: root

    // The message being edited, or empty when the box writes a new message.
    property string editingId: ""
    // The text that was in the field when editing began.
    property string draft: ""
    // The edit was sent and the answer is awaited.
    property bool saving: false
    readonly property bool editing: editingId !== ""

    spacing: 4

    function send() {
        if (Room.send(field.text))
            field.clear();
    }

    // Starts editing a message of the open room. Nothing happens if its text isn't known.
    function beginEdit(messageId) {
        const text = Room.messageText(messageId);
        if (text === "")
            return;
        if (!editing)
            draft = field.text;
        editingId = messageId;
        saving = false;
        field.text = text;
        field.forceActiveFocus();
        field.cursorPosition = field.length;
    }

    // Leaves edit mode and puts the draft back.
    function endEdit() {
        editingId = "";
        saving = false;
        field.text = draft;
        draft = "";
        field.forceActiveFocus();
    }

    function save() {
        if (saving || field.length === 0)
            return;
        saving = true;
        Room.edit(editingId, field.text);
    }

    // Up in an empty box edits the user's newest message.
    function editLast() {
        const id = Room.lastEditable();
        if (id !== "")
            beginEdit(id);
    }

    Connections {
        target: Room

        // A failure stays in edit mode, with the text; the room page shows why.
        function onActionDone(messageId, action, ok, errorText) {
            if (action !== "edit" || messageId !== root.editingId)
                return;
            if (ok)
                root.endEdit();
            else
                root.saving = false;
        }

        function onRoomIdChanged() {
            if (root.editing)
                root.endEdit();
        }
    }

    RowLayout {
        objectName: "composerEditBar"
        Layout.fillWidth: true
        visible: root.editing
        spacing: 8

        Label {
            Layout.fillWidth: true
            color: root.palette.placeholderText
            font.italic: true
            text: qsTr("Editing message")
        }

        ToolButton {
            objectName: "composerEditCancel"
            text: qsTr("Cancel")
            onClicked: root.endEdit()
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        ScrollView {
            Layout.fillWidth: true
            Layout.maximumHeight: 120

            TextArea {
                id: field

                objectName: "composerField"
                placeholderText: qsTr("Message")
                readOnly: root.saving
                wrapMode: TextEdit.Wrap

                Keys.onPressed: event => {
                    if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
                        if (root.editing)
                            root.save();
                        else
                            root.send();
                        event.accepted = true;
                    } else if (event.key === Qt.Key_Escape && root.editing) {
                        root.endEdit();
                        event.accepted = true;
                    } else if (event.key === Qt.Key_Up && !root.editing && field.length === 0) {
                        root.editLast();
                        event.accepted = true;
                    }
                }
            }
        }

        Button {
            objectName: "composerSend"
            Layout.alignment: Qt.AlignBottom
            text: root.editing ? qsTr("Save") : qsTr("Send")
            enabled: field.length > 0 && !root.saving
            onClicked: root.editing ? root.save() : root.send()
        }
    }
}
