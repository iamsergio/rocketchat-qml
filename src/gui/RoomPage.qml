// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import RocketChat.Bridge

ColumnLayout {
    spacing: 0

    ToolBar {
        Layout.fillWidth: true

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.spacingXXL
            anchors.rightMargin: Theme.spacingXXL

            Label {
                objectName: "currentUser"
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: qsTr("%1 on %2").arg(Session.username).arg(Session.server)
            }

            ToolButton {
                objectName: "logoutButton"
                text: qsTr("Log out")
                onClicked: Session.logout()
            }
        }
    }

    Label {
        objectName: "connectionBanner"
        Layout.fillWidth: true
        visible: Session.connectionText !== ""
        text: Session.connectionText
        padding: Theme.spacingM
        horizontalAlignment: Text.AlignHCenter
        color: palette.toolTipText
        background: Rectangle {
            color: palette.toolTipBase
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 0

        ColumnLayout {
            Layout.fillHeight: true
            Layout.maximumWidth: Theme.sidebarWidth
            Layout.minimumWidth: Theme.sidebarWidth
            Layout.preferredWidth: Theme.sidebarWidth
            spacing: 0

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

                // The value of `section` is the text of the bool: "true" for the favorites.
                section.delegate: Rectangle {
                    id: header

                    // The name of the section it heads, as `roomKind_<kind>` names a kind.
                    readonly property bool favorites: section === "true"
                    required property string section

                    clip: true
                    color: palette.window
                    // The view decides whether a section item is visible, so without favorites it
                    // is left empty and with no height.
                    height: RoomList.hasFavorites ? headerLabel.implicitHeight + Theme.spacingL : 0
                    objectName: "roomSection_" + (favorites ? "favorites" : "rooms")
                    width: ListView.view.width

                    Label {
                        id: headerLabel

                        objectName: "roomSectionHeader"
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.spacingXXL
                        anchors.verticalCenter: parent.verticalCenter
                        color: palette.placeholderText
                        font.bold: true
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
                visible: RoomList.roomActionError !== ""

                Label {
                    objectName: "roomListErrorText"
                    Layout.fillWidth: true
                    color: Theme.error
                    text: RoomList.roomActionError
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                ToolButton {
                    objectName: "roomListErrorDismiss"
                    text: qsTr("Dismiss")
                    onClicked: RoomList.dismissRoomActionError()
                }
            }
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: Theme.separatorWidth
            color: palette.mid
        }

        RoomView {
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
    }
}
