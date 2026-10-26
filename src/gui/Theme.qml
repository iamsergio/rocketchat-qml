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
    // The rail of servers at the left of the window: darker than the sidebar.
    readonly property color rail: "#111319"
    // The conversation, and the input fields.
    readonly property color surface: "#1b1f26"
    // A field set quietly into the sidebar while it isn't focused: darker than the sidebar, like
    // the rail.
    readonly property color fieldQuiet: rail
    // The ends of a gradient.
    readonly property real gradientStart: 0
    readonly property real gradientEnd: 1
    // The surface with no opacity: a gradient to it fades into the surface without going dark.
    readonly property color surfaceClear: Qt.alpha(surface, 0)
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
    // Own at 6%: the message whose thread is open beside the room.
    readonly property color ownTint: "#0f7f9cff"
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
    // the same size (its x-height is 0.51 em, theirs 0.55), and a 9 pt system font in it is too
    // small to read comfortably. 9 pt is also what Qt falls back to without a platform theme, and
    // this makes it 15 px, which chat apps settle on for messages.
    readonly property real fontSizeBase: Application.font.pointSize * fontScale
    readonly property real fontScale: 1.25
    readonly property real fontSizeEmoji: 18
    readonly property real fontSizeTiny: fontSizeBase * 0.82
    readonly property real fontSizeSmall: fontSizeBase * 0.89
    readonly property real fontSizeHeading: fontSizeBase * 1.2
    // The letter of a server's tile.
    readonly property real fontSizeRailTile: fontSizeBase * 1.13
    readonly property real fontSizeTitle: fontSizeBase * 1.7
    readonly property real avatarFontScale: 0.4
    readonly property int fontWeightNormal: Font.Normal
    readonly property int fontWeightMedium: Font.Medium
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
    readonly property int radiusCheckBox: 4

    // Lines
    readonly property int accentBarWidth: 3
    readonly property int quoteBarWidth: 2
    readonly property int borderWidth: 1
    readonly property int separatorWidth: 1

    // Sizes
    // The box of a check box, its radius and the mark in it.
    readonly property int checkBoxSize: 15
    readonly property int checkMarkSize: 11
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
    // The corner arrow of "Replied in a thread".
    readonly property int inThreadIconSize: 13
    readonly property int loginFormWidth: 340
    readonly property int menuButtonHeight: 22
    readonly property int menuButtonWidth: 28
    readonly property int roomAvatarSize: 22
    // Two lines fit: the name and the topic.
    readonly property int roomHeaderHeight: 56
    readonly property int roomHeaderIconSize: 18
    // The fade at the end of a topic that goes on past the header.
    readonly property int roomTopicFadeWidth: 32
    // The widest the tool tip with the whole topic gets, before it wraps.
    readonly property int roomTopicTipMaxWidth: 420
    readonly property int roomIconSize: 14
    readonly property int roomRowHeight: 34
    // The space between the avatar and the name in a room row.
    readonly property int roomRowSpacing: spacingL
    // Where the avatar and the name start in a room row, from the left edge of its background, and
    // the middle of the avatar: what the search field below the list lines up with.
    readonly property int roomAvatarInset: rowPaddingH
    readonly property int roomNameInset: roomAvatarInset + roomAvatarSize + roomRowSpacing
    readonly property real roomAvatarCenter: roomAvatarInset + roomAvatarSize / 2
    // The search field at the bottom of the room list, and the magnifier in it.
    readonly property int searchFieldHeight: 32
    // Its distance from the bottom edge: the settings button of the rail beside it sits spacingXXL
    // from the edge, so this puts the middle of the field on the middle of that button.
    readonly property real searchFieldBottomMargin: spacingXXL + (iconButtonSize - searchFieldHeight) / 2
    readonly property int searchIconSize: 14
    // The button that clears it, and the cross in it, and the keycap that hints at Ctrl+K.
    readonly property int searchClearSize: 22
    readonly property int searchClearIconSize: 12
    readonly property int searchHintPaddingH: 6
    readonly property int searchHintPaddingV: 1
    // The "No new notifications" toast over the search field: its fade, how long it stays, and
    // its distance above the field.
    readonly property int toastFadeDuration: 150
    readonly property int toastDuration: 2500
    readonly property int toastGap: spacingXXL
    // The groups of the search results (channels, people, your rooms): the label over each is this
    // tall with its text at the bottom, this far from the bottom edge, in widely spaced capitals.
    readonly property int directoryLabelHeight: 26
    readonly property int directoryLabelPaddingBottom: spacingS
    readonly property real directoryLabelLetterSpacing: 0.6
    // The "N more, keep typing" line under a group, and the rows that say the directory is being
    // searched or could not be.
    readonly property int directoryMoreHeight: 22
    // The space between the name of a person and their username.
    readonly property int directoryNameGap: spacingM
    // The spinner in the row that says the directory is being searched.
    readonly property int directoryBusyIndicatorSize: 20
    // The space each side of the server's name at the top of the sidebar.
    readonly property int sidebarHeaderPaddingH: 16
    readonly property int sidebarWidth: 264
    // The thread pane beside the room, and the summary of a thread under its parent.
    readonly property int threadPaneWidth: 360
    readonly property int threadSummaryHeight: 28
    readonly property int threadSummaryIconSize: 14
    // The list of a room's threads in the pane: the avatar of a row, the lines its text is clamped
    // to, the height of a segment of the filter, and the chevron of the back button.
    readonly property int threadRowAvatarSize: 28
    readonly property int threadRowTextLines: 2
    readonly property int threadFilterHeight: 28
    readonly property int threadBackIconSize: 14
    // The badge on the room's threads button: a pill of at least this height and width, at the
    // button's top right corner, sticking out of it by the overhangs, with a ring in the header's
    // colour around it so that it reads as cut out.
    readonly property int threadsBadgeHeight: 16
    readonly property int threadsBadgeMinWidth: 16
    readonly property int threadsBadgePaddingH: 4
    readonly property int threadsBadgeRing: 2
    readonly property int threadsBadgeOverhangX: 5
    readonly property int threadsBadgeOverhangY: 5
    // The line "alice is typing…" over the composer. Its height is always taken, with or without
    // text, so the messages don't move when someone starts or stops typing.
    readonly property int typingLineHeight: 20
    readonly property int unreadBadgeHeight: 18
    readonly property int unreadBadgeMinWidth: 20
    readonly property int windowHeight: 600
    // The rail, the sidebar, the room and room for a thread beside it.
    readonly property int windowWidth: 964

    // The rail of servers: one tile per account, a pip at the left edge, a badge for mentions
    // (bottom right) and a dot for a login that ended (top right), both cut out of the rail's
    // colour, and a ring that turns while the account connects.
    readonly property int railWidth: 64
    readonly property int railTileSize: 40
    readonly property int railTileRadius: 12
    // The pip: short for unread, tall for the account on screen.
    readonly property int railPipWidth: 4
    readonly property int railPipUnread: 8
    readonly property int railPipCurrent: 24
    // The mentions badge: its height (also its minimum width), the space each side of its text, how
    // far it sticks out of the tile's corner, and the width of the cut-out around it and around the
    // dot.
    readonly property int railBadgeHeight: 18
    readonly property int railBadgePaddingH: 5
    readonly property int railBadgeOverhangX: 5
    readonly property int railBadgeOverhangY: 4
    readonly property int railCutoutWidth: 3
    readonly property int railDotSize: 11
    readonly property int railDotOverhang: 3
    // The ring around a connecting tile: its distance from the tile, width and turn time.
    readonly property int railRingGap: 4
    readonly property int railRingWidth: 2
    readonly property int railRingDuration: 1000
    readonly property int railSeparatorWidth: 28
    readonly property int railAddIconSize: 18
    // The border of the tile of a server that isn't added yet.
    readonly property real railNewBorderWidth: 1.5
    // The ring's arc, in radians (a quarter of the circle, from the top left to the top right), and
    // the angles of its turn, in degrees.
    readonly property real railRingArcStart: -3 * Math.PI / 4
    readonly property real railRingArcEnd: -Math.PI / 4
    readonly property int railRingTurnFrom: 0
    readonly property int railRingTurnTo: 360
    // How far the tool tip of a tile overlaps the rail.
    readonly property int railTipOverlap: 4

    // Opacity
    readonly property real disabledOpacity: 0.6
    // What shows only while the pointer is over its row: shown, and clear but still clickable.
    readonly property real opaque: 1
    readonly property real clear: 0

    // The factor that makes the main button brighter under the pointer.
    readonly property real hoverLighter: 1.08

    // Stacking: what shows above its siblings, like the rail and its tool tip above the page.
    readonly property int zAbove: 1

    // Timing
    readonly property int toolTipDelay: 400
}
