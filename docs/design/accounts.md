# Accounts

An account is one login on one server. `rc_core` runs any number of them at once, each with its own session actor (see [Threading](threading.md)).

## What an account is

- A local id, `AccountId`, made up when the account is added. It never comes from the server, and it is the key of the secret store.
- A stored entry (`StoredAccount`: id, server, username, user id and token, or no token), once the user has logged in. The store keeps the entries in the order they were first saved.
- The actor's state: logged out, connecting, resuming or logged in, with its room list, open room and so on.

## At least one

There is always at least one account. At startup the core adds every stored account, in stored order, each resumed if it has a token. With nothing stored it adds one logged-out account. When the last account is removed, the core adds a new logged-out one right after its `Removed`. So a frontend that shows one account logs out and lands on a login page.

## Adding and removing

- `Client::add_account()` returns the new id at once (the caller makes it up), and the core adds a logged-out account: `Added`, then `Session(LoggedOut)`.
- `Client::remove_account(&id)` cancels a login in flight, or logs out and revokes the token, and forgets the stored entry, also when the account is logged out. The events of the logout come, then `Removed`.
- `Client::account(&id)` is a borrowed handle with the commands of one account (`login`, `open_room`, `send_message`...). A command for an id that isn't an account (any more) is dropped and logged at debug.

## Events

Every event comes with its account's id. The events of one account arrive in order, from `Added` to `Removed`. `Added` is emitted before the actor is spawned, so nothing precedes it. `Removed` is emitted when the actor has ended, so everything it emitted precedes it, and nothing follows. Events of different accounts may interleave. Accounts are listed in the order of their `Added`.

## Who an account is

`Event::Identity` (server and username) says who an account is, so that a frontend can list it and prefill its login while it isn't logged in. It comes when it becomes known or changes, and only then:

- at startup for a stored account, right after `Added` and before its first `Session`;
- after a login, before `LoggedIn`, if it differs from what was sent: a first login, or an expired account logged in again as someone else.

It is never unset while the account lives. A new account has none until its first login.

## Expired accounts

When the server refuses the token, at resume or later, the actor saves the account again without its token (same id, server, username and user id) and goes `LoggedOut` with `SessionExpired`. The actor keeps the `StoredAccount` it resumed or saved for this, so it doesn't depend on the session being alive. The account stays listed. At startup a stored account without a token is added logged out, with its `Identity`, and nothing is resumed. Logging in on it saves the new token under the same id.

An unreachable or too old server doesn't change what is stored. Cancelling a resume, or removing the account in any state, forgets the entry.

## One user once

Two accounts are the same if their servers are the same address (the normalized URLs, ignoring a trailing `/` and ASCII case) and their user ids are equal. Two users on one server are fine, and one server under two addresses counts as two servers.

`Shared` holds a registry of who each account is (`Users`, in `users.rs`). The core fills it from the stored accounts at startup, tokenless ones included. When a login (not a resume) succeeds, the actor claims the user for its account, replacing its own entry. If another account is that user, the new token is revoked, nothing is saved, no `Identity` is sent and the account goes `LoggedOut` with `AlreadyAdded`. An account releases its entry when it forgets itself, which is before its `Removed`, so the user can be added again as soon as the removal is asked for.

The registry is behind a `Mutex`. That is fine here: accounts use it only when a login succeeds, and the lock is never held across an await or while calling `on_event`.

## Unread totals

`Event::Unread` (rooms and mentions) is what a frontend shows as the account's badge. `rooms` is how many rooms show as unread (`Room::has_unread`), `mentions` the sum of their mention badges (`Room::mention_badge`, the mentions of the user: `@all` and `@here` make a room unread but have no badge). `RoomList::unread` computes them, and `rc_qt` shows rooms by the same rules.

The actor sends it right after every `Rooms` event, if the totals differ from what it sent last. That includes the logout that empties the list. Nothing is sent while the totals stay zero from the start.

## The core task

One task owns the shared services, the accounts in order (id and command sender) and a `JoinSet` of the actor tasks. It loops over the `Client`'s commands and over the actors that end:

- Add: emit `Added`, spawn an actor.
- Remove: send the actor a logout (which forgets the stored entry and the user), then drop its sender, which stops it. The actor waits for its own token revocation (2 s at most) in its own task, so the other accounts are not held up. When its task ends, the core emits `Removed`, and adds a new account if none is left.
- A command for an account goes to its actor.
- The `Client` dropped: the core drops every sender, waits for all the actors, drops the services and gives the store worker until the latest actor deadline to empty its queue. No `Removed` comes then.

Each actor runs in a tracing span with its id, so the logs say which account a line is about.

## rc_qt

The hub (`hub.rs`) keeps an `Accounts` (`accounts.rs`, plain Rust): the accounts in the order of their `Added`, each with its own `Cache`, and the current account. The first account added is current. Every event goes to its account's cache, so an account in the background stays up to date, with its unread totals. An event of an account that isn't listed is ignored.

- **Current account**: the one the view models show. Only its events are forwarded to them, as they come. Commands (`login`, `open_room`, `send_message`...) go to it. The answers to a message or room action (`MessageAction`, `RoomAction`) of another account are dropped, since no view model shows that account. Logging out is `remove_account` of the current one.
- **Switching**: `select` makes another account current, and so does the removal of the current one (the first account left takes over). The hub then sends every registered view model a full snapshot of the new current cache, the same one a view model gets when it registers: the session, the room list and its avatars, the open room's state, messages, avatars and images, the thread's state and messages, and the custom emoji. Each snapshot replaces what the view model showed (a model reset for the lists). When the last account is removed there is no current account until the core adds the next, and the view models keep showing the removed one, logged out, until then.
- **`AccountList`**: a model with a row per account, in order: `accountId`, `server`, `host`, `username`, `status` (logged out, connecting, resuming, logged in or expired), `connected` (logged in with the realtime connection up), `unreadRooms`, `mentions` and `current`. `Added` and `Removed` insert and remove a row; `Identity`, `Unread`, `Session` and `Connection` update that account's row, and a switch updates the rows of the account that stopped being current and of the one that became so. All are row changes (`AccountChange`), never resets. Its invokables are `select(accountId)`, `add()` and `remove(accountId)`. `add()` has the core add an account, lists it at once (so the `Added` that follows changes nothing) and makes it current, so that the login page shows for it. `remove` is `remove_account`; the row goes when the core says `Removed`.
- **Session**: `Session.host` is the host and port of the logged-in server, so QML has no URL parsing.
