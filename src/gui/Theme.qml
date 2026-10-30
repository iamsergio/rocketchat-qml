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
    // A passed check, and an idle runner's lamp. Success at 12%: the halo of that lamp.
    readonly property color success: "#52c48c"
    readonly property color successWash: "#1f52c48c"
    readonly property color warning: "#e8b44f"
    // The text on a warning-coloured button.
    readonly property color textOnWarning: "#1d1604"
    // Warning at 12%.
    readonly property color warningWash: "#1fe8b44f"
    readonly property color error: "#ff8a66"
    // The surface at 90%: what covers the messages while files are dragged over them.
    readonly property color dropOverlay: Qt.alpha(surface, 0.9)
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
    // The number after the title of a pull request, in styled text (which takes no style sheet).
    readonly property string richFaintStart: "<font color=\"" + textFaint + "\">"
    readonly property string richFaintEnd: "</font>"

    // Styled text from the model marks what the user typed with <b>: in the colour of what is
    // theirs, as the label of a mention would be.
    function matchStyled(text: string): string {
        return text.replace(/<b>/g, "<font color=\"" + own + "\"><b>").replace(/<\/b>/g, "</b></font>");
    }

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
    // The extension in a thumbnail.
    readonly property real fontSizeThumbnail: fontSizeBase * 0.7
    readonly property real fontSizeSmall: fontSizeBase * 0.89
    readonly property real fontSizeHeading: fontSizeBase * 1.2
    // The letter of a server's tile.
    readonly property real fontSizeRailTile: fontSizeBase * 1.13
    readonly property real fontSizeTitle: fontSizeBase * 1.7
    // The title of a pull request over its page.
    readonly property real fontSizePageTitle: fontSizeBase * 1.33
    // Branch names.
    readonly property string fontFamilyMono: "monospace"
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
    // A file waiting in the composer, the thumbnail in it, the track of an upload's progress and
    // the box that shows where dropped files go.
    readonly property int radiusFileChip: 7
    readonly property int radiusThumbnail: 5
    readonly property int radiusTrack: 2
    readonly property int radiusDrop: 14

    // Lines
    readonly property int accentBarWidth: 3
    readonly property int quoteBarWidth: 2
    readonly property int borderWidth: 1
    readonly property int dropBorderWidth: 2
    readonly property int separatorWidth: 1

    // Sizes
    // How far a list may be from its newest message, in pixels, and still count as at it.
    readonly property real scrollTolerance: 1
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
    // The files waiting in the composer, and those being uploaded: the chip, the thumbnail in it
    // (and in an upload row), the cross that takes one out, and the track of the progress.
    readonly property int fileChipHeight: 52
    readonly property int fileChipWidth: 212
    readonly property int thumbnailSize: 36
    readonly property int chipRemoveSize: 22
    readonly property int chipRemoveIconSize: 12
    readonly property int uploadTrackHeight: 3
    // The box shown while files are dragged over a pane: its distance from the edges, and its icon.
    readonly property int dropMargin: 16
    readonly property int dropIconSize: 36
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
    // The popup that completes an @mention above the composer: its width, the space between it and
    // the composer, the padding inside it, and the icon in the square of @all and @here.
    readonly property int mentionPopupWidth: 340
    readonly property int mentionPopupGap: spacingM
    readonly property int mentionPopupPadding: spacingS
    readonly property int mentionSpecialIconSize: 14
    // The hint line at its bottom: the space between its keys and words, and above it.
    readonly property int mentionHintSpacing: spacingS
    readonly property int mentionHintPaddingV: spacingL
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
    // The search of a room's messages takes the pane: the spinner beside the hint under its field,
    // and the lines a result's text is clamped to.
    readonly property int messageSearchSpinnerSize: 14
    readonly property int messageSearchTextLines: 3
    // The pill "Jump to latest" over the bottom of the message list, shown while the list is a
    // window onto the past: its height and its padding at the sides, the chevron in it, how far
    // it floats above the bottom, and the turn that makes the left-pointing chevron point down.
    readonly property int jumpToLatestHeight: 32
    readonly property int jumpToLatestPaddingH: 14
    readonly property int jumpToLatestIconSize: 14
    readonly property int jumpToLatestMarginBottom: 16
    readonly property int jumpToLatestChevronRotation: -90
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

    // The GitHub tile in the rail takes the server tiles' sizes, and has a mark of its own.
    readonly property int githubMarkSize: 21
    // The connect page: the mark over its heading, and the column the page is in.
    readonly property int githubConnectMarkSize: 36
    readonly property int githubConnectWidth: 380
    // The mark beside the repository and number on the page of a pull request.
    readonly property int githubPageMarkSize: 16
    // The check bar: one segment per job. Tall on the page, thin under a row of the list, with a
    // gap between segments. The segments of passed jobs are dimmed.
    readonly property int checkBarHeight: 10
    readonly property int checkBarHeightRow: 3
    readonly property int checkBarGap: 2
    readonly property real checkPassedOpacity: 0.55
    // A row of the pull request list: the state glyph, the vertical padding, the bell, and how
    // far the check bar is under the title.
    readonly property int prGlyphSize: 16
    readonly property int prRowPaddingV: 9
    readonly property int prBellSize: 12
    readonly property int prCheckBarGap: 7
    // The chevron of the Drafts heading, and the turns of the left-pointing icon to point right
    // (folded) and down (open).
    readonly property int prFoldChevronSize: 12
    readonly property int prFoldClosedRotation: 180
    readonly property int prFoldOpenRotation: -90
    // The strip over the actions of a pull request that has runs waiting for approval, and the
    // shield in it.
    readonly property int prBannerPaddingV: 8
    readonly property int prBannerIconSize: 16
    // The bar of actions: its padding at the sides, its least height (so that it doesn't jump
    // when the buttons give way to the question of a merge), and the icon in a button with one.
    readonly property int prActionBarPaddingH: 24
    readonly property int prActionBarMinHeight: 55
    readonly property int prButtonIconSize: 14
    // The small buttons (Approve runs, Rerun, the log of a job): their height, and the font of a
    // Rerun button.
    readonly property int prSmallButtonHeight: 26
    // The line under the actions that says why a command failed.
    readonly property int prErrorPaddingV: 8
    // The checks: the padding around them, and the space between their parts.
    readonly property int prChecksPaddingH: 24
    // The header of a pull request's page: its sides line up with the action bar and the checks.
    readonly property int prHeaderPaddingH: prActionBarPaddingH
    readonly property int prHeaderPaddingTop: 18
    readonly property int prHeaderPaddingBottom: 14
    readonly property int prChecksPaddingV: 18
    readonly property int prChecksSpacing: 16
    // The square before each word of the tally, and the gaps between the words.
    readonly property int prTallySwatchSize: 8
    readonly property int prTallySpacing: 14
    // How much taller a segment of the page's bar gets under the pointer, and the time it takes.
    readonly property real checkBarHoverScale: 1.5
    readonly property real checkBarRestScale: 1
    readonly property int checkBarHoverDuration: 100
    // A job row: its least height, and the glyph of its state.
    readonly property int prJobRowMinHeight: 36
    readonly property int prJobGlyphSize: 15
    // A heading of a group of jobs: its height.
    readonly property int prGroupHeadingHeight: 30
    // The toast over the page: the space around its text, its distance from the bottom, and the
    // check in it. It stays this long, in milliseconds.
    readonly property int prToastPaddingH: 16
    readonly property int prToastPaddingV: 10
    readonly property int prToastBottomMargin: 24
    readonly property int prToastIconSize: 14
    readonly property int githubToastDuration: 2600
    // A runner's lamp, and the halo around it while it is busy or idle.
    readonly property int runnerLampSize: 8
    readonly property int runnerLampHalo: 3
    // One turn of the refresh button while it fetches.
    readonly property int spinDuration: 1000
    // How often the page shows the ages again, in milliseconds.
    readonly property int ageInterval: 30000

    // Opacity
    readonly property real disabledOpacity: 0.6
    // What shows only while the pointer is over its row: shown, and clear but still clickable.
    readonly property real opaque: 1
    readonly property real clear: 0
    // The progress of an upload whose message is being posted: the whole bar, dimmed.
    readonly property real uploadConfirmingOpacity: 0.55

    // The factor that makes the main button brighter under the pointer.
    readonly property real hoverLighter: 1.08

    // Stacking: what shows above its siblings, like the rail and its tool tip above the page.
    readonly property int zAbove: 1

    // Timing
    readonly property int toolTipDelay: 400

    // The colour of a job or a segment of a check bar by its state: failed, running, passed,
    // cancelled, anything else (waiting for approval) being the quiet one.
    function checkColor(state: string): color {
        switch (state) {
        case "failed":
            return signal;
        case "running":
            return warning;
        case "passed":
            return success;
        case "cancelled":
            return textFaint;
        default:
            return raisedHover;
        }
    }
}
