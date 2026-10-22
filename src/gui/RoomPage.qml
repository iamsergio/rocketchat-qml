// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

ColumnLayout {
    id: root

    // The server without its scheme or path, as the sidebar names it.
    readonly property string host: Session.host

    spacing: Theme.spacingNone

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

            // One list, so that everything scrolls together. The favorites come first, and the
            // sections show only while there are any.
            ListView {
                objectName: "roomList"
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: RoomList
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
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: Theme.separatorWidth
            color: Theme.line
        }

        RoomView {
            Layout.fillWidth: true
            Layout.fillHeight: true
        }

        ThreadPane {
            Layout.fillHeight: true
            Layout.preferredWidth: Theme.threadPaneWidth
            visible: Thread.isOpen
        }
    }
}
