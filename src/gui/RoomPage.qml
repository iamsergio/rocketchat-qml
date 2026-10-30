// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQml.Models
import RocketChat.Bridge

ColumnLayout {
    id: root

    // The server without its scheme or path, as the sidebar names it.
    readonly property string host: Session.host

    spacing: Theme.spacingNone

    // Opens the search of the room's messages, or focuses its field with the text selected.
    Shortcut {
        sequence: "Ctrl+F"
        onActivated: {
            ThreadList.openSearch();
            searchPane.focusField();
        }
    }

    Label {
        objectName: "connectionBanner"
        Layout.fillWidth: true
        visible: Session.connectionText !== ""
        text: Session.connectionText
        padding: Theme.spacingM
        horizontalAlignment: Text.AlignHCenter
        color: Theme.warning
        font.pointSize: Theme.fontSizeSmall
        background: Rectangle {
            color: Theme.warningWash
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Theme.spacingNone

        ColumnLayout {
            Layout.fillHeight: true
            Layout.maximumWidth: Theme.sidebarWidth
            Layout.minimumWidth: Theme.sidebarWidth
            Layout.preferredWidth: Theme.sidebarWidth
            spacing: Theme.spacingNone

            // The server: its host, and who you are there.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.sidebarHeaderPaddingH
                Layout.rightMargin: Theme.sidebarHeaderPaddingH
                Layout.topMargin: Theme.spacingXXL
                Layout.bottomMargin: Theme.spacingXL
                spacing: Theme.spacingXXS

                Label {
                    objectName: "serverName"
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeHeading
                    font.weight: Theme.fontWeightBold
                    text: root.host
                    textFormat: Text.PlainText
                }

                Label {
                    objectName: "serverUser"
                    Layout.fillWidth: true
                    color: Theme.textFaint
                    elide: Text.ElideRight
                    font.pointSize: Theme.fontSizeSmall
                    text: Session.username
                    textFormat: Text.PlainText
                }
            }

            // What matches, in place of the list below: the channels and the people the server's
            // directory found, then the user's rooms, each group under its label. Row 0 is the one
            // right above the field, and the rest pile up from there.
            ListView {
                id: searchResults

                objectName: "roomSearchResults"
                Layout.fillHeight: true
                Layout.fillWidth: true
                clip: true
                model: RoomSearch
                // The best match is right above the field, and the rest pile up from there.
                verticalLayoutDirection: ListView.BottomToTop
                visible: RoomSearch.active

                delegate: DelegateChooser {
                    role: "rowType"

                    DelegateChoice {
                        roleValue: "room"

                        RoomDelegate {
                            id: result

                            // The one Enter would open.
                            cursor: result.index === RoomSearch.cursor
                            menuEnabled: false
                            namePrefix: "searchResult_"

                            // The normal list is back, with the room open.
                            onClicked: searchField.leave(true)
                        }
                    }

                    DelegateChoice {
                        roleValue: "person"

                        DirectoryRow {
                            id: personRow

                            cursor: personRow.index === RoomSearch.cursor

                            onClicked: {
                                if (RoomSearch.open(personRow.index))
                                    searchField.leave(true);
                            }
                        }
                    }

                    DelegateChoice {
                        roleValue: "channel"

                        DirectoryRow {
                            id: channelRow

                            cursor: channelRow.index === RoomSearch.cursor

                            onClicked: {
                                if (RoomSearch.open(channelRow.index))
                                    searchField.leave(true);
                            }
                        }
                    }

                    DelegateChoice {
                        roleValue: "label"

                        DirectoryLabel {}
                    }

                    DelegateChoice {
                        roleValue: "searching"

                        DirectoryNote {}
                    }

                    DelegateChoice {
                        roleValue: "failed"

                        DirectoryNote {}
                    }

                    DelegateChoice {
                        roleValue: "more"

                        DirectoryNote {}
                    }
                }

                ScrollBar.vertical: ScrollBar {}

                // The cursor is the model's, which skips what can't be acted on.
                Connections {
                    function onCursorChanged() {
                        if (RoomSearch.cursor >= 0)
                            searchResults.positionViewAtIndex(RoomSearch.cursor, ListView.Contain);
                    }

                    target: searchResults.model
                }
            }

            // One list, so that everything scrolls together. The favorites come first, and the
            // sections show only while there are any.
            ListView {
                objectName: "roomList"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: RoomList
                visible: !RoomSearch.active
                section.criteria: ViewSection.FullString
                section.property: "favorite"
                delegate: RoomDelegate {}

                ScrollBar.vertical: ScrollBar {}

                // The value of `section` is the text of the bool: "true" for the favorites.
                section.delegate: Rectangle {
                    id: header

                    // The name of the section it heads, as `roomKind_<kind>` names a kind.
                    readonly property bool favorites: section === "true"
                    required property string section

                    clip: true
                    color: Theme.base
                    // The view decides whether a section item is visible, so without favorites it
                    // is left empty and with no height.
                    height: RoomList.hasFavorites ? headerLabel.implicitHeight + Theme.spacingXXL : 0
                    objectName: "roomSection_" + (favorites ? "favorites" : "rooms")
                    width: ListView.view.width

                    Label {
                        id: headerLabel

                        objectName: "roomSectionHeader"
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.roomRowInset + Theme.rowPaddingH
                        anchors.verticalCenter: parent.verticalCenter
                        color: Theme.textFaint
                        font.pointSize: Theme.fontSizeSmall
                        font.weight: Theme.fontWeightSemibold
                        text: header.favorites ? qsTr("Favorites") : qsTr("Rooms")
                        visible: RoomList.hasFavorites
                    }
                }
            }

            // Why the last command on a room failed, until it is dismissed or the user tries again.
            RowLayout {
                objectName: "roomListError"
                Layout.fillWidth: true
                Layout.margins: Theme.spacingL
                spacing: Theme.spacingL
                visible: RoomList.roomActionError !== ""

                Label {
                    objectName: "roomListErrorText"
                    Layout.fillWidth: true
                    color: Theme.error
                    font.pointSize: Theme.fontSizeSmall
                    text: RoomList.roomActionError
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                FlatButton {
                    objectName: "roomListErrorDismiss"
                    text: qsTr("Dismiss")
                    onClicked: RoomList.dismissRoomActionError()
                }
            }

            Label {
                objectName: "roomSearchEmpty"
                Layout.fillWidth: true
                // Under the text of the field.
                Layout.leftMargin: Theme.roomRowInset + Theme.roomNameInset
                Layout.rightMargin: Theme.roomRowInset + Theme.rowPaddingH
                color: Theme.textFaint
                elide: Text.ElideRight
                font.pointSize: Theme.fontSizeSmall
                text: qsTr("No rooms, people or channels match “%1”").arg(RoomSearch.trimmedQuery)
                textFormat: Text.PlainText
                visible: RoomSearch.active && searchResults.count === 0
            }

            // Searches the user's rooms by name, at the bottom of the sidebar. While it holds
            // something, the results replace the list and pile up from here. The focus stays here
            // while typing: Up and Down move through the results, away from the field and back.
            // Enter, a click on a result and Escape clear the field and hand the focus on.
            InputField {
                id: searchField

                objectName: "roomSearchField"
                Layout.bottomMargin: Theme.searchFieldBottomMargin
                Layout.fillWidth: true
                Layout.leftMargin: Theme.roomRowInset
                Layout.rightMargin: Theme.roomRowInset
                Layout.topMargin: Theme.spacingL
                implicitHeight: Theme.searchFieldHeight
                // The text starts where the room names do, and the magnifier is over the avatars.
                leftPadding: Theme.roomNameInset
                placeholderText: qsTr("Search rooms")
                quiet: true
                // Under neither the cross nor the hint.
                rightPadding: Theme.fieldPaddingH + Math.max(hint.width, clearButton.width)

                Keys.onDownPressed: RoomSearch.cursorDown()
                // Clears the field and gives up the focus: to the composer of the open room, or of the
                // one being opened. With no room, nobody has it.
                function leave(opened) {
                    clear();
                    if (opened)
                        roomView.focusComposerOnceOpen();
                    else if (!roomView.focusComposer())
                        focus = false;
                }

                Keys.onEscapePressed: leave(false)
                Keys.onUpPressed: RoomSearch.cursorUp()
                onAccepted: {
                    if (RoomSearch.openCursor())
                        leave(true);
                }
                onTextChanged: RoomSearch.setQuery(text)

                Icon {
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.roomAvatarCenter - Theme.searchIconSize / 2
                    anchors.verticalCenter: parent.verticalCenter
                    height: Theme.searchIconSize
                    source: "icons/search.svg"
                    tint: Theme.textFaint
                    width: Theme.searchIconSize
                }

                // Clears the field and leaves the focus where it is.
                ToolButton {
                    id: clearButton

                    objectName: "roomSearchClear"
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacingM
                    anchors.verticalCenter: parent.verticalCenter
                    display: AbstractButton.IconOnly
                    focusPolicy: Qt.NoFocus
                    implicitHeight: Theme.searchClearSize
                    implicitWidth: Theme.searchClearSize
                    padding: Theme.spacingNone
                    text: qsTr("Clear the search")
                    visible: searchField.text !== ""
                    onClicked: searchField.clear()

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

                // The shortcut, in a keycap, while the field is at rest.
                Rectangle {
                    id: hint

                    objectName: "roomSearchHint"
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacingXL
                    anchors.verticalCenter: parent.verticalCenter
                    border.color: Theme.raisedHover
                    border.width: Theme.borderWidth
                    color: Theme.transparent
                    height: hintLabel.implicitHeight + 2 * Theme.searchHintPaddingV
                    radius: Theme.radiusCheckBox
                    visible: searchField.text === "" && !searchField.activeFocus
                    width: hintLabel.implicitWidth + 2 * Theme.searchHintPaddingH

                    Label {
                        id: hintLabel

                        anchors.centerIn: parent
                        color: Theme.textFaint
                        font.pointSize: Theme.fontSizeTiny
                        text: qsTr("Ctrl K")
                    }
                }

                Shortcut {
                    sequence: "Ctrl+K"
                    onActivated: searchField.forceActiveFocus()
                }

                // F5 shows the news the batch holds back. The room list says when there is none.
                Shortcut {
                    sequence: "F5"
                    onActivated: RoomList.releaseNews()
                }

                Connections {
                    function onNothingHeld() {
                        toast.shown = true;
                        toastTimer.restart();
                    }

                    target: RoomList as QtObject
                }

                // Above the field, centred in the sidebar (the field's margins are equal). It takes
                // no mouse events and no focus.
                Label {
                    id: toast

                    property bool shown: false

                    objectName: "noNewsToast"
                    anchors.bottom: parent.top
                    anchors.bottomMargin: Theme.toastGap
                    anchors.horizontalCenter: parent.horizontalCenter
                    bottomPadding: Theme.spacingM
                    color: Theme.text
                    font.pointSize: Theme.fontSizeSmall
                    leftPadding: Theme.spacingXL
                    opacity: shown ? Theme.opaque : Theme.clear
                    rightPadding: Theme.spacingXL
                    text: qsTr("No new notifications")
                    textFormat: Text.PlainText
                    topPadding: Theme.spacingM
                    visible: opacity > Theme.clear
                    z: Theme.zAbove

                    background: Rectangle {
                        color: Theme.raisedHover
                        radius: Theme.radiusControl
                    }

                    Behavior on opacity {
                        NumberAnimation {
                            duration: Theme.toastFadeDuration
                        }
                    }

                    Timer {
                        id: toastTimer

                        interval: Theme.toastDuration
                        onTriggered: toast.shown = false
                    }
                }
            }
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: Theme.separatorWidth
            color: Theme.line
        }

        RoomView {
            id: roomView

            Layout.fillWidth: true
            Layout.fillHeight: true
        }

        ThreadPane {
            Layout.fillHeight: true
            Layout.preferredWidth: Theme.threadPaneWidth
            visible: ThreadList.paneOpen
        }

        // The search of the room's messages takes the place of the threads.
        MessageSearchPane {
            id: searchPane

            Layout.fillHeight: true
            Layout.preferredWidth: Theme.threadPaneWidth
            visible: ThreadList.searchOpen
        }
    }
}
