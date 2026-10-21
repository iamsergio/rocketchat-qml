// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma Singleton

import QtQuick

// The visual constants of the GUI. Colours that follow the system theme come from the Controls
// palette instead; this only has the ones the palette has no role for.
QtObject {
    // Colours
    readonly property color error: "#c62828"
    readonly property color onAvatar: "white"

    // Fonts
    readonly property real fontSizeBase: Application.font.pointSize
    readonly property real fontSizeEmoji: 18
    readonly property real fontSizeSmall: fontSizeBase * 0.85
    readonly property real fontSizeTitle: fontSizeBase * 1.5
    readonly property real avatarFontScale: 0.4

    // Spacing, margins and paddings
    readonly property int spacingXXS: 1
    readonly property int spacingXS: 2
    readonly property int spacingS: 4
    readonly property int spacingM: 6
    readonly property int spacingL: 8
    readonly property int spacingXL: 10
    readonly property int spacingXXL: 12
    readonly property int pageMargin: 32
    readonly property int chipPadding: 14

    // Shapes
    readonly property real avatarRadiusRatio: 0.25
    readonly property int radiusSmall: 4

    // Lines
    readonly property int accentBarWidth: 3
    readonly property int borderWidth: 1
    readonly property int separatorWidth: 1

    // Sizes
    readonly property int attachmentFallbackHeight: 240
    readonly property int attachmentFallbackWidth: 360
    readonly property int attachmentMaxHeight: 480
    readonly property int attachmentMaxWidth: 360
    readonly property int avatarSize: 32
    readonly property int busyIndicatorSize: 32
    readonly property int chipAddWidth: 32
    readonly property int chipEmojiSize: 18
    readonly property int chipHeight: 24
    readonly property int composerMaxHeight: 120
    readonly property int dialogWidth: 360
    readonly property int emojiButtonSize: 28
    readonly property int emojiCellSize: 40
    readonly property int emojiPickerHeight: 340
    readonly property int emojiPickerWidth: 360
    readonly property int historyFooterHeight: 40
    readonly property int loginFormWidth: 360
    readonly property int menuButtonHeight: 22
    readonly property int menuButtonWidth: 28
    readonly property int roomIconSize: 24
    readonly property int roomIconSourceSize: 16
    readonly property int sidebarWidth: 260
    readonly property int windowHeight: 600
    readonly property int windowWidth: 900

    // Opacity
    readonly property real disabledOpacity: 0.6
    readonly property real highlightOpacity: 0.2
    readonly property real placeholderOpacity: 0.3

    // Timing
    readonly property int toolTipDelay: 400
}
