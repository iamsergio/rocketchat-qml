// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
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
// It sends files too (ACT-009): the paperclip, or files dropped on the pane (RoomView and
// ThreadPane call attachUrls), put them in a bar above the field, where each can be taken out. Send
// uploads them in order with the text as the message of the first one, and the uploads under way
// are listed in another bar until their messages are posted. Opening another room drops the files
// that wait; the uploads go on in the core.
//
// Typing an @word opens the popup of the room's members above it (MentionPopup, ACT-013): the
// field forwards its text and cursor to Mentions, which decides, and its keys while it is open.
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
    // The files that wait for Send, and the files being uploaded to the room or the thread.
    readonly property var staged: forThread ? ThreadStagedFiles : RoomStagedFiles
    readonly property var uploads: forThread ? ThreadUploads : RoomUploads

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

    // The name Mentions knows this composer by.
    readonly property string mentionOwner: forThread ? "thread" : "room"

    // Tells Mentions what the field holds. Called after the event loop turns, so that the change of
    // the text and the one of the cursor that go together are seen as one.
    function syncMentions() {
        if (field.activeFocus && !root.previewing)
            Mentions.update(mentionOwner, field.text, field.cursorPosition);
    }

    function send() {
        const sent = root.staged.count > 0 ? root.staged.submit(field.text, quotingId, alsoToChannel.checked) : forThread ? Thread.send(field.text, quotingId, alsoToChannel.checked) : Room.send(field.text, quotingId);
        if (sent) {
            field.clear();
            dropQuote();
            alsoToChannel.checked = false;
        }
    }

    // Puts the files the URLs name in the bar of the files that wait.
    function attachUrls(urls) {
        for (const url of urls)
            root.staged.add(url);
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
            Mentions.close(root.mentionOwner);
            if (root.editing)
                root.endEdit();
            root.dropQuote();
            root.staged.clear();
        }
    }

    // Another thread is another conversation: what was started for the last one is dropped.
    Connections {
        function onParentIdChanged() {
            Mentions.close(root.mentionOwner);
            if (root.editing)
                root.endEdit();
            root.dropQuote();
            alsoToChannel.checked = false;
            root.staged.clear();
        }

        enabled: root.forThread
        target: Thread
    }

    // The popup's choice goes into the field as the replacement of the word, which the field's
    // undo stack follows.
    Connections {
        function onInserted(start, end, replacement, cursor) {
            if (Mentions.owner !== root.mentionOwner)
                return;
            field.remove(start, end);
            field.insert(start, replacement);
            field.cursorPosition = cursor;
        }

        target: Mentions as QtObject
    }

    MentionPopup {
        owner: root.mentionOwner
    }

    FileDialog {
        id: fileDialog

        fileMode: FileDialog.OpenFiles
        title: qsTr("Attach files")
        onAccepted: root.attachUrls(selectedFiles)
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

        // The files that wait for Send, over a hairline.
        ColumnLayout {
            objectName: "composerAttachments"
            Layout.fillWidth: true
            spacing: Theme.spacingNone
            visible: root.staged.count > 0

            Flow {
                Layout.fillWidth: true
                Layout.bottomMargin: Theme.spacingL
                Layout.leftMargin: Theme.composerPaddingH
                Layout.rightMargin: Theme.composerPaddingH
                Layout.topMargin: Theme.spacingL
                spacing: Theme.spacingL

                Repeater {
                    model: root.staged

                    StagedFileChip {
                        onRemoveRequested: root.staged.remove(index)
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                color: Theme.raisedHover
                implicitHeight: Theme.separatorWidth
            }
        }

        // The files on their way, over a hairline.
        ColumnLayout {
            objectName: "composerUploads"
            Layout.fillWidth: true
            spacing: Theme.spacingNone
            visible: root.uploads.count > 0

            ColumnLayout {
                Layout.fillWidth: true
                Layout.bottomMargin: Theme.spacingL
                Layout.leftMargin: Theme.composerPaddingH
                Layout.rightMargin: Theme.composerPaddingH
                Layout.topMargin: Theme.spacingL
                spacing: Theme.spacingXL

                Repeater {
                    model: root.uploads

                    UploadRow {
                        Layout.fillWidth: true
                        onCancelRequested: root.uploads.cancel(index)
                        onRetryRequested: root.uploads.retry(index)
                    }
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

            // The paperclip: opens the system dialog to pick files (or takes the ones a test
            // names, see StagedFilesModel).
            ToolButton {
                id: attach

                objectName: "composerAttach"
                Layout.alignment: Qt.AlignBottom
                display: AbstractButton.IconOnly
                implicitHeight: Theme.iconButtonSize
                implicitWidth: Theme.iconButtonSize
                padding: Theme.spacingNone
                text: qsTr("Attach files")
                visible: !root.editing && !root.previewing
                onClicked: {
                    if (!root.staged.addTestFiles())
                        fileDialog.open();
                }

                ToolTip.delay: Theme.toolTipDelay
                ToolTip.text: text
                ToolTip.visible: hovered

                background: Rectangle {
                    color: attach.hovered ? Theme.raisedHover : Theme.transparent
                    radius: Theme.radiusControl
                }

                contentItem: Item {
                    Icon {
                        anchors.centerIn: parent
                        height: Theme.iconButtonIconSize
                        source: "icons/attach.svg"
                        tint: Theme.textMuted
                        width: Theme.iconButtonIconSize
                    }
                }
            }

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

                    onActiveFocusChanged: {
                        if (activeFocus)
                            Qt.callLater(root.syncMentions);
                        else
                            Mentions.close(root.mentionOwner);
                    }
                    onCursorPositionChanged: Qt.callLater(root.syncMentions)
                    onTextChanged: Qt.callLater(root.syncMentions)

                    Keys.onPressed: event => {
                        // What the popup shows now, not as of the last turn of the event loop.
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Tab || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Escape)
                            root.syncMentions();
                        const popup = Mentions.open && Mentions.owner === root.mentionOwner;
                        if (popup && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Tab) && !(event.modifiers & Qt.ShiftModifier) && Mentions.cursor >= 0) {
                            Mentions.acceptCursor();
                            event.accepted = true;
                        } else if (popup && event.key === Qt.Key_Up && Mentions.cursor >= 0) {
                            Mentions.cursorUp();
                            event.accepted = true;
                        } else if (popup && event.key === Qt.Key_Down && Mentions.cursor >= 0) {
                            Mentions.cursorDown();
                            event.accepted = true;
                        } else if (popup && event.key === Qt.Key_Escape) {
                            Mentions.escape();
                            event.accepted = true;
                        } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) {
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
                        } else if (event.matches(StandardKey.Paste) && !root.editing && !root.previewing && root.staged.paste()) {
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
                enabled: (field.length > 0 || (root.staged.count > 0 && !root.editing)) && !root.saving
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
