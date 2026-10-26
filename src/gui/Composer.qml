// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The box under the messages of the open room, or of the open thread (`forThread`). Enter sends,
// Shift+Enter starts a new line.
//
// It also edits a message of the user's: beginEdit() puts the message's text in the field, a bar
// above it says so, and Enter saves instead of sending. Escape, the bar's button, or opening
// another room cancels. The draft that was in the field before editing comes back when editing
// ends, by saving or cancelling.
//
// And it quotes a message: beginQuote() shows a bar above the field with the author and the text of
// the message, and Enter sends the text as a reply to it. Escape or the bar's button drops the
// quote, and so does opening another room. Quoting and editing exclude each other.
//
// For a thread it replies in the open thread, with a check box to send the reply to the room too,
// and Escape with nothing to cancel goes back to the list of threads. The edits, deletions and reactions go
// through Room in both cases.
Rectangle {
    id: root

    // The box writes in the open thread and not in the room.
    property bool forThread: false
    // Where the messages it reads are: the thread's view model, or the room's.
    readonly property var target: forThread ? Thread : Room

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
    // The room is a channel previewed without being in it: the box offers to join, not to send.
    readonly property bool previewing: !forThread && Room.preview

    border.color: field.activeFocus ? Theme.ownBorder : Theme.raisedHover
    border.width: Theme.borderWidth
    color: Theme.raised
    implicitHeight: column.implicitHeight + 2 * Theme.borderWidth
    radius: Theme.radiusPanel

    function send() {
        if (forThread ? Thread.send(field.text, quotingId, alsoToChannel.checked) : Room.send(field.text, quotingId)) {
            field.clear();
            dropQuote();
            alsoToChannel.checked = false;
        }
    }

    // Gives the field the focus.
    function focusField() {
        field.forceActiveFocus();
    }

    // Starts quoting a message of the open room, ending an edit first. Nothing happens if the
    // message isn't known.
    function beginQuote(messageId) {
        const author = target.messageAuthor(messageId);
        if (author === "")
            return;
        if (editing)
            endEdit();
        quotingId = messageId;
        quotedAuthor = author;
        quotedText = target.messageText(messageId);
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
        const text = target.messageText(messageId);
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
        const id = target.lastEditable();
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

    // Another thread is another conversation: what was started for the last one is dropped.
    Connections {
        function onParentIdChanged() {
            if (root.editing)
                root.endEdit();
            root.dropQuote();
            alsoToChannel.checked = false;
        }

        enabled: root.forThread
        target: Thread
    }

    // Inside the border.
    ColumnLayout {
        id: column

        anchors.fill: parent
        anchors.margins: Theme.borderWidth
        spacing: Theme.spacingNone

        // The bars above the field, each over a hairline.
        ColumnLayout {
            objectName: "composerQuoteBar"
            Layout.fillWidth: true
            spacing: Theme.spacingNone
            visible: root.quoting

            RowLayout {
                // A layout with a child that fills the height would otherwise take the room's spare
                // height.
                Layout.fillHeight: false
                Layout.fillWidth: true
                Layout.bottomMargin: Theme.spacingL
                Layout.leftMargin: Theme.composerPaddingH
                Layout.rightMargin: Theme.spacingXL
                Layout.topMargin: Theme.spacingL
                spacing: Theme.spacingXL

                Rectangle {
                    Layout.fillHeight: true
                    color: Theme.own
                    implicitWidth: Theme.quoteBarWidth
                    radius: Theme.quoteBarWidth
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingXXS

                    Label {
                        objectName: "composerQuoteAuthor"
                        font.pointSize: Theme.fontSizeSmall
                        font.weight: Theme.fontWeightSemibold
                        text: root.quotedAuthor
                        textFormat: Text.PlainText
                    }

                    Label {
                        objectName: "composerQuoteText"
                        Layout.fillWidth: true
                        color: Theme.textMuted
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeSmall
                        maximumLineCount: 1
                        text: root.quotedText
                        textFormat: Text.PlainText
                    }
                }

                FlatButton {
                    objectName: "composerQuoteCancel"
                    text: qsTr("Cancel")
                    onClicked: root.endQuote()
                }
            }

            Rectangle {
                Layout.fillWidth: true
                color: Theme.raisedHover
                implicitHeight: Theme.separatorWidth
            }
        }

        ColumnLayout {
            objectName: "composerEditBar"
            Layout.fillWidth: true
            spacing: Theme.spacingNone
            visible: root.editing

            RowLayout {
                Layout.fillWidth: true
                Layout.bottomMargin: Theme.spacingL
                Layout.leftMargin: Theme.composerPaddingH
                Layout.rightMargin: Theme.spacingXL
                Layout.topMargin: Theme.spacingL
                spacing: Theme.spacingXL

                Rectangle {
                    Layout.fillHeight: true
                    color: Theme.own
                    implicitWidth: Theme.quoteBarWidth
                    radius: Theme.quoteBarWidth
                }

                Label {
                    Layout.fillWidth: true
                    color: Theme.textMuted
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("Editing message")
                }

                FlatButton {
                    objectName: "composerEditCancel"
                    text: qsTr("Cancel")
                    onClicked: root.endEdit()
                }
            }

            Rectangle {
                Layout.fillWidth: true
                color: Theme.raisedHover
                implicitHeight: Theme.separatorWidth
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.bottomMargin: Theme.spacingL
            Layout.leftMargin: Theme.composerPaddingH
            Layout.rightMargin: Theme.spacingL
            Layout.topMargin: Theme.spacingL
            spacing: Theme.spacingM

            ScrollView {
                Layout.fillWidth: true
                Layout.maximumHeight: Theme.composerMaxHeight
                Layout.minimumHeight: Theme.composerFieldMinHeight

                TextArea {
                    id: field

                    objectName: "composerField"
                    // The box draws the background and the border.
                    background: null
                    bottomPadding: Theme.textAreaPaddingV
                    leftPadding: Theme.spacingNone
                    placeholderText: root.forThread ? qsTr("Reply in the thread") : qsTr("Message")
                    readOnly: root.saving
                    rightPadding: Theme.spacingNone
                    topPadding: Theme.textAreaPaddingV
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
                        } else if (event.key === Qt.Key_Escape && root.forThread) {
                            ThreadList.escape();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Up && !root.editing && field.length === 0) {
                            root.editLast();
                            event.accepted = true;
                        }
                    }
                }
            }

            PrimaryButton {
                objectName: "composerSend"
                Layout.alignment: Qt.AlignBottom
                text: root.editing ? qsTr("Save") : qsTr("Send")
                enabled: field.length > 0 && !root.saving
                visible: !root.previewing
                onClicked: root.editing ? root.save() : root.send()
            }

            // In place of Send while the room is a channel the user isn't in: what is typed waits
            // in the field.
            PrimaryButton {
                objectName: "composerJoin"
                Layout.alignment: Qt.AlignBottom
                enabled: !Room.joining
                text: Room.joining ? qsTr("Joining…") : qsTr("Join")
                visible: root.previewing
                onClicked: Room.join()
            }
        }

        // Under the text, inside the box: also shows the reply in the room. Off for each reply.
        CheckBox {
            id: alsoToChannel

            objectName: "composerAlsoToChannel"
            Layout.bottomMargin: Theme.spacingL
            // As wide as its text, and no wider than the box: the room's name may be long. The
            // layout rounds the width down, so the ceiling keeps a name that fits from eliding.
            Layout.fillWidth: true
            Layout.leftMargin: Theme.composerPaddingH
            Layout.maximumWidth: Math.ceil(implicitWidth)
            Layout.rightMargin: Theme.composerPaddingH
            // The content sits right of the box.
            leftPadding: Theme.checkBoxSize + Theme.spacingL
            padding: Theme.spacingNone
            visible: root.forThread

            contentItem: RowLayout {
                spacing: Theme.spacingS

                Label {
                    color: Theme.textMuted
                    font.pointSize: Theme.fontSizeSmall
                    text: qsTr("Also send to")
                }

                Label {
                    Layout.fillWidth: true
                    color: Theme.text
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeSmall
                    font.weight: Theme.fontWeightSemibold
                    text: "#" + Thread.roomName
                    textFormat: Text.PlainText
                }
            }

            indicator: Rectangle {
                x: Theme.spacingNone
                y: (alsoToChannel.height - height) / 2
                border.color: alsoToChannel.checked ? Theme.own : Theme.raisedHover
                border.width: Theme.borderWidth
                color: alsoToChannel.checked ? Theme.own : Theme.surface
                implicitHeight: Theme.checkBoxSize
                implicitWidth: Theme.checkBoxSize
                radius: Theme.radiusCheckBox

                Icon {
                    anchors.centerIn: parent
                    height: Theme.checkMarkSize
                    source: "icons/check.svg"
                    tint: Theme.textOnOwn
                    visible: alsoToChannel.checked
                    width: Theme.checkMarkSize
                }
            }
        }
    }
}
