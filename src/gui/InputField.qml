// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// A line of text on its own: the login fields and the emoji search. The border says which one has
// the focus. `quiet` sets it into its surroundings until it has the focus: no border, and a darker
// fill.
TextField {
    id: control

    property bool quiet: false

    bottomPadding: Theme.spacingNone
    color: readOnly ? Theme.textMuted : Theme.text
    implicitHeight: Theme.fieldHeight
    leftPadding: Theme.fieldPaddingH
    opacity: enabled ? 1 : Theme.disabledOpacity
    rightPadding: Theme.fieldPaddingH
    topPadding: Theme.spacingNone
    verticalAlignment: TextInput.AlignVCenter

    background: Rectangle {
        border.color: control.activeFocus ? Theme.own : control.quiet ? Theme.transparent : Theme.raisedHover
        border.width: Theme.borderWidth
        color: control.readOnly ? Theme.base : control.quiet && !control.activeFocus ? Theme.fieldQuiet : Theme.surface
        radius: Theme.radiusControl
    }
}
