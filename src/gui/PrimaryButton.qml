// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// The main action of a place: Send, Log in. In the colour of what is the user's.
Button {
    id: control

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
        color: Theme.textOnOwn
        elide: Text.ElideRight
        font: control.font
        horizontalAlignment: Text.AlignHCenter
        text: control.text
        verticalAlignment: Text.AlignVCenter
    }

    background: Rectangle {
        color: control.hovered ? Qt.lighter(Theme.own, Theme.hoverLighter) : Theme.own
        radius: Theme.radiusControl
    }
}
