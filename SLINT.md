# Slint notes

The limitations we hit in Slint (1.18.1) and its testing crate, how we work around them, and what could be fixed upstream. Remove an entry when a release we use fixes it. What the Slint app still lacks against the Qt one, through no fault of Slint's, is in PLAN.md's "Slint GUI gaps".

## `StyledText` takes only a subset of markdown

`StyledText::from_markdown` accepts bold, italic, strikethrough, inline code, links, lists, and the inline tags `<font color>` and `<u>`. It rejects headings, code blocks, block quotes, images and every other HTML tag (`i-slint-common` `styled_text.rs`), and its spans can't set a font size or a background. Links are always underlined: there is a `link-color`, but nothing to turn the underline off.

- **Workaround:** `rc_slint` turns rich text into that subset and nothing else. A heading is a bold line, a code block one inline-code line per line, a quote its lines behind a bar in the quote colour, a message of only emoji a larger default font size, a custom emoji its `:shortcode:`. Author text is escaped character by character (`\` before ASCII punctuation), since a stray `<b>` fails the whole parse. Mentions get colour but no background, unlike the Qt app.
- **Possible patch:** support inline images (custom emoji, at the text's height), block quotes and code blocks in `styled_text.rs`, a background colour span, and a property for the links' underline.

## No popup-aware position in the element API

`ElementHandle::absolute_position()` of an element inside a `PopupWindow` (every menu) is its position inside the popup, not in the window. The popup-aware `absolute_center()` is `pub(crate)` in `i-slint-backend-testing`.

- **Workaround:** `spix_slint` clicks with the public `single_click`/`double_click`, which use `absolute_center()`. `getBoundingBox` of a popup item is still relative to the popup; no GUI test reads it.
- **Possible patch:** make `absolute_center()` public, or give `absolute_position()` a popup-aware sibling.

## A click on an `if` item inside a `PopupWindow` misses

The testing backend's click on an element made by an `if` (a conditional child) of a `PopupWindow` lands somewhere other than the element: the popup closes as for a click outside it, and the item's `triggered` doesn't run. Items made unconditionally are hit.

- **Workaround:** `room_delegate.slint` makes every item of the room menu always, with `visible` and a height of zero while hidden, instead of `if`.
- **Possible patch:** make the popup-aware position of `absolute_center()` take the repeater's offset into account.

## The testing crate is internal

`i-slint-backend-testing` must match `slint`'s version exactly (`=1.18.1` in `Cargo.toml`), and has no stability promise. Its `internal` feature doesn't build from crates.io: it `include_bytes!`s fonts that only exist in Slint's repository.

- **Workaround:** pin both, and expect breakage on upgrades.
- **Possible patch:** publish the search API as a stable part of `slint`, or make `internal` buildable from the published crate.

## Only a component named `ListView` creates just the rows in sight

The compiler instantiates a `for` lazily only inside a component named `ListView` (it matches the name, `object_tree.rs`). The standard widgets' `ListView` brings a style with it.

- **Workaround:** `crates/rc_slint/ui/list_view.slint` is a style-free component of that name. Check it on each Slint upgrade.
- **Possible patch:** a documented attribute or property that asks for lazy instantiation.

## The lazy `ListView` can't keep its place on an insert at the top, or bottom-align a few rows

A chat list needs both: older messages are inserted above the rows in view and the view must not move, and a room with few messages sits at the bottom of the list. A spike with the lazy list (`ListView`, above) kept neither: with the top of the view in sight (exactly the state after Home) an insert above shifts what is shown, and the list has no way to align its content to the bottom.

- **Workaround:** the message list (`message_list.slint`) isn't lazy. It is a plain `for` of every loaded message in one `VerticalLayout` (`alignment: end`) in a `Flickable`, and keeps its place itself: Rust counts the insertions at the top, and the handler of the layout's height moves `content-y` by the height that was added. The cost is that every loaded message is instantiated and laid out again in O(n) per change (the spike's numbers: about 5 ms at 500 rows, 22 ms at 2000). Rows are measured when they are made, so each wrapped text is given an explicit width.
- **Possible patch:** a lazy list that anchors to a row (or to the bottom), keeping it in place when rows are inserted before it, and that can bottom-align content shorter than the list.

## `accessible-id` needs a constant `accessible-role`

An element is only found by its `accessible-id` if it also has an `accessible-role` that doesn't change. The element search also never visits the window's own element.

- **Workaround:** every element a GUI test addresses sets `accessible-role`. A path that stops at the window has no element behind it.

## The image cache keys on the path

`Image::load_from_path` caches by path, so a file replaced under the same name (a new avatar) keeps showing the old picture.

- **Workaround:** none yet. It's one of PLAN.md's "Slint GUI gaps".
- **Possible patch:** a way to drop one entry from the cache, or to key it on the file's modification time.

## The mock renderer of the testing backend has no pixels

The testing backend keeps a mock renderer by default, which measures text with fixed metrics but draws nothing, so `Window::take_snapshot`, the only way a Spix test can see what the app drew, fails on it. Its software rasterizer gives it pixels, behind the crate's `renderer-software` feature, which adds a `renderer_name` option to `TestingBackendOptions`.

- **Workaround:** `rc_slint --spix-port` sets the testing backend itself with `renderer_name: Some("software".into())`, and `spix_slint`'s `takeScreenshot` crops its PNG from the snapshot. The software renderer also measures text with real fonts, so layouts differ slightly from the mock's.
- **Possible patch:** `init_integration_test_with_system_time` taking the renderer name, so that no one has to build the platform by hand.

## A multi-line `TextInput` is only a text input

`TextInput` has no placeholder, doesn't grow with its text and doesn't keep the cursor in view in a `Flickable`. Enter in a multi-line one starts a line before any handler of an ancestor sees it.

- **Workaround:** `composer.slint` puts the `TextInput` in a `Flickable` inside a `FocusScope`, whose `capture-key-pressed` takes Enter without Shift before the input does (`reject` for the rest). The field's height follows the input's `preferred-height`, clamped. A `Text` over it is the placeholder, and `cursor-position-changed` moves the `Flickable` to the cursor's line, whose height is estimated from the font size.
- **Possible patch:** a `placeholder-text` on `TextInput`, and its own scrolling and `cursor-rect` property.

## A component can't name a property or function after a built-in of its base

`Rectangle` and `FocusScope` already have `padding-left`, `padding-right` and `clear-focus()`, and a component that declares a property named `text` can't write `accessible-role: text` (the name resolves to the property, not to the role).

- **Workaround:** other names (`text-inset-left`, `give-up-focus`), and `AccessibleRole.text` in such a component.

## A hidden item keeps its place in a layout

`visible: false` only stops drawing and the element search; the item still takes its share of a layout, so two stretching siblings that swap (the room list and the search results) split the height.

- **Workaround:** the pair sits in one `Rectangle` that stretches, each sized to its parent, and `visible` picks one.

## The testing API can't tell whether a `Text` is elided

`ElementHandle` exposes size, position and the accessible properties, but no preferred size, and Slint's accessibility has no notion of elided text. A GUI test that asks whether a name is cut (Qt's `Text.truncated`) has nothing to read.

- **Workaround:** the `Text` sets `accessible-description: self.preferred-width > self.width ? "truncated" : ""` (`msgAuthor` in `message_delegate.slint`, the only one that needs it), and `spix_slint` answers `truncated` as `true` when the description is exactly `truncated`, `false` otherwise. `preferred-width` of an eliding `Text` is the width of its whole text, so it reflects the cut.
- **Possible patch:** `ElementHandle` exposing the layout info (preferred size), or an accessible "text is truncated" state.

## A `FlexboxLayout` reports a tall layout info to its parent

Inside a `Rectangle`, the `FlexboxLayout` of `msgReactions` reported (as the `Rectangle`'s minimum and preferred height) about two lines of chips whatever the width, while its own `preferred-height` at the real width was one line. The message's column got that extra height and gave it to the body above the chips, so the chips sat about 30px lower than in the Qt app. Setting only one of `min-height` and `preferred-height` on the `Rectangle` didn't help; both are needed.

- **Workaround:** `min-height` and `preferred-height` of that `Rectangle` are both `chips.preferred-height` (`message_delegate.slint`), which follows the width and so wraps right.
- **Possible patch:** the flexbox's layout info taking the width it is given for its height.
