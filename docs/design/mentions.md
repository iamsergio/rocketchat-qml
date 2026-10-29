# Mentions

Typing `@` in the composer of a room or a thread opens a popup above it with the room's members that match what follows the `@` (ACT-013). This is the server call, the core and the logic of `rc_view::mention`, which both frontends use; the Qt popup and the Slint one come after (see PLAN.md).

## What the user gets

- The popup opens when the word at the cursor starts with `@`, at the start of the text or after whitespace. A space, or leaving the word, closes it, and so does Esc, which leaves the text.
- It lists at most 6 people: avatar, display name, `@username`, the typed part highlighted. If more match, a line "N more, keep typing" follows them, then `@all` and `@here` when they start with the typed text, in channels and private groups only.
- Up and Down move, Enter and Tab insert `@username ` (with a space) in place of the `@` and what was typed after it, and of the rest of the word after the cursor.
- The previous results stay while a new search runs; a spinner shows only when there are none yet. A failure says "Can't search the members: the server didn't answer." in the popup.

## What the server does

The contract tests (`crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`) record it. Checked on Rocket.Chat 8.0.

- `GET /api/v1/rooms.membersOrderedByRole?roomId=<id>&filter=<text>&count=<n>` answers `{members: [{_id, username, name, status, ...}], count, offset, total}`. `filter` matches as a substring of the username or of the name, ignoring case (`lice` finds `alice…`, `Doe` finds "Alice Doe"); an empty one lists everybody. The order is owners, moderators, then by name, and `rc_api` keeps it. `total` is how many match, whatever `count` is.
- It works for a public channel the user is not in, and for a private group only if the user is in it (`404` otherwise).
- For a direct message it answers `400 error-room-type-not-supported`. So the core answers a direct message itself, with the other person (`Room::peer` and the room's name), filtered the same way, without a request.

## Searching

`AccountHandle::search_members(room, query)` is called on every change of the text after the `@`; `cancel_member_search()` when the popup closes. The core works as for the [directory search](directory.md#searching) (`crates/rc_core/src/account/members.rs`):

- the query is not trimmed, and may be empty (just `@`): the room's first members;
- it tells the frontend at once with `Event::MemberSearch(Some(MemberSearch { room, query, state: Searching }))`, and waits for the typing to stop (300 ms; `Tuning::members_debounce`, `RC_MEMBERS_DEBOUNCE_MS` in whole milliseconds, `0` for no wait: a testing aid) before it asks for `MEMBERS_PER_SEARCH + 1` (7) members; The wait is a deadline of the actor, so it runs on the fake clock in tests;
- the user is left out of the answer (`members::without_user`): the page is cut to 6 after that, so a page that held the user still fills the rows, and `total` is the server's minus one if the user matches the query, whether or not they were on the page. The match is the server's, a substring of the username or of the display name ignoring case, with the account's own username and display name (if the account has none, the username alone). A direct message's answer is unaffected;
- the answer is `Found { members, total }` or `Failed(MemberError)`. A newer search (another query or another room), a cancel and a logout end the request in flight, and the answer of one that was ended is dropped by its attempt id. An expired token expires the session. A cancel, and a logout that finds a search, tell `MemberSearch(None)`;
- the same room and query again does nothing, except after a `Failed`: then it asks again at once;
- a direct message is told as `Found` at once, with no `Searching` and no request;
- the avatars of the people found are wanted (`AvatarOwner::User`), as the directory's are. The avatar download does not care whether the person ever wrote a message, so the popup needs nothing more than the `Event::Avatars` the frontend follows already.

## What `rc_view` decides

`rc_view::mention` is plain Rust:

- `word_at_cursor(text, cursor)` gives the `@` position and the query up to the cursor, or `None` when the popup is closed. A second `@` in the word closes it (`a@b` is an address, not a mention).
- `insert(text, cursor, start, username)` gives the new text and the new cursor.
- `Suggestions`, which the `Cache` keeps (`Cache::mentions`) from `Event::MemberSearch`, keeps the last answer while the next search runs. `popup(room_kind, query)` gives the rows (`Row::Person`, `Row::More`, `Row::Special`) and a `Status` (`Searching` only when there are no rows, `Failed` with the text `FAILED_TEXT`), leaving out the people of the last answer that the query as typed now rules out (a substring of the username or of the name), so that `al` to `ali` doesn't show Alan while the new search runs. The highlight and `@all`/`@here` follow the typing the same way. The "more" count is the last answer's: its total minus the people it showed.

**Units.** Positions in the text are UTF-16 code units, which is what Qt's cursor uses; a frontend with another unit converts at its edge. The highlight ranges are byte ranges of the name and the username, found ignoring case.
