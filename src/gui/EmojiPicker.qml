// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import RocketChat.Bridge

// The emoji to react with: a search box and a grid of the emoji that match it. The model does the
// searching. Picking one emits `picked` and closes the popup. The popup hangs off the window's
// overlay while open, so Spix finds its items from the window (mainWindow/emojiSearch).
Popup {
    id: root

    // The shortcode, with its colons, of the emoji the user picked.
    signal picked(string shortcode)

    anchors.centerIn: Overlay.overlay
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    height: Theme.emojiPickerHeight
    modal: true
    padding: Theme.spacingL
    parent: Overlay.overlay
    width: Theme.emojiPickerWidth

    background: Rectangle {
        border.color: Theme.raisedHover
        border.width: Theme.borderWidth
        color: Theme.raised
        radius: Theme.radiusPanel
    }

    onAboutToShow: search.clear()
    onOpened: search.forceActiveFocus()

    contentItem: ColumnLayout {
        spacing: Theme.spacingL

        InputField {
            id: search

            objectName: "emojiSearch"
            Layout.fillWidth: true
            placeholderText: qsTr("Search emoji")
            onTextChanged: EmojiCatalog.query = text
        }

        GridView {
            id: grid

            objectName: "emojiGrid"
            Layout.fillHeight: true
            Layout.fillWidth: true
            cellHeight: Theme.emojiCellSize
            cellWidth: Theme.emojiCellSize
            clip: true
            model: EmojiCatalog

            ScrollBar.vertical: ScrollBar {}

            delegate: Item {
                id: pick

                required property string shortcode
                required property string glyph
                required property string image

                // The shortcode without its colons, as the tests address it.
                objectName: "emojiPick_" + shortcode.slice(1, -1)
                height: grid.cellHeight
                width: grid.cellWidth

                Label {
                    anchors.centerIn: parent
                    font.pointSize: Theme.fontSizeEmoji
                    text: pick.glyph
                    textFormat: Text.PlainText
                    visible: pick.image === ""
                }

                Image {
                    anchors.centerIn: parent
                    asynchronous: true
                    fillMode: Image.PreserveAspectFit
                    height: Theme.emojiImageSize
                    source: pick.image
                    visible: pick.image !== ""
                    width: Theme.emojiImageSize
                }

                // Not a TapHandler, which Spix clicks don't trigger.
                MouseArea {
                    id: area

                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: {
                        root.picked(pick.shortcode);
                        root.close();
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    color: Theme.ownWash
                    radius: Theme.radiusControl
                    visible: area.containsMouse
                }

                ToolTip.delay: Theme.toolTipDelay
                ToolTip.text: pick.shortcode
                ToolTip.visible: area.containsMouse
            }
        }
    }
}
