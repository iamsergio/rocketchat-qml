# Accounts

An account is one login on one server. `rc_core` runs any number of them at once, each with its own session actor (see [Threading](threading.md)).

## What an account is

- A local id, `AccountId`, made up when the account is added. It never comes from the server, and it is the key of the secret store.
- A stored entry (`StoredAccount`: id, server, user and token, or no token), once the user has logged in. The store keeps the entries in the order they were first saved.
- The actor's state: logged out, connecting, resuming or logged in, with its room list, open room and so on.

## At least one

There is always at least one account. At startup the core adds every stored account, in stored order, each resumed if it has a token. With nothing stored it adds one logged-out account. When the last account is removed, the core adds a new logged-out one right after its `Removed`. So a frontend that shows one account logs out and lands on a login page.

## Adding and removing

- `Client::add_account()` returns the new id at once (the caller makes it up), and the core adds a logged-out account: `Added`, then `Session(LoggedOut)`.
- `Client::remove_account(&id)` cancels a login in flight, or logs out and revokes the token, and forgets the stored entry, also when the account is logged out. The events of the logout come, then `Removed`.
- `Client::account(&id)` is a borrowed handle with the commands of one account (`login`, `open_room`, `send_message`...). A command for an id that isn't an account (any more) is dropped and logged at debug.

## Events

Every event comes with its account's id. The events of one account arrive in order, from `Added` to `Removed`. `Added` is emitted before the actor is spawned, so nothing precedes it. `Removed` is emitted when the actor has ended, so everything it emitted precedes it, and nothing follows. Events of different accounts may interleave. Accounts are listed in the order of their `Added`.

## The core task

One task owns the shared services, the accounts in order (id and command sender) and a `JoinSet` of the actor tasks. It loops over the `Client`'s commands and over the actors that end:

- Add: emit `Added`, spawn an actor.
- Remove: send the actor a logout, then drop its sender, which stops it. The actor waits for its own token revocation (2 s at most) in its own task, so the other accounts are not held up. When its task ends, the core emits `Removed`, and adds a new account if none is left.
- A command for an account goes to its actor.
- The `Client` dropped: the core drops every sender, waits for all the actors, drops the services and gives the store worker until the latest actor deadline to empty its queue. No `Removed` comes then.

Each actor runs in a tracing span with its id, so the logs say which account a line is about.

## rc_qt

The hub follows one account, the current one: the first `Added`. It ignores the events of the others, and the commands go to the current account only. When the current account is removed, the current account becomes none until the next `Added`, which takes its place. Logging out is `remove_account`, so the app ends on a logged-out account as before; the cache already shows logged out by the time of `Removed`.
