// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// A line of text on its own: the login fields and the emoji search. The border says which one has
// the focus.
TextField {
    id: control

    bottomPadding: Theme.spacingNone
    color: readOnly ? Theme.textMuted : Theme.text
    implicitHeight: Theme.fieldHeight
    leftPadding: Theme.fieldPaddingH
    opacity: enabled ? 1 : Theme.disabledOpacity
    rightPadding: Theme.fieldPaddingH
    topPadding: Theme.spacingNone
    verticalAlignment: TextInput.AlignVCenter

    background: Rectangle {
        border.color: control.activeFocus ? Theme.own : Theme.raisedHover
        border.width: Theme.borderWidth
        color: control.readOnly ? Theme.base : Theme.surface
        radius: Theme.radiusControl
    }
}
