// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

ColumnLayout {
    id: root

    // The server without its scheme or path, as the sidebar names it.
    readonly property string host: String(Session.server).replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").replace(/\/.*$/, "")

    // The first letter of the host, for its mark: none for an IP address, which starts with a digit.
    readonly property string hostInitial: {
        const first = host.charAt(0);
        return first.toLowerCase() !== first.toUpperCase() ? first.toUpperCase() : "";
    }

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

            // The server: its initial on the colour of the brand, and its name.
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Theme.spacingXXL
                spacing: Theme.spacingXL

                Rectangle {
                    Layout.preferredHeight: Theme.serverMarkSize
                    Layout.preferredWidth: Theme.serverMarkSize
                    color: Theme.signal
                    radius: Theme.radiusControl
                    visible: root.hostInitial !== ""

                    Label {
                        anchors.centerIn: parent
                        color: Theme.textOnSignal
                        font.weight: Theme.fontWeightBold
                        text: root.hostInitial
                        textFormat: Text.PlainText
                    }
                }

                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    font.weight: Theme.fontWeightSemibold
                    text: root.host
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

            Rectangle {
                Layout.fillWidth: true
                color: Theme.line
                implicitHeight: Theme.separatorWidth
            }

            // Who is logged in, where, and the way out.
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.spacingXXL
                Layout.rightMargin: Theme.spacingXXL
                Layout.topMargin: Theme.spacingXL
                Layout.bottomMargin: Theme.spacingXL
                spacing: Theme.spacingXL

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingNone

                    Label {
                        objectName: "currentUser"
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.weight: Theme.fontWeightSemibold
                        text: Session.username
                        textFormat: Text.PlainText
                    }

                    Label {
                        Layout.fillWidth: true
                        color: Theme.textFaint
                        elide: Text.ElideRight
                        font.pointSize: Theme.fontSizeSmall
                        text: root.host
                        textFormat: Text.PlainText
                    }
                }

                ToolButton {
                    id: settings

                    objectName: "settingsButton"
                    // Only the tool tip and the accessibility name show it.
                    display: AbstractButton.IconOnly
                    implicitHeight: Theme.iconButtonSize
                    implicitWidth: Theme.iconButtonSize
                    padding: Theme.spacingNone
                    text: qsTr("Settings")
                    // Above the button: it is at the bottom of the window.
                    onClicked: settingsMenu.popup(this, 0, -settingsMenu.implicitHeight)

                    ToolTip.delay: Theme.toolTipDelay
                    ToolTip.text: text
                    ToolTip.visible: hovered && !settingsMenu.visible

                    background: Rectangle {
                        color: settings.hovered ? Theme.raised : Theme.transparent
                        radius: Theme.radiusControl
                    }

                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            height: Theme.iconButtonIconSize
                            source: "icons/settings.svg"
                            tint: settings.hovered ? Theme.text : Theme.textMuted
                            width: Theme.iconButtonIconSize
                        }
                    }

                    // The popup is parented to the window's overlay while open, so Spix finds its
                    // items by name from the window (mainWindow/dumpInfoAction), and only while it
                    // is open.
                    Menu {
                        id: settingsMenu

                        // Keeps the menu inside the window when it opens near an edge.
                        margins: Theme.spacingL

                        MenuItem {
                            objectName: "dumpInfoAction"
                            text: qsTr("Dump info")
                            onTriggered: DebugTools.dumpInfo()
                        }
                    }
                }

                ToolButton {
                    id: logout

                    objectName: "logoutButton"
                    // Only the tool tip and the accessibility name show it.
                    display: AbstractButton.IconOnly
                    implicitHeight: Theme.iconButtonSize
                    implicitWidth: Theme.iconButtonSize
                    padding: Theme.spacingNone
                    text: qsTr("Log out")
                    onClicked: Session.logout()

                    ToolTip.delay: Theme.toolTipDelay
                    ToolTip.text: text
                    ToolTip.visible: hovered

                    background: Rectangle {
                        color: logout.hovered ? Theme.raised : Theme.transparent
                        radius: Theme.radiusControl
                    }

                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            height: Theme.iconButtonIconSize
                            source: "icons/logout.svg"
                            tint: logout.hovered ? Theme.text : Theme.textMuted
                            width: Theme.iconButtonIconSize
                        }
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
