// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

pragma Singleton

import QtQuick

// The visual constants of the GUI: one dark design. Main.qml feeds the colours into the Controls
// palette, so that what we don't restyle (menus, dialogs, scroll bars) follows.
QtObject {
    // Colours: ink-slate neutrals with a slight blue bias, and two accents with fixed meanings.
    // Signal (Rocket.Chat red) marks what needs the user: mentions of them. Own (periwinkle) marks
    // what is the user's or where they are: the open room, links, their reactions, the main
    // button, the focused field.
    // The sidebar and the window.
    readonly property color base: "#15181e"
    // The conversation, and the input fields.
    readonly property color surface: "#1b1f26"
    // What sits on the surface: the composer, chips, menus and popups.
    readonly property color raised: "#242933"
    // Hover on raised, and the borders that must show against it. Also the unread badge.
    readonly property color raisedHover: "#2f3541"
    // The hairlines between regions.
    readonly property color line: "#262b34"
    readonly property color text: "#e6e8ed"
    readonly property color textMuted: "#949bab"
    readonly property color textFaint: "#626a7a"
    readonly property color signal: "#f5455c"
    // The text on signal and on own. Not `onSignal` and `onOwn`, which QML takes for the handlers of
    // the signals of the properties `signal` and `own`.
    readonly property color textOnSignal: "#fff4f5"
    // Signal at 9%.
    readonly property color signalWash: "#17f5455c"
    readonly property color own: "#7f9cff"
    readonly property color textOnOwn: "#0f1424"
    // Own at 14%.
    readonly property color ownWash: "#247f9cff"
    // Own at 50%: the border of the focused composer and of the user's reactions.
    readonly property color ownBorder: "#807f9cff"
    readonly property color warning: "#e8b44f"
    // Warning at 12%.
    readonly property color warningWash: "#1fe8b44f"
    readonly property color error: "#ff8a66"
    readonly property color onAvatar: "white"
    readonly property color transparent: "transparent"
    // What dims the window behind a modal popup, at the alpha the style gives it.
    readonly property color shadow: "black"
    // White at 2.5%: the message under the pointer.
    readonly property color rowHover: "#06ffffff"

    // The style sheet that goes before the HTML of a message (rich_html.rs gives the markup class
    // names and no colours): links, mentions, channels, code and the bar of a quote. A Label's
    // `linkColor` doesn't colour the links of rich text. A QML colour with alpha turns into
    // `#AARRGGBB`, which Qt's CSS takes.
    readonly property string richTextStyle: "<style>"
        + "a { color: " + own + "; text-decoration: none; }"
        + "span.mention-me { color: " + signal + "; background-color: " + signalWash + "; font-weight: bold; }"
        + "span.mention, span.channel { color: " + own + "; background-color: " + ownWash + "; }"
        + "code, table.code-block { background-color: " + base + "; }"
        + "td.quote-bar { background-color: " + raisedHover + "; }"
        + "</style>"

    // Fonts
    // The system's size, made larger: Instrument Sans reads smaller than the usual system fonts at
    // the same size, and a 9 pt system font in it is too small to read comfortably.
    readonly property real fontSizeBase: Application.font.pointSize * fontScale
    readonly property real fontScale: 1.17
    readonly property real fontSizeEmoji: 18
    readonly property real fontSizeTiny: fontSizeBase * 0.82
    readonly property real fontSizeSmall: fontSizeBase * 0.89
    readonly property real fontSizeHeading: fontSizeBase * 1.2
    readonly property real fontSizeTitle: fontSizeBase * 1.7
    readonly property real avatarFontScale: 0.4
    readonly property int fontWeightNormal: Font.Normal
    readonly property int fontWeightSemibold: Font.DemiBold
    readonly property int fontWeightBold: 650

    // Spacing, margins and paddings
    readonly property int spacingNone: 0
    readonly property int spacingXXS: 1
    readonly property int spacingXS: 2
    readonly property int spacingS: 4
    readonly property int spacingM: 6
    readonly property int spacingL: 8
    readonly property int spacingXL: 10
    readonly property int spacingXXL: 12
    readonly property int spacingXXXL: 20
    readonly property int pageMargin: 32
    readonly property int chipPadding: 14
    readonly property int badgePaddingH: 12
    readonly property int buttonPaddingH: 14
    readonly property int composerPaddingH: 14
    readonly property int dialogPadding: 6
    readonly property int fieldPaddingH: 12
    readonly property int messagePaddingV: 6
    readonly property int rowPaddingH: 8
    // How far the rounded background of a room row is from the edges of the list.
    readonly property int roomRowInset: 8
    readonly property int textAreaPaddingV: 5

    // Shapes
    readonly property real avatarRadiusRatio: 0.25
    readonly property int radiusControl: 7
    readonly property int radiusPanel: 12
    readonly property int radiusAttachment: 8

    // Lines
    readonly property int accentBarWidth: 3
    readonly property int quoteBarWidth: 2
    readonly property int borderWidth: 1
    readonly property int separatorWidth: 1

    // Sizes
    readonly property int attachmentFallbackHeight: 240
    readonly property int attachmentFallbackWidth: 360
    readonly property int attachmentMaxHeight: 480
    readonly property int attachmentMaxWidth: 360
    readonly property int attachmentFileIconSize: 22
    readonly property int attachmentFileMaxWidth: 340
    readonly property int avatarSize: 36
    readonly property int busyIndicatorLargeSize: 40
    readonly property int busyIndicatorSize: 32
    readonly property int buttonHeight: 30
    readonly property int chipAddWidth: 32
    readonly property int chipEmojiSize: 18
    readonly property int chipHeight: 24
    readonly property int composerMaxHeight: 120
    readonly property int composerFieldMinHeight: 30
    readonly property int dialogWidth: 360
    readonly property int emojiCellSize: 40
    readonly property int emojiImageSize: 28
    readonly property int emojiPickerHeight: 340
    readonly property int emojiPickerWidth: 360
    readonly property int fieldHeight: 38
    readonly property int historyFooterHeight: 40
    readonly property int iconButtonSize: 30
    readonly property int iconButtonIconSize: 17
    readonly property int loginFormWidth: 340
    readonly property int menuButtonHeight: 22
    readonly property int menuButtonWidth: 28
    readonly property int roomAvatarSize: 22
    readonly property int roomHeaderHeight: 52
    readonly property int roomHeaderIconSize: 18
    readonly property int roomIconSize: 14
    readonly property int roomRowHeight: 34
    readonly property int serverMarkSize: 26
    readonly property int sidebarWidth: 264
    readonly property int unreadBadgeHeight: 18
    readonly property int unreadBadgeMinWidth: 20
    readonly property int windowHeight: 600
    readonly property int windowWidth: 900

    // Opacity
    readonly property real disabledOpacity: 0.6

    // The factor that makes the main button brighter under the pointer.
    readonly property real hoverLighter: 1.08

    // Timing
    readonly property int toolTipDelay: 400
}
