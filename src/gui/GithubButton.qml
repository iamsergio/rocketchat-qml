// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// A button of the GitHub page, in one of the looks of the design: "primary" (the user's colour),
// "warning" and "signal" (filled), "ghost" (an outline that fills when the pointer is over it; with
// `on` it is a toggle that is on) and "quiet" (text only). `iconSource` goes before the text; while
// `busy` a small spinner takes its place and the button keeps its colour though it is disabled.
// `small` is the 26 px button with the tiny font.
Button {
    id: control

    property bool busy: false
    property url iconSource
    property string look: "ghost"
    property bool on: false
    property bool small: false
    // A filled button that can't be used now is greyed out rather than paled.
    readonly property bool greyed: !enabled && !busy && (look === "primary" || look === "warning" || look === "signal")

    // The colour of the text and the icon.
    readonly property color foreground: {
        if (greyed)
            return Theme.textFaint;
        switch (look) {
        case "primary":
            return Theme.textOnOwn;
        case "warning":
            return Theme.textOnWarning;
        case "signal":
            return Theme.textOnSignal;
        case "quiet":
            return hovered ? Theme.signal : Theme.textFaint;
        default:
            return on ? Theme.own : Theme.text;
        }
    }

    bottomPadding: Theme.spacingNone
    font.pointSize: small ? Theme.fontSizeTiny : Theme.fontSizeSmall
    font.weight: Theme.fontWeightSemibold
    hoverEnabled: true
    implicitHeight: small ? Theme.prSmallButtonHeight : Theme.buttonHeight
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    leftPadding: Theme.buttonPaddingH
    opacity: enabled || busy || greyed ? Theme.opaque : Theme.disabledOpacity
    rightPadding: Theme.buttonPaddingH
    topPadding: Theme.spacingNone

    contentItem: RowLayout {
        spacing: Theme.spacingM

        Icon {
            Layout.preferredHeight: Theme.prButtonIconSize
            Layout.preferredWidth: Theme.prButtonIconSize
            source: control.iconSource
            tint: control.foreground
            visible: control.iconSource.toString() !== "" && !control.busy
        }

        BusyIndicator {
            Layout.preferredHeight: Theme.prButtonIconSize
            Layout.preferredWidth: Theme.prButtonIconSize
            palette.dark: control.foreground
            running: visible
            visible: control.busy
        }

        Text {
            color: control.foreground
            elide: Text.ElideRight
            font: control.font
            text: control.text
            verticalAlignment: Text.AlignVCenter
        }
    }

    background: Rectangle {
        border.color: {
            if (control.look !== "ghost") {
                return Theme.transparent;
            }
            return control.on ? Theme.own : Theme.raisedHover;
        }
        border.width: control.look === "ghost" ? Theme.borderWidth : Theme.spacingNone
        color: {
            if (control.greyed)
                return Theme.raised;
            switch (control.look) {
            case "primary":
                return control.hovered ? Qt.lighter(Theme.own, Theme.hoverLighter) : Theme.own;
            case "warning":
                return control.hovered ? Qt.lighter(Theme.warning, Theme.hoverLighter) : Theme.warning;
            case "signal":
                return control.hovered ? Qt.lighter(Theme.signal, Theme.hoverLighter) : Theme.signal;
            case "quiet":
                return Theme.transparent;
            default:
                if (control.on) {
                    return Theme.ownWash;
                }
                return control.hovered ? Theme.raised : Theme.transparent;
            }
        }
        radius: Theme.radiusControl
    }
}
