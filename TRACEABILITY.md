# Traceability

Every test carries a `// <ID>` comment. A feature can have tests at several levels (unit; api against a mock HTTP server for `rc_api`; contract, `rc_api` and `rc_core` against the local test server, opt-in locally with `RC_TEST_SERVER_URL` and always in CI; GUI through Spix, which also needs the test server).

The `Slint` column is how much of a feature's GUI tests run against `rocketchat_slint`, the Slint frontend: `done` when all of them are in `crates/rc_gui_tests/slint.txt` (the list ctest runs against that app), `partial` when some are, `todo` when none is, and `n/a` when the feature has no GUI test at all. INFRA-005 checks that the column and the list agree.

## App

| ID | Feature | Slint | Tests |
|---|---|---|---|
| APP-001 | App starts and shows the version from Rust | done | GUI: `crates/rc_gui_tests/tests/app.rs`; unit: `crates/rc_slint/src/lib.rs` |
| APP-002 | Logs to stderr, and to a file in developer-mode builds | n/a | core: `crates/rc_core/tests/logging.rs` |
| APP-003 | Settings menu with a Dump info item that logs the DPI info | done | GUI: `crates/rc_gui_tests/tests/app.rs` |

## Authentication

| ID | Feature | Slint | Tests |
|---|---|---|---|
| AUTH-001 | Log in with server URL, username and password | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/session.rs`, `crates/rc_view/src/session_view.rs`, `crates/rc_view/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-002 | Invalid credentials show an error | done | unit: `crates/rc_core/src/client/tests/login.rs`, `crates/rc_view/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-003 | Unreachable or non-Rocket.Chat server shows an error | done | unit: `crates/rc_api/src/client.rs`, `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/types.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/session.rs`, `crates/rc_view/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-004 | Session resumes at startup from the stored token | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_core/src/client/tests/rooms.rs`, `crates/rc_core/src/adapters/blob.rs`, `crates/rc_core/src/adapters/file.rs`, `crates/rc_core/src/adapters/memory.rs`, `crates/rc_core/src/adapters/keyring.rs`, `crates/rc_view/src/startup.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-005 | Logout clears the stored token and returns to login | done | unit: `crates/rc_core/src/client/tests/login.rs`, `crates/rc_view/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-006 | 2FA (TOTP / email code) | n/a | todo |
| AUTH-007 | Personal access token login | n/a | todo |
| AUTH-008 | OAuth/SSO login | n/a | todo |

## Accounts

| ID | Feature | Slint | Tests |
|---|---|---|---|
| ACCT-001 | Add an account on another server while signed in | done | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_view/src/accounts.rs`, `crates/rc_view/src/switchboard.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/session_view.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |
| ACCT-002 | Every stored account resumes at startup, independently of the others, and tells who it is | done | unit (the store keeps a list; every stored account runs): `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/adapters/blob.rs`, `crates/rc_core/src/adapters/file.rs`, `crates/rc_core/src/adapters/memory.rs`, `crates/rc_core/src/adapters/keyring.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs` |
| ACCT-003 | Switching accounts shows that account's rooms, open room and thread | done | unit: `crates/rc_view/src/accounts.rs`, `crates/rc_view/src/switchboard.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs` |
| ACCT-004 | Accounts in the background stay live, with their unread and mention totals | done | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_view/src/accounts.rs`, `crates/rc_view/src/switchboard.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |
| ACCT-005 | Logging out of one account removes it and leaves the others signed in | done | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_view/src/accounts.rs`, `crates/rc_view/src/switchboard.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |
| ACCT-006 | An expired account stays listed and logs in again with the server and user filled in | done | unit: `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_view/src/accounts.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_core/src/client/tests/login.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_core/src/client/tests/rooms.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs` |
| ACCT-007 | The same user on the same server can't be added twice | done | unit (rc_core): `crates/rc_core/src/client/tests/accounts.rs`, `crates/rc_core/src/users.rs`; GUI: `crates/rc_gui_tests/tests/accounts.rs`; contract: `crates/rc_core/tests/contract.rs` |

## Rooms

| ID | Feature | Slint | Tests |
|---|---|---|---|
| ROOM-001 | List subscribed rooms after login | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/rooms.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-002 | Room shows name and type (channel, private, DM) | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-003 | Room shows unread and mention counts | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs`, `crates/rc_gui_tests/tests/realtime.rs` |
| ROOM-004 | Opening a room marks it read, and it stays read while open | done | unit: `crates/rc_core/src/client/tests/mark_read.rs`, `crates/rc_core/src/domain.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs`, `crates/rc_gui_tests/tests/realtime.rs` |
| ROOM-005 | Rooms ordered by last activity | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/activity.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs`, `crates/rc_gui_tests/tests/realtime.rs` |
| ROOM-006 | Favorites first in the room list, in their own section | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/room_actions.rs`, `crates/rc_view/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/room_menu.rs` |
| ROOM-007 | Room menu: favorite/unfavorite, mark read/unread, leave | done | unit: `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/room_actions.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/room_menu.rs` |
| ROOM-008 | Quiet direct messages (no activity for 10 days, not favorite, unread or open) are left out of the room list, and one comes back with a message | done | unit: `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/quiet.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-009 | The room header shows the room's topic, rendered as markdown | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/activity.rs`, `crates/rc_view/src/rich_html.rs`, `crates/rc_slint/src/rich_markdown.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-010 | Search the room list by name with a field below it, quiet direct messages included: matching rooms replace the list while typing | done | unit: `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/downloads.rs`, `crates/rc_core/src/client/tests/quiet.rs`, `crates/rc_view/src/room_search.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_slint/src/room_search_model.rs`; GUI: `crates/rc_gui_tests/tests/room_search.rs` |
| ROOM-011 | Batched channels: the room list shows the news of channels and private groups in batches, at most every 25 minutes, unless they mention the user, are the open room or are direct messages | done | unit: `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests/batch.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |

## Messages

| ID | Feature | Slint | Tests |
|---|---|---|---|
| MSG-001 | Opening a room shows its latest messages | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/message_store.rs`, `crates/rc_core/src/client/tests/timeline.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_list.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-002 | Scrolling to the top loads older messages | done | unit: `crates/rc_slint/src/message_model.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/timeline.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-003 | Message shows author and timestamp | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_view/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-004 | Day separators between days | done | unit: `crates/rc_view/src/message_list.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_view/src/message_view.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-005 | System messages rendered distinctly | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_view/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-006 | A name that fits is shown whole, not elided | done | GUI: `crates/rc_gui_tests/tests/messages.rs` |

## Realtime

| ID | Feature | Slint | Tests |
|---|---|---|---|
| RT-001 | New message in the open room appears live | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_core/src/timeline.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |
| RT-002 | Unread counts of other rooms update live, and rooms joined or left show up | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/realtime.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |
| RT-003 | Reconnect after connection loss, catch up on missed messages | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_core/src/backoff.rs`, `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_test_server/src/proxy.rs`; contract: `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |
| RT-004 | Connection status indicator | done | unit: `crates/rc_core/src/client/tests/realtime.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/session_view.rs`; GUI: `crates/rc_gui_tests/tests/realtime.rs` |

## Sending

| ID | Feature | Slint | Tests |
|---|---|---|---|
| SEND-001 | Send a text message | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/sending.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/send.rs` |
| SEND-002 | Pending/failed state with retry | done | unit: `crates/rc_core/src/client/tests/sending.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/send.rs` |

## Rich content

| ID | Feature | Slint | Tests |
|---|---|---|---|
| RICH-001 | Markdown formatting (bold, italic, code, quote, lists, links) | done | unit: `crates/rc_api/src/md.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_view/src/rich_html.rs`, `crates/rc_view/src/rich_view.rs`, `crates/rc_slint/src/rich_markdown.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rich.rs` |
| RICH-002 | Mentions, @me highlighted | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_view/src/rich_html.rs`, `crates/rc_slint/src/rich_markdown.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/cache.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rich.rs` |
| RICH-003 | Unicode and custom emoji | partial | unit: `crates/rc_api/src/md.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/emoji.rs`, `crates/rc_core/src/client/tests/emoji.rs`, `crates/rc_view/src/rich_html.rs`, `crates/rc_slint/src/rich_markdown.rs`, `crates/rc_view/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rich.rs` |
| RICH-004 | Avatars (cached) | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_test_server/src/lib.rs`, `crates/rc_core/src/ports.rs`, `crates/rc_core/src/adapters/disk_cache.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/downloads.rs`, `crates/rc_core/src/client/tests/avatars.rs`, `crates/rc_view/src/avatar_view.rs`, `crates/rc_view/src/file_urls.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_list.rs`, `crates/rc_gui_tests/src/app.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/avatars.rs` |
| RICH-005 | Image attachments inline, other files as links | done | unit: `crates/rc_api/src/client.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/images.rs`, `crates/rc_core/src/downloads.rs`, `crates/rc_core/src/ports.rs`, `crates/rc_core/src/client/tests/images.rs`, `crates/rc_view/src/attachment_view.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_list.rs`, `crates/rc_slint/src/message_model.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/attachments.rs` |
| RICH-006 | Live message edits and deletes | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/client/tests/changes.rs`, `crates/rc_view/src/row_ops.rs`, `crates/rc_slint/src/message_model.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edits.rs` |
| RICH-007 | Reactions displayed | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_core/src/client/tests/changes.rs`, `crates/rc_core/src/client/tests/emoji_images.rs`, `crates/rc_view/src/reaction_view.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_list.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/reactions.rs` |
| RICH-008 | Right-click on a link: open it or copy its address | todo | GUI: `crates/rc_gui_tests/tests/links.rs` |

## Interaction

| ID | Feature | Slint | Tests |
|---|---|---|---|
| ACT-001 | Edit own message | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/policy.rs`, `crates/rc_core/src/client/tests/message_actions.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edit_delete.rs` |
| ACT-002 | Delete own message | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/policy.rs`, `crates/rc_core/src/client/tests/message_actions.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edit_delete.rs` |
| ACT-003 | Add and remove reactions | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/emoji.rs`, `crates/rc_core/src/policy.rs`, `crates/rc_core/src/client/tests/message_actions.rs`, `crates/rc_view/src/emoji_picker_view.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_slint/src/emoji_picker_model.rs`, `crates/rc_slint/src/message_model.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/reactions.rs` |
| ACT-004 | Threads: view and reply | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/client/tests/threads.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/thread_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/threads.rs` |
| ACT-005 | Reply/quote a message | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/quote.rs`, `crates/rc_core/src/rich.rs`, `crates/rc_core/src/client/tests/quotes.rs`, `crates/rc_slint/src/message_model.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/message_view.rs`, `crates/rc_view/src/quote_view.rs`, `crates/rc_view/src/rich_html.rs`, `crates/rc_view/src/room_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/edit_delete.rs`, `crates/rc_gui_tests/tests/quotes.rs` |
| ACT-006 | See who is typing in the open room and the open thread (received only, never sent) | done | unit: `crates/rc_api/src/ddp.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/rest.rs`, `crates/rc_core/src/typing.rs`, `crates/rc_core/src/client/tests/typing.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/cache.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/typing.rs` |
| ACT-007 | Start a DM | todo | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/directory.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/directory.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/directory.rs` |
| ACT-008 | Directory search and join channel | todo | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests/directory.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/directory.rs`, `crates/rc_view/src/room_view.rs`, `crates/rc_view/src/switchboard.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/directory.rs` |
| ACT-009 | File upload with progress | n/a | todo |
| ACT-010 | A room's threads, listed in the pane beside it: all or followed, newest reply first | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/threads.rs`, `crates/rc_core/src/client/tests/thread_list.rs`, `crates/rc_slint/src/thread_list_model.rs`, `crates/rc_view/src/cache.rs`, `crates/rc_view/src/thread_list_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/thread_list.rs` |
| ACT-011 | Threads with unread replies: the header badge, the emphasis of a row and the Unread filter | done | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/threads.rs`, `crates/rc_core/src/client/tests/thread_list.rs`, `crates/rc_slint/src/thread_list_model.rs`, `crates/rc_view/src/thread_list_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/thread_list.rs` |

## Persistence and desktop

| ID | Feature | Slint | Tests |
|---|---|---|---|
| PERS-001 | Startup from local cache, works offline read-only | n/a | todo |
| PERS-002 | Delta sync after cached startup | n/a | todo |
| DESK-001 | Desktop notifications for DMs and mentions | n/a | todo |
| DESK-002 | Unread indicator in window title or tray | n/a | todo |
| DESK-003 | Settings page | n/a | todo |

## Test infrastructure

| ID | Feature | Slint | Tests |
|---|---|---|---|
| INFRA-001 | XML-RPC client encodes Spix calls and decodes its answers and faults | n/a | unit: `crates/rc_gui_tests/src/xmlrpc.rs` |
| INFRA-002 | GUI test harness gives each app its own Spix port; reports an app that can't start, exits early or won't quit; passes environment overrides; waits for the UI with a timeout; addresses room rows, search result rows and message rows by id; takes an element's screenshot as PNG pixels | n/a | unit: `crates/rc_gui_tests/src/app.rs`, `crates/rc_gui_tests/src/gui.rs`, `crates/rc_gui_tests/src/paths.rs` |
| INFRA-003 | Developer builds' `--dev` fills in the login page for the local test server, with environment overrides; release builds have no such option | n/a | unit: `crates/rc_view/src/dev_login.rs` |
| INFRA-004 | Spix server for the Slint app: the calls of the GUI test suite over XML-RPC, paths by `accessible-id`, the properties the tests read, input, `takeScreenshot` (a PNG of an element cropped from the window's snapshot), and `quit` ending the event loop | n/a | unit: `3rdparty/spix/rust/spix_slint/src/`; wire: `3rdparty/spix/rust/spix_slint/tests/server.rs` |
| INFRA-005 | `slint.txt` and TRACEABILITY's `Slint` column agree: every line names a GUI test that exists, once, and each feature's cell says what the list says | n/a | unit: `crates/rc_gui_tests/src/slint_list.rs` |
| INFRA-006 | CI runs only the checks a pull request's changed paths need, and everything for a path it doesn't know | n/a | script: `scripts/ci_scope_test.sh` |
