// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// Shown until the core has said whether a stored session resumes, so that the login page doesn't
// flash first.
Item {
    BusyIndicator {
        objectName: "startupBusy"
        anchors.centerIn: parent
        implicitHeight: Theme.busyIndicatorLargeSize
        implicitWidth: Theme.busyIndicatorLargeSize
        palette.dark: Theme.textMuted
        running: visible
    }

    VersionLabel {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.spacingXXL
        anchors.horizontalCenter: parent.horizontalCenter
    }
}
