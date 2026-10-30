// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQml.Models
import RocketChat.Bridge

// The pane beside the room while it searches the room's messages, in the place of the threads: a
// field, the messages found grouped by day, and the keys. What it shows is decided in Rust
// (`MessageSearch`); RoomPage shows the pane while `ThreadList.pane` is `search`.
Rectangle {
    id: root

    // Focuses the field with its text selected, as Ctrl+F does.
    function focusField() {
        field.forceActiveFocus();
        field.selectAll();
    }

    // The title of a note in the list area, and the hint below it.
    component Note: ColumnLayout {
        property alias hint: noteHint.text
        property alias hintName: noteHint.objectName
        property alias title: noteTitle.text
        property alias titleName: noteTitle.objectName

        anchors.left: parent.left
        anchors.margins: Theme.spacingXXXL
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacingL

        Label {
            id: noteTitle

            Layout.fillWidth: true
            font.weight: Theme.fontWeightSemibold
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        Label {
            id: noteHint

            Layout.fillWidth: true
            color: Theme.textFaint
            font.pointSize: Theme.fontSizeSmall
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }

    color: Theme.surface
    objectName: "messageSearchPane"
    // Whenever the pane appears, the field is where the user types.
    onVisibleChanged: {
        if (visible)
            focusField();
    }

    Connections {
        // The pane closed: the next search starts from an empty field.
        function onSearchOpenChanged() {
            if (!ThreadList.searchOpen)
                field.clear();
        }

        target: ThreadList
    }

    Connections {
        // The room changed under the pane: it searches nothing in the new one.
        function onFieldReset() {
            field.clear();
        }

        target: MessageSearch as QtObject
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

        // The title and the room, over a hairline: as the header of the threads.
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
                        objectName: "messageSearchTitle"
                        Layout.alignment: Qt.AlignBaseline
                        font.pointSize: Theme.fontSizeHeading
                        font.weight: Theme.fontWeightSemibold
                        text: qsTr("Search")
                    }

                    Label {
                        objectName: "messageSearchRoom"
                        Layout.alignment: Qt.AlignBaseline
                        Layout.fillWidth: true
                        color: Theme.textMuted
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeSmall
                        text: qsTr("in #%1").arg(MessageSearch.roomName)
                        textFormat: Text.PlainText
                    }
                }

                ToolButton {
                    id: close

                    objectName: "messageSearchClose"
                    // Only the tool tip and the accessibility name show it.
                    display: AbstractButton.IconOnly
                    implicitHeight: Theme.iconButtonSize
                    implicitWidth: Theme.iconButtonSize
                    padding: Theme.spacingNone
                    text: qsTr("Close the search")
                    onClicked: ThreadList.closePane()

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

        // What to look for. The focus stays here while the results are browsed: Up and Down move
        // the cursor, Enter shows the message under it, Escape closes the pane.
        TextField {
            id: field

            objectName: "messageSearchField"
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingXXXL
            Layout.rightMargin: Theme.spacingXXXL
            Layout.topMargin: Theme.spacingXL
            bottomPadding: Theme.spacingNone
            color: Theme.text
            implicitHeight: Theme.fieldHeight
            // Right of the magnifier, and left of the cross.
            leftPadding: Theme.fieldPaddingH + Theme.searchIconSize + Theme.spacingL
            placeholderText: qsTr("Search messages")
            placeholderTextColor: Theme.textFaint
            rightPadding: Theme.fieldPaddingH + (clearButton.visible ? clearButton.width : Theme.spacingNone)
            topPadding: Theme.spacingNone
            verticalAlignment: TextInput.AlignVCenter

            background: Rectangle {
                border.color: field.activeFocus ? Theme.ownBorder : Theme.line
                border.width: Theme.borderWidth
                color: Theme.base
                radius: Theme.radiusControl
            }

            Keys.onDownPressed: MessageSearch.cursorDown()
            Keys.onEscapePressed: ThreadList.escape()
            Keys.onUpPressed: MessageSearch.cursorUp()
            onAccepted: MessageSearch.pick(-1)
            onTextChanged: MessageSearch.setQuery(text)

            Icon {
                anchors.left: parent.left
                anchors.leftMargin: Theme.fieldPaddingH
                anchors.verticalCenter: parent.verticalCenter
                height: Theme.searchIconSize
                source: "icons/search.svg"
                tint: Theme.textFaint
                width: Theme.searchIconSize
            }

            // Clears the field and leaves the focus where it is.
            ToolButton {
                id: clearButton

                objectName: "messageSearchClear"
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacingM
                anchors.verticalCenter: parent.verticalCenter
                display: AbstractButton.IconOnly
                focusPolicy: Qt.NoFocus
                implicitHeight: Theme.searchClearSize
                implicitWidth: Theme.searchClearSize
                padding: Theme.spacingNone
                text: qsTr("Clear the search")
                visible: field.text !== ""
                onClicked: field.clear()

                ToolTip.delay: Theme.toolTipDelay
                ToolTip.text: text
                ToolTip.visible: hovered

                background: Rectangle {
                    color: clearButton.hovered ? Theme.raisedHover : Theme.transparent
                    radius: Theme.radiusControl
                }

                contentItem: Item {
                    Icon {
                        anchors.centerIn: parent
                        height: Theme.searchClearIconSize
                        source: "icons/close.svg"
                        tint: clearButton.hovered ? Theme.text : Theme.textMuted
                        width: Theme.searchClearIconSize
                    }
                }
            }
        }

        // What the search matches; "Searching…" with a spinner while the rows are being replaced.
        RowLayout {
            Layout.bottomMargin: Theme.spacingL
            Layout.leftMargin: Theme.spacingXXXL + Theme.fieldPaddingH
            Layout.topMargin: Theme.spacingM
            spacing: Theme.spacingM

            Label {
                objectName: "messageSearchHint"
                color: Theme.textFaint
                font.pointSize: Theme.fontSizeTiny
                text: MessageSearch.fieldHint
                textFormat: Text.PlainText
            }

            BusyIndicator {
                objectName: "messageSearchRefreshing"
                Layout.preferredHeight: Theme.messageSearchSpinnerSize
                Layout.preferredWidth: Theme.messageSearchSpinnerSize
                implicitHeight: Theme.messageSearchSpinnerSize
                implicitWidth: Theme.messageSearchSpinnerSize
                padding: Theme.spacingNone
                palette.dark: Theme.textMuted
                running: visible
                visible: MessageSearch.refreshing
            }
        }

        // Why nothing happened in the room: the message picked is gone.
        Label {
            objectName: "messageSearchMissing"
            Layout.bottomMargin: Theme.spacingL
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingXXXL + Theme.fieldPaddingH
            Layout.rightMargin: Theme.spacingXXXL
            color: Theme.error
            font.pointSize: Theme.fontSizeSmall
            text: MessageSearch.missingNote
            textFormat: Text.PlainText
            visible: text !== ""
            wrapMode: Text.Wrap
        }

        Item {
            Layout.fillHeight: true
            Layout.fillWidth: true

            ListView {
                id: results

                objectName: "messageSearchList"
                anchors.fill: parent
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                model: MessageSearch

                delegate: DelegateChooser {
                    role: "rowType"

                    DelegateChoice {
                        roleValue: "day"

                        MessageSearchDay {}
                    }

                    DelegateChoice {
                        roleValue: "message"

                        MessageSearchRow {}
                    }
                }

                ScrollBar.vertical: ScrollBar {}

                // Under the last row: a spinner while the next page loads, or why it could not.
                footer: Item {
                    // The list's content item, as wide as the list (see RoomView).
                    height: moreFailed.visible ? moreFailed.implicitHeight + 2 * Theme.spacingL : moreBusy.visible ? Theme.historyFooterHeight : Theme.spacingNone
                    width: parent?.width ?? 0

                    BusyIndicator {
                        id: moreBusy

                        objectName: "messageSearchMoreBusy"
                        anchors.centerIn: parent
                        height: Theme.busyIndicatorSize
                        palette.dark: Theme.textMuted
                        running: visible
                        visible: MessageSearch.footer === "loading"
                    }

                    RowLayout {
                        id: moreFailed

                        anchors.left: parent.left
                        anchors.leftMargin: Theme.spacingXXXL
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.spacingXXXL
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: Theme.spacingL
                        visible: MessageSearch.footer === "failed"

                        Label {
                            objectName: "messageSearchMoreError"
                            Layout.fillWidth: true
                            color: Theme.error
                            font.pointSize: Theme.fontSizeSmall
                            text: MessageSearch.footerError
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }

                        FlatButton {
                            objectName: "messageSearchMoreRetry"
                            focusPolicy: Qt.NoFocus
                            text: qsTr("Retry")
                            onClicked: MessageSearch.retryMore()
                        }
                    }
                }

                // Rust asks for the next page if there is one; a page that does not fill the list
                // leaves the end in view.
                onAtYEndChanged: {
                    if (atYEnd)
                        MessageSearch.reachedEnd();
                }
                onContentHeightChanged: {
                    if (atYEnd)
                        MessageSearch.reachedEnd();
                }

                // The cursor is the model's, which skips the day labels.
                Connections {
                    function onCursorChanged() {
                        if (MessageSearch.cursor >= 0)
                            results.positionViewAtIndex(MessageSearch.cursor, ListView.Contain);
                    }

                    target: MessageSearch as QtObject
                }
            }

            // The first search runs and there is nothing to show meanwhile.
            BusyIndicator {
                objectName: "messageSearchLoading"
                anchors.centerIn: parent
                implicitHeight: Theme.busyIndicatorLargeSize
                implicitWidth: Theme.busyIndicatorLargeSize
                palette.dark: Theme.textMuted
                running: visible
                visible: MessageSearch.status === "searching"
            }

            // What to do, while the field is empty.
            Note {
                hint: MessageSearch.noteHint
                hintName: "messageSearchEmptyHint"
                objectName: "messageSearchEmpty"
                title: MessageSearch.noteTitle
                titleName: "messageSearchEmptyTitle"
                visible: MessageSearch.status === "empty"
            }

            // Nothing has the words.
            Note {
                hint: MessageSearch.noteHint
                hintName: "messageSearchNoResultsHint"
                objectName: "messageSearchNoResults"
                title: MessageSearch.noteTitle
                titleName: "messageSearchNoResultsTitle"
                visible: MessageSearch.status === "none"
            }

            // The first page could not be fetched.
            ColumnLayout {
                anchors.left: parent.left
                anchors.margins: Theme.spacingXXXL
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spacingXL
                visible: MessageSearch.status === "failed"

                Label {
                    objectName: "messageSearchFailed"
                    Layout.fillWidth: true
                    color: Theme.error
                    horizontalAlignment: Text.AlignHCenter
                    text: MessageSearch.error
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                FlatButton {
                    objectName: "messageSearchRetry"
                    Layout.alignment: Qt.AlignHCenter
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Retry")
                    onClicked: MessageSearch.retry()
                }
            }
        }

        // The keys, under a hairline.
        Rectangle {
            Layout.fillWidth: true
            color: Theme.line
            implicitHeight: Theme.separatorWidth
        }

        Flow {
            objectName: "messageSearchKeys"
            Layout.bottomMargin: Theme.mentionHintPaddingV
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingXXXL
            Layout.rightMargin: Theme.spacingXXXL
            Layout.topMargin: Theme.mentionHintPaddingV
            spacing: Theme.spacingXXL

            Row {
                spacing: Theme.mentionHintSpacing

                Keycap {
                    text: "↑↓"
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
                    text: qsTr("show in room")
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
