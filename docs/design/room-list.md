# The room list

- **Order and sections:** `rooms::display_order` puts the user's favorites first, and orders each group by last activity (most recent first), then name (ignoring case), then id. The list is one list: a favorite that is set or taken back is an `upsert` whose position changes, which comes out as a `Moved` change (or `Updated` if it stays where it is), like any other change of order; a message in a room that is not a favorite moves it up only as far as the first non-favorite. `Room.favorite` comes from the subscription's `f`, which is absent for a room never favorited; `subscriptions.get` and the subscription the server pushes are the same wire type, so a favorite set by another client arrives live.
- **Sections:** the GUI shows the headings "Favorites" and "Rooms" only while there are favorites, so a user without any sees a plain list.

## The room menu

Right-click on a row. Its rules are the core's, so a frontend only offers what the core will accept:

- **Add to / remove from favorites:** all rooms.
- **Mark as read** when the room shows something unread (`Room::has_unread`: unread, mentions or alert), else **mark as unread**. The open room stays read while it is open (ROOM-004), so marking it unread is refused by the core and the menu doesn't offer it.
- **Leave:** channels and private groups (`Room::can_leave`), after a confirmation naming the room. Direct messages can't be left.

## Server calls and what they push

`Client::set_favorite`, `mark_read`, `mark_unread` and `leave_room` end in one `Event::RoomAction` (room, `RoomAction`, and `Ok` or a `RoomActionError` with a text to show), as for messages. A room that is not in the list, a direct message to leave and the open room to mark unread are refused at once, without a request. Ignored when logged out; an answer after a logout is dropped; a dead token ends the session.

- `POST /api/v1/rooms.favorite` with `roomId` and `favorite`. The request says which of the two is wanted, so repeating it changes nothing. The server refuses a room the user isn't in ("You must be part of a room to favorite it"). It pushes the subscription with `f`.
- `POST /api/v1/subscriptions.read` with `rid` (the call opening a room makes too): zeroes the counts and the alert.
- `POST /api/v1/subscriptions.unread` with `roomId`. Whatever the number of messages, the subscription ends with `unread` 1 and `alert` true, the mentions untouched (it counts from the newest message of somebody else); calling it again changes nothing. A room without messages is refused ("There are no messages to mark unread [error-no-message-for-unread]"). The change is pushed.
- `POST /api/v1/channels.leave` or `groups.leave` with `roomId`, by the kind of room (the other one answers "room not found"). The subscription is removed and pushed as removed. The last owner of a room is refused with "You are the last owner. Please set new owner before leaving the room. [error-you-are-last-owner]", which is shown as the server words it, after "The server reported an error: the server answered 400:".

## Applying at once

On success the core applies the change to the room as it is by then, without waiting for the push, which then changes nothing: a favorite is set, not toggled; mark read clears the counts and the alert (`Room::read`); mark unread sets `unread` to at least 1 and the alert, which is what the push says; leaving removes the room, and closes it if it was open, as a removal that the server pushes does. A room opened meanwhile stays read, as for any change of the open room. An answer for a room that has gone changes nothing.

## Qt side

- **`RoomList` model:** roles `favorite`, `hasUnread` and `canLeave` besides the old ones, and the properties `hasFavorites` (recomputed after each batch of changes, announced when it flips) and `roomActionError` (the text of the last failure, cleared by the next command and by `dismissRoomActionError`). The invokables `setFavorite`, `markRead`, `markUnread` and `leave` take a room id. The model gets `Event::RoomAction` from the hub like the `Room` view model gets message actions.
- **QML:** one `ListView` with `section.property: "favorite"` (a bool section value is the text "true" or "false"). The section item always exists and has no height without favorites, because the view decides whether it is visible. Menu items take their text and visibility from the roles, with no branching in QML: `roomMarkReadAction` is visible when `hasUnread`, and `roomMarkUnreadAction` when not `hasUnread` and the row is not the open room. The menu and the confirmation are parented to the window's overlay while open.
- **Spix:** the menu opens with a right-click (`MouseArea`, as `TapHandler` doesn't react to Spix clicks) on the row `room_<id>`; its items and the dialog are addressed from the window and exist only while open. A test closes the menu with Escape. See [GUI testing contract](gui-testing.md) for the names.
