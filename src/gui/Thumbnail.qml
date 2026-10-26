// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

import QtQuick
import QtQuick.Controls.Basic

// The small square before the name of a file waiting in the composer or being uploaded: the picture
// itself, cropped to fill, for an image, else the file's extension in capitals. Which one is
// decided in Rust.
Rectangle {
    id: root

    property bool isImage: false
    property string label: ""
    property url source

    clip: true
    color: Theme.raisedHover
    implicitHeight: Theme.thumbnailSize
    implicitWidth: Theme.thumbnailSize
    radius: Theme.radiusThumbnail

    Image {
        objectName: "thumbnailImage"
        anchors.fill: parent
        asynchronous: true
        fillMode: Image.PreserveAspectCrop
        source: root.isImage ? root.source : ""
        sourceSize.height: Theme.thumbnailSize * Screen.devicePixelRatio
        sourceSize.width: Theme.thumbnailSize * Screen.devicePixelRatio
        visible: root.isImage
    }

    Label {
        objectName: "thumbnailLabel"
        anchors.centerIn: parent
        color: Theme.textMuted
        font.pointSize: Theme.fontSizeThumbnail
        font.weight: Theme.fontWeightBold
        text: root.label
        visible: !root.isImage
    }
}
