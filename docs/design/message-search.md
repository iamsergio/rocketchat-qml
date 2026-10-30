# Message search

Searching the messages of the open room on the server (MSG-008). This step is the server call, the core and what `rc_view::Cache` keeps; the panel comes in the next steps (see PLAN.md), and so do jumping to a result in the room and the search across all rooms (MSG-009).

## What the user gets

The Qt panel is step 2 of the plan; nothing is shown yet.

## What the server does

The contract tests (`crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`) record it. Checked on Rocket.Chat 8.0.

- `GET /api/v1/chat.search?roomId=<id>&searchText=<text>&count=<n>&offset=<n>` answers `{messages: [...], success}`, the messages as the room history sends them (so `rc_api` parses them with the same type), plus a `score` per message that `rc_api` drops. Without `roomId` it is a `400 invalid-params`; no setting of the server is needed when there is one.
- It is a full-text (Mongo `$text`) search: whole words, ignoring case, so `Bananas` finds `banana` and `Banana` (the server stems), but `alph` doesn't find `alpha`. More than one word finds the messages with any of them; `"two words"` in quotes finds the phrase, and `-word` leaves out the messages with it. `from:admin` and the like are not understood: the word is just searched for.
- `/regex/i` works, and then it is a substring search (`/alph/i` finds `alpha`); `/^alpha/i` finds the messages that begin with it. Such a result has no `score`. An invalid expression is a `400` whose message says "Regular expression is invalid".
- The order is by `ts`, newest first, whatever the score (a message with the word three times does not come before a newer one with it once). `count` and `offset` page through that order; an `offset` beyond the end gives an empty list.
- Thread replies are in the results, with their `tmid`, whether or not they were also sent to the room, and so are system messages (a topic change whose new topic matches comes as `room_changed_topic`).
- A direct message is searched like a room. A public channel the user is not in can be searched; a private group the user is not in is a `400` with no message, and so is a room that doesn't exist.
- An empty `searchText` is a `400` (`error-searchText-param-not-provided`); one of blanks only is a `200` with nothing.

## Searching

`AccountHandle::search_messages(room, query)` is called when the user changes the text; `more_message_search()` for the next page; `cancel_message_search()` when the panel closes. The core works as for the [member search](mentions.md#searching) (`crates/rc_core/src/account/message_search.rs`):

- the query is trimmed. It tells the frontend at once with `Event::MessageSearch(Some(MessageSearch { room, query, state: Searching }))`, and waits for the typing to stop (300 ms; `Tuning::message_search_debounce`, `RC_MESSAGE_SEARCH_DEBOUNCE_MS` in whole milliseconds, `0` for no wait: a testing aid) before it asks for `MESSAGES_PER_SEARCH + 1` (21) messages. The wait is a deadline of the actor, so it runs on the fake clock in tests;
- the answer is `Found { messages, more }` or `Failed(MessageSearchError)`: the first 20 of what came, in the server's order (newest first). `more` is the list's footer, a `More`: `Available` when the 21st was there, else `No`. The results are not filtered: replies the room doesn't show and system messages are in them, as the server finds them. `Failed` is for the first page only;
- `more_message_search()` works from `Available` and from `Failed(_)` (the retry): it tells `Found` with the same messages and `more: Loading` at once, and asks for the next 21 after the ones held. The answer tells `Found` with the 20 appended and `Available` or `No`; a failure tells the same messages with `more: Failed(..)`, so what the user reads stays (an expired token still expires the session). From `No` and `Loading`, with no search, and while the first page isn't here, it does nothing: `more` is the one record of whether a page is on its way;
- a newer search (another query or another room), a cancel and a logout end the request in flight, and the answer of one that was ended is dropped by its attempt id. An expired token expires the session. A cancel, and a logout that finds a search, tell `MessageSearch(None)`;
- the same room and query again does nothing, except after a `Failed`: then it asks again at once;
- an empty query asks nothing: it is told as `Found` with no messages and `more: No`, and ends the search in flight;
- the avatars of the authors found are wanted (`AvatarOwner::User`), as the member search's are, so the frontend needs nothing more than the `Event::Avatars` it follows already.

**A snapshot.** The results do not follow edits, deletions or reactions that happen after the answer, nor new messages: searching again brings them up to date.

## What `rc_view` keeps

`rc_view::Cache` keeps the last `MessageSearch` the core sent (`Cache::message_search`), `None` after a `MessageSearch(None)`, so that a view created late catches up. There are no rows or view logic yet: they come with the panel (step 2).
