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
//
// And it quotes a message: beginQuote() shows a bar above the field with the author and the text of
// the message, and Enter sends the text as a reply to it. Escape or the bar's button drops the
// quote, and so does opening another room. Quoting and editing exclude each other.
ColumnLayout {
    id: root

    // The message being edited, or empty when the box writes a new message.
    property string editingId: ""
    // The text that was in the field when editing began.
    property string draft: ""
    // The edit was sent and the answer is awaited.
    property bool saving: false
    readonly property bool editing: editingId !== ""
    // The message being quoted, or empty when the box writes a message that quotes nothing, with
    // what the bar shows of it.
    property string quotingId: ""
    property string quotedAuthor: ""
    property string quotedText: ""
    readonly property bool quoting: quotingId !== ""

    spacing: Theme.spacingS

    function send() {
        if (Room.send(field.text, quotingId)) {
            field.clear();
            dropQuote();
        }
    }

    // Starts quoting a message of the open room, ending an edit first. Nothing happens if the
    // message isn't known.
    function beginQuote(messageId) {
        const author = Room.messageAuthor(messageId);
        if (author === "")
            return;
        if (editing)
            endEdit();
        quotingId = messageId;
        quotedAuthor = author;
        quotedText = Room.messageText(messageId);
        field.forceActiveFocus();
    }

    function dropQuote() {
        quotingId = "";
        quotedAuthor = "";
        quotedText = "";
    }

    // Stops quoting and gives the field the focus back.
    function endQuote() {
        dropQuote();
        field.forceActiveFocus();
    }

    // Starts editing a message of the open room. Nothing happens if its text isn't known.
    function beginEdit(messageId) {
        const text = Room.messageText(messageId);
        if (text === "")
            return;
        if (!editing)
            draft = field.text;
        dropQuote();
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
            root.dropQuote();
        }
    }

    RowLayout {
        objectName: "composerQuoteBar"
        // A layout with a child that fills the height would otherwise take the room's spare height.
        Layout.fillHeight: false
        Layout.fillWidth: true
        visible: root.quoting
        spacing: Theme.spacingL

        Rectangle {
            Layout.fillHeight: true
            color: root.palette.mid
            implicitWidth: Theme.accentBarWidth
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spacingXXS

            Label {
                objectName: "composerQuoteAuthor"
                font.bold: true
                text: root.quotedAuthor
                textFormat: Text.PlainText
            }

            Label {
                objectName: "composerQuoteText"
                Layout.fillWidth: true
                color: root.palette.placeholderText
                elide: Text.ElideRight
                maximumLineCount: 1
                text: root.quotedText
                textFormat: Text.PlainText
            }
        }

        ToolButton {
            objectName: "composerQuoteCancel"
            text: qsTr("Cancel")
            onClicked: root.endQuote()
        }
    }

    RowLayout {
        objectName: "composerEditBar"
        Layout.fillWidth: true
        visible: root.editing
        spacing: Theme.spacingL

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
        spacing: Theme.spacingL

        ScrollView {
            Layout.fillWidth: true
            Layout.maximumHeight: Theme.composerMaxHeight

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
                    } else if (event.key === Qt.Key_Escape && root.quoting) {
                        root.endQuote();
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
