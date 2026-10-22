// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick

// Over a Text of rich text: a right-click on one of its links asks for the menu of that link, and
// one anywhere else in the text is told as such, for the owner to do what a right-click does there.
// It takes every right-click, as a press it rejects didn't reach what is below it when Spix clicked.
// A MouseArea, as Spix clicks don't reach a TapHandler. Left-clicks go through to the text, which
// opens the link. A MouseArea always sets a cursor, which hides the text's own, so it shows the
// pointing hand over a link itself.
MouseArea {
    id: root

    // The text with the links.
    required property Text label

    // The user right-clicked the link `url`, at `x`, `y` in `origin`.
    signal menuRequested(string url, Item origin, real x, real y)
    // The user right-clicked the text, not a link, at `x`, `y` in `origin`.
    signal elsewhereClicked(Item origin, real x, real y)

    acceptedButtons: Qt.RightButton
    anchors.fill: parent
    cursorShape: label.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor

    onClicked: mouse => {
        const url = label.linkAt(mouse.x, mouse.y);
        if (url !== "")
            root.menuRequested(url, root, mouse.x, mouse.y);
        else
            root.elsewhereClicked(root, mouse.x, mouse.y);
    }
}
