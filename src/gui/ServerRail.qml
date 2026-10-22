// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import RocketChat.Bridge

// The accounts, in the order they were added, one tile each, then the way to add one, and the
// settings at the bottom. Always there, with one account too, so that + is where one would look.
Rectangle {
    id: root

    objectName: "serverRail"
    color: Theme.rail
    width: Theme.railWidth

    // The server whose tool tip shows, if any: a tile or the add button.
    property var tipOwner: null

    function setTip(owner, shown) {
        if (shown)
            tipOwner = owner;
        else if (tipOwner === owner)
            tipOwner = null;
    }

    // The tiles scroll when they don't fit, above the settings button. The + follows the last one.
    ListView {
        anchors.bottom: settings.top
        anchors.bottomMargin: Theme.spacingL
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        clip: true
        model: AccountList
        spacing: Theme.spacingL
        // Inside the view, so that the ring and the badges of the first and last tile aren't cut.
        topMargin: Theme.spacingXXL

        delegate: ServerTile {
            id: tile

            onTipShownChanged: root.setTip(tile, tipShown)
        }

        footer: Column {
            topPadding: Theme.spacingL
            spacing: Theme.spacingL
            width: Theme.railWidth

            // A new account has its dashed tile, which is where to go to add one.
            Item {
                height: Theme.separatorWidth
                visible: !AccountList.hasNew
                width: Theme.railWidth

                Rectangle {
                    anchors.centerIn: parent
                    color: Theme.line
                    height: Theme.separatorWidth
                    width: Theme.railSeparatorWidth
                }
            }

            AbstractButton {
                id: add

                readonly property bool expired: false
                readonly property real freshInset: Theme.spacingNone
                readonly property string tipLine: ""
                readonly property string tipTitle: qsTr("Add a server")
                property bool tipShown: false

                Accessible.name: tipTitle
                height: Theme.railTileSize
                hoverEnabled: true
                objectName: "addServerButton"
                padding: Theme.spacingNone
                visible: !AccountList.hasNew
                width: Theme.railWidth

                onClicked: AccountList.add()
                onHoveredChanged: {
                    if (!hovered)
                        tipShown = false;
                }
                onTipShownChanged: root.setTip(add, tipShown)

                Timer {
                    interval: Theme.toolTipDelay
                    running: add.hovered
                    onTriggered: add.tipShown = true
                }

                background: Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    border.color: add.hovered ? Theme.own : Theme.raisedHover
                    border.width: Theme.borderWidth
                    color: Theme.transparent
                    height: Theme.railTileSize
                    radius: Theme.railTileRadius
                    width: Theme.railTileSize
                }

                contentItem: Item {
                    Icon {
                        anchors.centerIn: parent
                        height: Theme.railAddIconSize
                        source: "icons/plus.svg"
                        tint: add.hovered ? Theme.own : Theme.textFaint
                        width: Theme.railAddIconSize
                    }
                }
            }
        }
    }

    // Beside the tile it names, over the page. Not a Popup: a popup takes the mouse events of the
    // area it covers, and this one covers the sidebar.
    RailTip {
        objectName: "serverTileTip"
        subtitle: root.tipOwner ? root.tipOwner.tipLine : ""
        subtitleColor: root.tipOwner && root.tipOwner.expired ? Theme.warning : Theme.textMuted
        title: root.tipOwner ? root.tipOwner.tipTitle : ""
        visible: root.tipOwner !== null
        x: Theme.railWidth - Theme.railTipOverlap
        y: root.tipOwner ? root.tipOwner.mapToItem(root, 0, 0).y + (root.tipOwner.height + root.tipOwner.freshInset - height) / 2 : 0
        z: Theme.zAbove
    }

    // Opens above the button, to its right: the button is at the bottom of the window.
    ToolButton {
        id: settings

        objectName: "settingsButton"
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.spacingXXL
        anchors.horizontalCenter: parent.horizontalCenter
        // Only the tool tip and the accessibility name show it.
        display: AbstractButton.IconOnly
        implicitHeight: Theme.iconButtonSize
        implicitWidth: Theme.iconButtonSize
        padding: Theme.spacingNone
        text: qsTr("Settings")

        ToolTip.delay: Theme.toolTipDelay
        ToolTip.text: text
        ToolTip.visible: hovered && !settingsMenu.visible

        onClicked: settingsMenu.popup(this, width, height - settingsMenu.implicitHeight)

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

        // The popup is parented to the window's overlay while open, so Spix finds its items by name
        // from the window (mainWindow/dumpInfoAction), and only while it is open.
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

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.top: parent.top
        color: Theme.line
        width: Theme.separatorWidth
    }
}
