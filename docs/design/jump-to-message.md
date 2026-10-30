# Jumping to a message

Choosing a result of the [message search](message-search.md) shows that message in the room, even when it is far older than what the room holds (MSG-008, step 3). This is the target design of the whole step; it is built in five parts (see Steps), and parts 3a and 3b are done: the open room's fetch has a kind and its timeline goes to the store through one function, and the server calls and their ports exist. Everything else below is not built yet.

## What the user gets

- Clicking a result, or Enter on the one under the cursor, scrolls the room to that message and marks it (a tinted background and a bar at its side). The search pane stays open, with the result marked as picked.
- If the message is not among those the room holds, the room shows a window around it. Scrolling down loads the newer messages until the window joins the latest ones; a pill "Jump to latest" shows while the room is cut off from the latest messages, and so does End.
- A message that no longer exists says "This message no longer exists" in the search pane and leaves the room as it is.
- The mark goes when the user jumps again, jumps to the latest, sends a message, opens another room or the message is deleted. Closing the search pane keeps it. It does not fade.
- A reply in a thread shows its parent in the room (or the reply itself if it was also sent to the room), with the mark, and opens the thread in the pane over the results; Back returns to the results. The thread does not scroll to the reply.

## What the server does

Checked on Rocket.Chat 8.0.1. Today's history is REST only (`RestClient::history`, `ServerApi::history`), and REST has no "around a message": `channels.history?oldest=X&count=n` returns the newest n after X, so it cannot walk forward either. The DDP methods are reachable through REST `method.call`, which the client's private realtime task does not need to share.

- `POST /api/v1/method.call/loadSurroundingMessages` with `{"message": "<DDP method frame as a JSON string>"}`, the frame being `{"msg":"method","id":"7","method":"loadSurroundingMessages","params":[{"_id":"<id>"},<limit>]}`, answers `200 {"message": "<DDP JSON>", "success": true}`. The inner `result` is `{messages, moreBefore, moreAfter}` with EJSON dates, which `wire.rs` already parses. The order is floor(limit/2) older messages newest first, then the target, then up to ceil(limit/2) - 1 newer ones oldest first; at an edge the other side is not filled up. It finds the message by `_id` only, and includes thread replies not sent to the room (the core filters them with `shows_in_room`). A missing or deleted id is a `200` with `result: false`, and so is the id of a message in a private group the user is not in (the server does not say that it exists). A bad argument is a `400` with the DDP `error` inside `message`; a bad token is a `401`. It works for channels, groups and direct messages.
- `loadNextMessages(rid, {"$date": ms} | null, limit)` answers the messages with a timestamp after the given one (exclusive), oldest first, hidden thread replies included, with no "more" flag. For something that is not a room it is a `400 error-not-allowed`.
- `chat.getMessage` is a `400` for a deleted message.

In `rc_api` (built): a private `RestSession::method_call(name, params)` that decodes the inner frame and maps its `error` (in a `400` or a `200`) to `Error::Server` with the error's `reason`, a `401` to `Unauthorized` and any other non-2xx to the REST mapping; `surrounding_messages(id, limit)` giving `Option<Window { messages, more_before, more_after }>` (`None` for `result: false`), which `wire.rs` sorts oldest first, the one place that knows the server's order; `next_messages(room, after, limit)` giving a `NextPage` (a `HistoryPage` has `reached_start`, so not that one), whose `reached_end` is the raw length being below the limit, counted before the filtering as `parse_history` does. The core asks from `newest.ts - 1 ms`, and the timeline de-duplicates. The ports are `ServerApi::around` and `ServerApi::newer` (`MessageWindow`, `NewerPage`), whose REST adapter leaves out the thread replies the room does not show as `history` does, after the flags were taken; the fake serves both from the messages it is given for a room (`set_room_messages`) unless a script is queued. A `429` (the DDP rate limit, when enabled) is an error like any other.

## What the core does

- **The window.** `Timeline` gets `detached`: newer messages exist that it does not hold. It is not part of `RoomHistory`, so it is never stored. The open room's fetch (`Fetch`) has a `FetchKind`: `Newest` and `Older` today (part 3a), `Newer` and `Around(MessageId)` with the window. `OpenRoom` gets `found: Option<Found { message, state }>`, the state being `Loading`, `Shown` or `Missing`.
- **Commands.** `AccountHandle::show_message(id)`, `load_newer()` and `show_latest()`.
- **show_message.** If the timeline holds the target, there is no request: `found` is `Shown` and `Event::ShowMessage` follows. Otherwise the fetch in flight is aborted (attempt ids, the last wins), `found` is `Loading` and the core asks `around(id, PAGE_SIZE)`. The answer replaces the window; if it shares a message with what is held, the two merge and the timeline stays attached. `result: false` makes `found` `Missing` and leaves the timeline alone.
- **The store.** `OpenRoom::persist` is the one place the open room's timeline goes to the `MessageStore` (done in 3a); the window is never saved while detached, so the store keeps the attached copy. While detached, `change_timeline` applies the changes (edits, deletions, echoes settling, delivery failures) to the window, emitting them, and to the stored copy, as it does for a closed room. Pushed new messages go to neither: `merge_live` on a detached timeline drops what it does not hold. Unread counts and notifications come from the subscriptions and are not affected.
- **load_newer** appends pages (`Appended`) until a page reaches the end or reaches a message the stored copy holds. Then it joins: appends the rest, clears `detached`, and fetches the newest page with the catch-up, as opening a room does. `load_older` is as today, with `reached_start` from `moreBefore`.
- **show_latest** resets to the stored copy, then fetches the newest page. Sending while detached runs it first. A reconnect while detached only runs `fetch_changes` for the window (`sync.rs`). Opening another room and coming back opens at the newest: the jump is forgotten.
- **Events.** `Reset`, `Prepended` and `Appended` are reused. `TimelineStatus` gets `detached`, `loading_newer` and `found`, emitted before the window's reset. A one-shot `Event::ShowMessage { room, message }` follows the change that brings the target in; the cache does not keep it.

## The highlight

The state is the core's `TimelineStatus.found`. `rc_view::MessageList` holds the found id, and `set_found` returns the rows to announce, as `set_viewer` does. The search row's `picked` comes from it. It clears on another jump, `show_latest`, sending, another room or closing the room, and the deletion of the message; not on closing the search pane.

## What the Qt side does

- `MessageMirror` gets a `found` role; the message list model a `showRequested(row)` signal; `RoomVm` gets `detached`, `loadingNewer`, `loadNewer()` and `showLatest()`. `MessageSearch.pick` calls `show_message`.
- `RoomView.qml`: on `showRequested` it sets `followsNewest` to false and calls `positionViewAtIndex(row, ListView.Center)`; on a model reset `followsNewest` is `!Room.detached`; reaching the bottom while detached calls `loadNewer`, with a spinner in the header (the list is BottomToTop); End while detached calls `showLatest`.
- `MessageDelegate`: `msgFound` gives `Theme.ownTint` as the background and a `Theme.own` bar `Theme.accentBarWidth` wide.
- "Jump to latest": a floating pill centred above the bottom of the list, only while detached: `Theme.raised` fill (`raisedHover` hovered), `Theme.line` border, radius half the height, the chevron turned down tinted `Theme.own` and the text in `Theme.text`, `fontSizeSmall`; its sizes are in `Theme.qml`.
- A target that is gone: a note in the search pane.

## Thread replies

The room shows the parent, and the thread opens in the pane over the results; Back returns to the results. There is no "around" for threads, so the thread does not scroll to the reply. `ThreadListState` needs a "search held" input apart from `pane`; `back_steps` returns to the results; `open_thread_steps` from a result keeps the search; `RoomPage.qml` shows the search pane by `pane === "search"` and keeps it alive while hidden, so its scroll position survives.

## Steps

One PR each.

- **3a** (done): a refactor with no behaviour change in `rc_core`. `Fetch.older: bool` is `FetchKind`; the open room's saves go through `OpenRoom::persist`.
- **3b** (done): `rc_api` and the ports: `method_call`, `surrounding_messages`, `next_messages`, the adapter and the fake. wiremock tests in `rc_api/tests/rest.rs` (`result: false`, an error frame, a `401`); contract tests: the order, the edges, the flags, hidden replies, a deleted message giving `false`, a group, a direct message, `loadNextMessages` excluding the end, a group the user is not in.
- **3c**: the detached window in the core: unit tests in `timeline.rs` including a mirror following windows; `client/tests/jump.rs` (a held target makes no request; a second jump drops the first; live messages are dropped while detached and present after `show_latest`; `load_newer` joins; a missing or deleted message; reconnect, sending and another room; the store is never overwritten); an `rc_core` contract test: jump in a 120-message channel, then `load_newer` until attached.
- **3d**: `rc_view` and the Qt side, with GUI tests: click and Enter show the old message with `msgFound` in view; the pane stays with the row picked; scrolling down reaches the latest; the pill; the not-found note.
- **3e**: thread replies.

## Costs and risks

- One request of about 50 messages per jump to a message not held; the server uses the room and timestamp index.
- The window grows while scrolling and is not trimmed.
- Hidden thread replies use up the `limit`, so a window can be short: the view must ask for more while the content is shorter than the view.
- Inserting at row 0 of a BottomToTop list while detached may move the view (a GUI test covers it). Centring is imprecise until images load.
- The consistency of the window and the stored copy is the main correctness risk.
