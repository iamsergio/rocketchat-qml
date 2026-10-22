// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// What to do with a link that was right-clicked: open it or copy its address. One per view, shared
// by everything in it that shows links, as the emoji picker is; `openFor` says which link.
//
// The popup is parented to the window's overlay while open, so Spix finds its items by name from
// the window (mainWindow/linkCopyAction), and only while it is open.
Menu {
    id: root

    // The address the items act on.
    property string url

    // Opens the menu for the link `address` at `x`, `y` in `origin`.
    function openFor(address, origin, x, y) {
        url = address;
        popup(origin, x, y);
    }

    // Keeps the menu inside the window when it opens near an edge.
    margins: Theme.spacingL

    MenuItem {
        objectName: "linkOpenAction"
        text: qsTr("Open link")
        onTriggered: Qt.openUrlExternally(root.url)
    }

    MenuItem {
        objectName: "linkCopyAction"
        text: qsTr("Copy link")
        onTriggered: Clipboard.setText(root.url)
    }
}
