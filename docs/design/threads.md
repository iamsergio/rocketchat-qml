# Threads

A thread is a message, its parent, and the replies to it. The parent gets a count and the time of the last reply; a reply carries the id of its parent and says whether the server also shows it in the channel. The app shows the open thread in a pane beside the room.

## What the server does

The contract tests (`crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`) record it.

- **Wire:** a parent has `tcount` (the number of replies) and `tlm` (the time of the last one); a reply has `tmid` (its parent) and `tshow` (also shown in the channel). `rc_api` reads them into `Message`'s `thread_replies`, `thread_last_reply`, `thread_id` and `thread_shown`.
- **Reading:** `chat.getThreadMessages` gives a page of the replies, newest first and without the parent, with the `offset` to continue from and the `total`, which says when a page ends the thread. The channel's history lists the replies that were also sent to the channel, and none of the others.
- **Sending:** `chat.sendMessage` with `tmid` and `tshow`. The first reply makes the message a parent. The server also stores a reply to a message of another room.
- **Pushes:** a reply is pushed on `__my_messages__` like any message, shown in the channel or not. The parent is pushed again, before the reply, with its new count and last reply time.
- **Deleting:** a deleted reply is announced only as a deleted message. The parent is not pushed: the server counts one less and leaves the time of the last reply alone, so the core does the same to the copies it holds. A parent deleted while it has replies is not removed, but pushed as a message of the removed kind (`rm`), which closes the thread.
- **One message:** `chat.getMessage` gives any message by its id, whatever its room, if the user may read that room. An id that is missing or unreadable is a `Server` error.
- **Catch-up:** `chat.syncMessages` lists changed and deleted replies, and the parent.

## The core

`Client::open_thread(parent)` shows the replies of a message of the open room beside it, one thread at a time. The room needn't hold the message.

- **Which message:** a sent text message of the open room that is not a reply itself. Anything else is logged at debug and ignored. So is the thread that is open already, unless its last load failed: then it tries again. Ignored unless logged in, and with no room open.
- **A parent the room doesn't hold:** it has left the window, or the frontend knows it from somewhere else. The core fetches it (`ServerApi::message`) and opens the thread when it arrives, if it is a sent text message that is not a reply and is in the open room. Meanwhile nothing is announced and the thread that was open stays. The fetch has an attempt id of the same counter as the pages, and the last thing asked for wins: another `open_thread` (also for the thread that is open), `close_thread`, another room or none, and logout drop the answer. A reconnect doesn't. A message of another room, a reply, a removed message, an id the server doesn't have and a failed fetch all end with nothing opened and nothing said to the frontend (a debug log for the first four and a warning for the failure; an expired token expires the session as everywhere). A parent pushed while the fetch is on its way replaces its answer, and its deletion drops the fetch.
- **The rows:** the parent first, then the replies, oldest first. The parent joins with the page that reaches the start, so older pages are plain prepends. Pages are asked by the number of replies held, with attempt ids like the room's, so stale answers are dropped.
- **Events:** `ThreadTimeline` carries the changes and `ThreadStatus` the parent, the loading flags and the error, as `Timeline` and `TimelineStatus` do for the room. Opening resets the thread's list to nothing first.
- **Pushes:** a pushed reply goes to the thread; one also sent to the channel goes to the room too. The parent's updates go to both, or to the thread alone if the room doesn't hold the parent (a push from before the room's window is not taken in). The open thread keeps its own copy of the parent, so a deleted reply is counted off it, and a deleted parent closes the thread, also when the room doesn't hold it. The catch-up of the room brings the parent's new copy to the thread wherever it is.
- **Actions:** edits, deletions, reactions, retries and quotes find a message in either timeline and apply to every one that holds it. A reply is sent only to the open thread (`send_message` with `thread`, which must be the open one) and shows as pending in the thread, and in the room if it also goes to the channel.
- **Closing:** `close_thread` closes it, and it closes by itself with its room (another room is opened, or the room closes or leaves the list), on logout and when the parent goes. The newest replies are fetched again on every reconnect. Nothing of a thread is kept.

## Qt side

- **Models:** `MessageList` and the new `ThreadMessageList` are thin bridges over one `MessageMirror` (`message_mirror.rs`), which holds the messages newest first, the viewer, the avatars and the images, and has the roles, the application of a `TimelineChange` through `row_ops`, and the three setters. The calls that announce the changes are a macro (`message_model!`), because they name the bridge's own type (see CXX-QT.md). Both are singletons, as the emoji catalog is: qmllint can't resolve a list model that QML creates.
- **Roles, on both:** `threadReplies` (the count, 0 unless it is a parent), `threadLastReply` (the time of day if it is today, else the heading of its day, empty without replies), `threadId` (the parent of a reply, else empty), `inThreadShown` (a reply also shown in the room), `canStartThread` (a sent text message that is not a reply: the rule of `open_thread`) and `threadParentText` (the parent's text on one line, if the model holds the parent, for the line above a reply in the room). The helpers are in `message_view.rs`.
- **`Thread`:** a singleton view model like `Room`. Properties `parentId`, `isOpen` (not `open`, which is the command: a property and an invokable of one name clash in QML), `loading`, `loadingOlder`, `reachedStart`, `errorText` and `roomName`; invokables `open(messageId)`, `close()`, `loadOlder()`, `send(text, quotedId, alsoToChannel)`, `lastEditable()`, `messageText(id)` and `messageAuthor(id)`. What it shows is `thread_view.rs`, from the status and the room list (the name follows a rename).
- **Hub and cache:** the cache mirrors the thread's messages and status next to the room's, so that a model or the view model created late catches up under the same lock. `ThreadTimeline` goes to `ThreadMessageList`, `ThreadStatus` (and any change of the room list or the room's status, which the name and the room to send to depend on) to `Thread`. Avatars, images and viewer changes go to both models. `messageText`, `messageAuthor` and the lookup of the newest editable message find a message in the thread too, so `Room.edit`, `remove`, `react` and `retry` work on a message in the pane, and their errors show in the room's one action-error bar.
- **Sending:** `Thread.send` builds the reply from the cache (`outgoing_reply`: the open room, the open thread, the quote) and hands it to `send_message` with the thread and `alsoToChannel`. It returns whether there was anything to send, so the composer clears itself.

## QML

- **Pane:** `ThreadPane.qml`, in `RoomPage` after `RoomView`, `Theme.threadPaneWidth` wide, shown while it has something to show (`ThreadList.paneOpen`: a thread or the list of threads, see below), with a hairline at its left. The header has, for a thread, the back button and the title, then `# room` and the close button; then the list (bottom to top like the room's, loading older replies at the top, with its busy indicators and error label) and a `Composer` with `forThread`. The window stays 900 wide by default, so the room is narrow beside the pane.
- **Delegate:** `MessageDelegate` is the room's and the pane's, with `inThreadPane` set in the pane. Under the parent in the pane it draws the "N replies" divider. In the room:
  - a parent with replies has a summary button under its text, files and reactions: the thread icon, "N replies" and "Last reply HH:MM". The core has no list of who replied, so there are no faces. While its thread is open the button is washed in `ownWash` and the row has the faint `ownTint`, with no edge, which means "about you";
  - "Reply in thread" leads the menu of every message that `canStartThread`, also one that has replies;
  - a reply also sent to the room has a faint line above its text, a corner arrow and "Replied in a thread: <start of the parent>", or just "Replied in a thread" if the room doesn't hold the parent. Clicking it opens the thread, and the core fetches the parent if the room doesn't hold it.
  None of the three is drawn in the pane. A long author name gives way to the time and the menu button (`maximumWidth: implicitWidth` with `fillWidth`; a `minimumWidth` of 0 did not shrink the row).
- **Composer:** `forThread` picks the target of the reads (`Thread` or `Room`, as the `target` property) and of the send; edits still go to `Room`. The placeholder is "Reply in the thread". Under the text, inside the box, a check box "Also send to #room" is off by default and off again after each send, and when another thread opens. Escape with no edit or quote to cancel goes back to the list of threads (`ThreadList.escape()`). The pane takes the focus when it opens.
- **Closing:** the close button (which closes the thread and the list), and opening another room (the core does it). Escape goes back to the list: see the thread list below.
- **Not done:** the faces of the repliers, a count of unread replies, and a notification for a reply in a thread the user doesn't have open.

## Slint

- **Pane:** `thread_pane.slint` in `RoomPage` after the room, `Theme.thread-pane-width` wide, there while `thread-id` (the open thread's parent, `ThreadState.parent_id`) is not empty. The header has the title, `# room` and the close button, with room left of the title for the list's back button. The list is the room's `MessageList` with `in-thread-pane`, the box a second `Composer` with `for-thread`.
- **Model:** a second `MessageModel` (`in_pane`), fed by `Event::ThreadTimeline`, with the viewer, avatars, images and emoji images of the room's. `show_all` rebuilds it from `Cache::thread_snapshot` and `show_thread` shows `Cache::thread_state` (parent, room name, loading flags, error, typing), also after `Event::Rooms`, `TimelineStatus`, `ThreadStatus` and `ThreadTyping`.
- **Row:** `rows::message_row` fills `can-start-thread`, `thread-replies`, `thread-last-reply`, `thread-id`, `in-thread-shown` and `thread-parent-text` from the helpers of `message_view`. In the pane `can-start-thread` and `in-thread-shown` are false, so the menu has no "Reply in thread" and no line leads anywhere. Which thread is open is not in the row: the delegate compares `open-thread-id` with its message.
- **Commands:** `app/thread_pane.rs`: open, close (`close_pane_steps`), Escape (`escape_steps`), older replies, send (`outgoing_reply`, the box's check box as `also_to_channel`) and the newest editable message of the thread. Edits, deletes, reactions and retries are the room's commands: `Cache::held` finds the pane's messages too.
- **Not in Slint yet:** the thread list, its button and badge, and the back button (steps 2 and 3).

## Thread list

A room's threads, in the pane beside it (ACT-010): all of them, or the ones the user follows, the one with the most recent reply first.

### What the server does

The contract tests record it (`crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`).

- **Reading:** `chat.getThreadsList?rid=<room>&count=&offset=` gives the parent messages, ordered by `tlm` (the time of the last reply) newest first, with `total`. Every thread is what it lists without a `type`: `type=all` is refused (400), `following` and `unread` are the other two (see Unread threads below). A message with no replies is not a thread.
- **Followers:** the parent carries `replies`, the ids of the users who follow its thread: the author and each user who replied, once the first reply is in; `chat.followMessage` and `chat.unfollowMessage` add and take the user. A reply by somebody else doesn't bring back an unfollower. `type=following` is the threads whose `replies` has the user. `rc_api` reads it into `Message::thread_followers`, the core into `thread_followers: Vec<UserId>` (empty on a copy that has none to say).
- **Pushes:** the parent is pushed again before each reply, with its new `tcount`, `tlm` and `replies`: that is how a thread moves, appears (the first reply) and, for the followed ones, appears when the user replies. Unfollowing is not announced: the next push of the parent says so, and a list of the followed threads drops it then.

### The core

`Client::open_thread_list(filter)`, `close_thread_list()` and `load_more_threads()`, next to the thread's, with `ThreadFilter` (`All`, `Following`) in `rc_core::threads`. The list is of the open room, independent of the open thread: both can be open, and the frontend decides which one it shows.

- **Opening:** ignored unless logged in and with a room open. Another filter than the open list's starts again from the first page; the same one does nothing, unless the last request failed (then it tries again). It starts with a reset to nothing and `ThreadListStatus { loading }`; the first page is a reset to its rows, the later ones are one `Inserted` per row at the end. A page is asked by the number of rows held as the offset, and a row already held is skipped, as a thread that moved to the front shifts the pages. Attempt ids come from the counter of the history and thread fetches, so a page of a filter that was left, a list that was closed, another room or a logout is dropped. The page size is `PAGE_SIZE`, or `RC_THREADS_PAGE_SIZE` for the GUI tests (as `RC_QUIET_AFTER_SECS` is).
- **Events:** `Event::ThreadList(Vec<ThreadListChange>)`, with `ThreadListChange = ListChange<Message>`: the shape of `RoomChange` (reset, inserted, updated, moved, removed), which is the same generic type now (`rc_core::list_change`; behaviour-neutral for the room list). `Event::ThreadListStatus(ThreadListStatus { room, filter, loading, loading_more, reached_end, error })`: `room` says it is open. It closes (a reset to nothing and the default status) with its room, when another room opens, and on logout.
- **Live updates** (`ThreadList` in `threads.rs`, a plain list with its rules): a pushed parent that is in the list is updated where it is, and moved to the front if its last reply is newer; one that is not held and has a reply is inserted before the first row that is not newer, unless that is past the last row and more pages are to come (it belongs among them); one the list shouldn't hold (no replies, removed kind, or for the followed ones no user among the followers) leaves it. A deleted parent leaves it. A deleted reply counts one less in the row, as in the room's copy (`reply_removed`). Edits and reactions the user makes change the row in place.
- **Reconnect:** the first page is fetched again, silently: if the list starts with it, nothing changes; if not, it is a reset to the page (a reset, not fine-grained changes: the list is short-lived and the case is rare).
- **`open_thread`:** looks in the thread list too before it fetches the parent, so a thread opened from a row needs no request.

### rc_view and Qt

- **`thread_list_view.rs`:** the texts of a row (`row_text`: the start of the parent on one line; `row_summary`: "N replies"), the `ThreadListState` (what the pane shows, the filter, the loading flags, the error, the empty words) and the steps of the commands on the pane (`back_steps`, `close_pane_steps`, `toggle_steps`, `escape_steps`), so the rules are in Rust and Slint can use them. The pane shows the thread when one is open, else the list when it is open, else nothing. The cache keeps the list and its status, and the filter last used in each room, for the back button.
- **`ThreadListModel`:** a QML singleton list model (`thread_list_model.rs`) with fine-grained row operations (`row_ops` is generic over `ListChange` now), roles `threadId`, `author`, `lastReply`, `body`, `replies`, `summary`, `avatar`, `avatarInitials` and `avatarColor`.
- **`ThreadList`:** a view model singleton (`thread_list_vm.rs`) with `pane`, `paneOpen`, `listOpen`, `filter`, `loading`, `loadingMore`, `reachedEnd`, `errorText`, `roomName`, `empty`, `emptyTitle` and `emptyHint`, and the invokables `openList(filter)`, `closeList()`, `loadMore()`, `back()`, `closePane()`, `toggle()` and `escape()`. The back button closes the thread and opens the list if it wasn't (with the last filter used in the room, else "All"); the close button closes both; the header button toggles; Escape in a thread is back, in the list it closes the list.

### QML

`roomThreadsButton` in the room's header is lit while the pane shows the list or a thread. `ThreadPane` holds the thread (kept hidden while the list shows) and `ThreadListPage`, which stays alive under a thread so its scroll position stays: the header ("Threads", `# room`, close), the compact segmented filter ("All", "Following", "Unread" and its count), the rows (`ThreadListRow`: 28 px avatar, author, time of the last reply, the text clamped to two lines, the thread icon and "N replies", and the marks of an unread thread), a busy indicator at the end while the next page loads, the error with a retry button and the empty words. The next page is asked when the end of the list is in view, also when a page leaves the list short. The thread's header has `threadBackButton`. The names are in [GUI testing](gui-testing.md).

## Unread threads

The header button's badge, the emphasis of a row and the "Unread" filter (ACT-011). All of it comes from the room's subscription.

### What the server does

The contract tests record it (`crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`), on RC 8.0.

- **Fields:** the subscription (`subscriptions.get` and the `subscriptions-changed` pushes) has `tunread`, the ids of the threads with replies the user hasn't read, `tunreadUser`, those among them with a reply that mentions the user, and `tunreadGroup`, those with `@all` or `@here`. A field is left out while it is empty, which reads as empty. `rc_api` reads them into `Subscription::thread_unread`, `thread_unread_user` and `thread_unread_group`; the core's `Room` keeps `unread_threads` and `mentioned_threads` (the user's mentions only, as the room's mention badge counts only those). The room's `alert` flag is set too, which the room list already shows.
- **When they change:** a reply by somebody else in a thread the user follows (they started it or replied to it) adds the thread to `tunread`. A plain reply in a thread they don't follow does nothing. A reply that mentions them adds the thread to `tunread` and `tunreadUser`, followed or not. `@all` in a followed thread goes to `tunreadGroup`. The changed subscription is pushed each time.
- **No count:** nothing says how many replies of a thread are unread. A row says that it is unread ("New"), not how many are.
- **`type=unread`:** `chat.getThreadsList?type=unread` lists exactly the threads of `tunread`.
- **Reading:** fetching the replies (`chat.getThreadMessages`, `chat.syncThreadMessages`) doesn't read a thread, and there is no REST call that does. The web client calls the DDP method `readThreads` with the parent's id: the server takes the thread out of `tunread` (and `tunreadUser`) and pushes the subscription. Reading a thread that isn't unread is no error. `rc_api` has `DdpClient::read_thread`.

### The core

- **The sets** come with the room (`Room::unread_threads`, `mentioned_threads`) like the unread counts, and change with its subscription pushes (`Event::Rooms`, an `Updated` of the room). No new event.
- **Marking read:** `Realtime::read_threads` is a channel to the connection, next to `watch`. The actor sends the id of the open thread when the room's subscription lists it as unread: when the thread opens, when a push brings it (a reply that came while the thread is open), and when a connection opens (a new connection hasn't been asked). Once for each time it is unread (`read_requested`, cleared when the set no longer holds it). Without a connection it is not sent, and the next push or the next connection does.
- **The "Unread" filter** is `type=unread`, which agrees with `tunread`. The list's `pushed` takes the room's set as a rule for it: a parent that is unread is listed, any other leaves. When a push changes the set, the actor removes the rows that were read (`Removed`) and fetches the parent of each thread that became unread and isn't held (`chat.getMessage`, since the push carries ids only) and puts it where its last reply says (`Inserted`). A parent pushed with a new reply either way updates or moves its row.
- **The rows' marks** are not a change of the list: the frontend takes them from the room's sets.

### rc_view, Qt and QML

- `ThreadListState` has `unread_count` and `unread_mention` (a mentioned thread that is still unread), and the empty words of the filter ("No unread threads in #room", "New replies in a thread you follow show up here."). `ThreadUnread` (the sets of the open room, from the cache) says which rows look unread and mentioned, and `rows_changed` which rows to announce.
- `ThreadListModel` has the roles `unread` and `mentioned`, and `set_unread`, which the hub calls on each `Rooms` event: `dataChanged` for those two roles, and only for the rows that look different. `ThreadList` has `unreadCount` and `unreadMention`.
- **QML:** `roomThreadsBadge` is a pill at the top right of the header button, with a ring in the header's colour around it (`roomThreadsBadgeRing`), grey or red when a thread mentions the user, and not shown at 0. A row with unread replies has its text in the text colour at medium weight, a grey "New" pill (`threadListNew`) and, if it mentions the user, a red "@" (`threadListMention`) at the end of its last line. The third segment, `threadListFilterUnread`, has its count in faint text after the label while there are unread threads.

