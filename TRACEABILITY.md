# Traceability

Every test carries a `// <ID>` comment. A feature can have tests at several levels (unit; api against a mock HTTP server for `rc_api`; contract, `rc_api` and `rc_core` against the local test server, opt-in locally with `RC_TEST_SERVER_URL` and always in CI; GUI through Spix, which also needs the test server).

## App

| ID | Feature | Tests |
|---|---|---|
| APP-001 | App starts and shows the version from Rust | GUI: `crates/rc_gui_tests/tests/app.rs` |
| APP-002 | Logs to stderr, and to a file in developer-mode builds | core: `crates/rc_core/tests/logging.rs` |
| APP-003 | Settings menu with a Dump info item that logs the DPI info | GUI: `crates/rc_gui_tests/tests/app.rs` |

## Authentication

| ID | Feature | Tests |
|---|---|---|
| AUTH-001 | Log in with server URL, username and password | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/session.rs`, `crates/rc_qt/src/session_view.rs`, `crates/rc_qt/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-002 | Invalid credentials show an error | unit: `crates/rc_core/src/client/tests/login.rs`, `crates/rc_qt/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-003 | Unreachable or non-Rocket.Chat server shows an error | unit: `crates/rc_api/src/client.rs`, `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/types.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/session.rs`, `crates/rc_qt/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-004 | Session resumes at startup from the stored token | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_core/src/client/tests/rooms.rs`, `crates/rc_core/src/adapters/blob.rs`, `crates/rc_core/src/adapters/file.rs`, `crates/rc_core/src/adapters/memory.rs`, `crates/rc_core/src/adapters/keyring.rs`, `crates/rc_qt/src/hub.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-005 | Logout clears the stored token and returns to login | unit: `crates/rc_core/src/client/tests/login.rs`, `crates/rc_qt/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-006 | 2FA (TOTP / email code) | todo |
| AUTH-007 | Personal access token login | todo |
| AUTH-008 | OAuth/SSO login | todo |

## Accounts

| ID | Feature | Tests |
|---|---|---|
| ACCT-001 | Add an account on another server while signed in | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_qt/src/accounts.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/session_view.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |
| ACCT-002 | Every stored account resumes at startup, independently of the others, and tells who it is | unit (the store keeps a list; every stored account runs): `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/adapters/blob.rs`, `crates/rc_core/src/adapters/file.rs`, `crates/rc_core/src/adapters/memory.rs`, `crates/rc_core/src/adapters/keyring.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs` |
| ACCT-003 | Switching accounts shows that account's rooms, open room and thread | unit: `crates/rc_qt/src/accounts.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs` |
| ACCT-004 | Accounts in the background stay live, with their unread and mention totals | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_qt/src/accounts.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |
| ACCT-005 | Logging out of one account removes it and leaves the others signed in | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_qt/src/accounts.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |
| ACCT-006 | An expired account stays listed and logs in again with the server and user filled in | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_qt/src/accounts.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_core/src/client/tests/rooms.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs` |
| ACCT-007 | The same user on the same server can't be added twice | unit (rc_core): `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_core/src/users.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |

## Rooms

| ID | Feature | Tests |
|---|---|---|
| ROOM-001 | List subscribed rooms after login | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/rooms.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-002 | Room shows name and type (channel, private, DM) | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-003 | Room shows unread and mention counts | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs`, `crates/rc_gui_tests/tests/realtime.rs` |
| ROOM-004 | Opening a room marks it read, and it stays read while open | unit: `crates/rc_core/src/client/tests/mark_read.rs`, `crates/rc_core/src/domain.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs`, `crates/rc_gui_tests/tests/realtime.rs` |
| ROOM-005 | Rooms ordered by last activity | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/activity.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs`, `crates/rc_gui_tests/tests/realtime.rs` |
| ROOM-006 | Favorites first in the room list, in their own section | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/room_actions.rs`, `crates/rc_qt/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/room_menu.rs` |
| ROOM-007 | Room menu: favorite/unfavorite, mark read/unread, leave | unit: `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/room_actions.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/room_menu.rs` |
| ROOM-008 | Quiet direct messages (no activity for 10 days, not favorite, unread or open) are left out of the room list, and one comes back with a message | unit: `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/quiet.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-009 | The room header shows the room's topic, rendered as markdown | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/activity.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |

## Messages

| ID | Feature | Tests |
|---|---|---|
| MSG-001 | Opening a room shows its latest messages | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/message_store.rs`, `crates/rc_core/src/client/tests/timeline.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-002 | Scrolling to the top loads older messages | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/timeline.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-003 | Message shows author and timestamp | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_qt/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-004 | Day separators between days | unit: `crates/rc_qt/src/message_view.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-005 | System messages rendered distinctly | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_qt/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |

## Realtime

| ID | Feature | Tests |
|---|---|---|
| RT-001 | New message in the open room appears live | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_core/src/timeline.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |
| RT-002 | Unread counts of other rooms update live, and rooms joined or left show up | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/realtime.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |
| RT-003 | Reconnect after connection loss, catch up on missed messages | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_core/src/backoff.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_test_server/src/proxy.rs`; contract: `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |
| RT-004 | Connection status indicator | unit: `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/session_view.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |

## Sending

| ID | Feature | Tests |
|---|---|---|
| SEND-001 | Send a text message | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/sending.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/send.rs` |
| SEND-002 | Pending/failed state with retry | unit: `crates/rc_core/src/client/tests/sending.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/send.rs` |

## Rich content

| ID | Feature | Tests |
|---|---|---|
| RICH-001 | Markdown formatting (bold, italic, code, quote, lists, links) | unit: `crates/rc_api/src/md.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_qt/src/rich_html.rs`, `crates/rc_qt/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rich.rs` |
| RICH-002 | Mentions, @me highlighted | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_qt/src/rich_html.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/cache.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rich.rs` |
| RICH-003 | Unicode and custom emoji | unit: `crates/rc_api/src/md.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/emoji.rs`, `crates/rc_core/src/client/tests/emoji.rs`, `crates/rc_qt/src/rich_html.rs`, `crates/rc_qt/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rich.rs` |
| RICH-004 | Avatars (cached) | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_test_server/src/lib.rs`, `crates/rc_core/src/ports.rs`, `crates/rc_core/src/adapters/disk_cache.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/avatars.rs`, `crates/rc_core/src/client/tests/avatars.rs`, `crates/rc_qt/src/avatar_view.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_gui_tests/src/app.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/avatars.rs` |
| RICH-005 | Image attachments inline, other files as links | unit: `crates/rc_api/src/client.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/images.rs`, `crates/rc_core/src/ports.rs`, `crates/rc_core/src/client/tests/images.rs`, `crates/rc_qt/src/attachment_view.rs`, `crates/rc_qt/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/attachments.rs` |
| RICH-006 | Live message edits and deletes | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/changes.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edits.rs` |
| RICH-007 | Reactions displayed | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_core/src/client/tests/changes.rs`, `crates/rc_qt/src/reaction_view.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/reactions.rs` |
| RICH-008 | Right-click on a link: open it or copy its address | GUI: `crates/rc_gui_tests/tests/links.rs` |

## Interaction

| ID | Feature | Tests |
|---|---|---|
| ACT-001 | Edit own message | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/policy.rs`, `crates/rc_core/src/client/tests/message_actions.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edit_delete.rs` |
| ACT-002 | Delete own message | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/policy.rs`, `crates/rc_core/src/client/tests/message_actions.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edit_delete.rs` |
| ACT-003 | Add and remove reactions | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/emoji.rs`, `crates/rc_core/src/policy.rs`, `crates/rc_core/src/client/tests/message_actions.rs`, `crates/rc_qt/src/emoji_picker_view.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/reactions.rs` |
| ACT-004 | Threads: view and reply | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/client/tests/threads.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/thread_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/threads.rs` |
| ACT-005 | Reply/quote a message | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/quote.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/client/tests/quotes.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/quote_view.rs`, `crates/rc_qt/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edit_delete.rs`, `crates/rc_gui_tests/tests/quotes.rs` |
| ACT-006 | See who is typing in the open room and the open thread (received only, never sent) | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/typing.rs`, `crates/rc_core/src/client/tests/typing.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/cache.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs` |
| ACT-007 | Start a DM | todo |
| ACT-008 | Directory search and join channel | todo |
| ACT-009 | File upload with progress | todo |

## Persistence and desktop

| ID | Feature | Tests |
|---|---|---|
| PERS-001 | Startup from local cache, works offline read-only | todo |
| PERS-002 | Delta sync after cached startup | todo |
| DESK-001 | Desktop notifications for DMs and mentions | todo |
| DESK-002 | Unread indicator in window title or tray | todo |
| DESK-003 | Settings page | todo |

## Test infrastructure

| ID | Feature | Tests |
|---|---|---|
| INFRA-001 | XML-RPC client encodes Spix calls and decodes its answers and faults | unit: `crates/rc_gui_tests/src/xmlrpc.rs` |
| INFRA-002 | GUI test harness gives each app its own Spix port; reports an app that can't start, exits early or won't quit; passes environment overrides; waits for the UI with a timeout; addresses room rows and message rows by id | unit: `crates/rc_gui_tests/src/app.rs`, `crates/rc_gui_tests/src/gui.rs`, `crates/rc_gui_tests/src/paths.rs` |
| INFRA-003 | Developer builds' `--dev` fills in the login page for the local test server, with environment overrides; release builds have no such option | unit: `crates/rc_qt/src/dev_login.rs` |
