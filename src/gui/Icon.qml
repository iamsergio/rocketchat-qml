// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Effects

// A monochrome SVG icon in the colour `tint`. The SVGs are white: colorizing keeps the luminance,
// so a white source takes the colour as it is.
Image {
    id: root

    property color tint: Theme.text

    fillMode: Image.PreserveAspectFit
    // Rasterised at the size shown, in device pixels.
    sourceSize.width: width * Screen.devicePixelRatio
    sourceSize.height: height * Screen.devicePixelRatio
    // The software scene graph (under the offscreen platform, for one) draws no layer effects: there
    // the icon stays white.
    layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
    layer.effect: MultiEffect {
        colorization: 1
        colorizationColor: root.tint
    }
}
