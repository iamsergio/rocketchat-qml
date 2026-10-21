// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// A secondary action that doesn't draw attention: Cancel, Retry, Dismiss. Only the pointer over it
// shows that it is a button.
Button {
    id: control

    flat: true
    font.pointSize: Theme.fontSizeSmall
    font.weight: Theme.fontWeightSemibold
    bottomPadding: Theme.spacingNone
    leftPadding: Theme.buttonPaddingH
    rightPadding: Theme.buttonPaddingH
    topPadding: Theme.spacingNone
    implicitHeight: Theme.buttonHeight
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    opacity: enabled ? 1 : Theme.disabledOpacity

    contentItem: Text {
        color: control.hovered ? Theme.text : Theme.textMuted
        elide: Text.ElideRight
        font: control.font
        horizontalAlignment: Text.AlignHCenter
        text: control.text
        verticalAlignment: Text.AlignVCenter
    }

    background: Rectangle {
        color: control.hovered ? Theme.raisedHover : Theme.transparent
        radius: Theme.radiusControl
    }
}
