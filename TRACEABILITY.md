# Traceability

Every test carries a `// <ID>` comment. A feature can have tests at several levels (unit; api against a mock HTTP server for `rc_api`; contract, `rc_api` and `rc_core` against the local test server, opt-in locally with `RC_TEST_SERVER_URL` and always in CI; GUI through Spix, which also needs the test server).

## App

| ID | Feature | Tests |
|---|---|---|
| APP-001 | App starts and shows the version from Rust | GUI: `crates/rc_gui_tests/tests/app.rs` |
| APP-002 | Logs to stderr, and to a file in developer-mode builds | core: `crates/rc_core/tests/logging.rs` |

## Authentication

| ID | Feature | Tests |
|---|---|---|
| AUTH-001 | Log in with server URL, username and password | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests.rs`, `crates/rc_core/src/session.rs`, `crates/rc_qt/src/session_view.rs`, `crates/rc_qt/src/cache.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-002 | Invalid credentials show an error | unit: `crates/rc_core/src/client/tests.rs`, `crates/rc_qt/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-003 | Unreachable or non-Rocket.Chat server shows an error | unit: `crates/rc_api/src/client.rs`, `crates/rc_api/src/types.rs`, `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests.rs`, `crates/rc_core/src/session.rs`, `crates/rc_qt/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-004 | Session resumes at startup from the stored token | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests.rs`, `crates/rc_core/src/adapters/blob.rs`, `crates/rc_core/src/adapters/file.rs`, `crates/rc_core/src/adapters/memory.rs`, `crates/rc_core/src/adapters/keyring.rs`, `crates/rc_qt/src/hub.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-005 | Logout clears the stored token and returns to login | unit: `crates/rc_core/src/client/tests.rs`, `crates/rc_qt/src/session_view.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/auth.rs` |
| AUTH-006 | 2FA (TOTP / email code) | todo |
| AUTH-007 | Personal access token login | todo |
| AUTH-008 | OAuth/SSO login | todo |
| AUTH-009 | Multiple server accounts | todo |

## Rooms

| ID | Feature | Tests |
|---|---|---|
| ROOM-001 | List subscribed rooms after login | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_core/src/client/tests.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-002 | Room shows name and type (channel, private, DM) | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-003 | Room shows unread and mention counts | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/rooms.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/rooms.rs` |
| ROOM-004 | Opening a room marks it read | todo |
| ROOM-005 | Rooms ordered by last activity | todo |

## Messages

| ID | Feature | Tests |
|---|---|---|
| MSG-001 | Opening a room shows its latest messages | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/adapters/message_store.rs`, `crates/rc_core/src/client/tests.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/message_view.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-002 | Scrolling to the top loads older messages | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/client/tests.rs`, `crates/rc_core/src/timeline.rs`, `crates/rc_qt/src/cache.rs`, `crates/rc_qt/src/room_view.rs`, `crates/rc_qt/src/row_ops.rs`; api: `crates/rc_api/tests/rest.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-003 | Message shows author and timestamp | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_qt/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`, `crates/rc_core/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-004 | Day separators between days | unit: `crates/rc_qt/src/message_view.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |
| MSG-005 | System messages rendered distinctly | unit: `crates/rc_api/src/wire.rs`, `crates/rc_core/src/domain.rs`, `crates/rc_qt/src/message_view.rs`; contract: `crates/rc_api/tests/contract.rs`; GUI: `crates/rc_gui_tests/tests/messages.rs` |

## Realtime

| ID | Feature | Tests |
|---|---|---|
| RT-001 | New message in the open room appears live | todo |
| RT-002 | Unread counts of other rooms update live | todo |
| RT-003 | Reconnect after connection loss, catch up on missed messages | todo |
| RT-004 | Connection status indicator | todo |

## Sending

| ID | Feature | Tests |
|---|---|---|
| SEND-001 | Send a text message | todo |
| SEND-002 | Pending/failed state with retry | todo |

## Rich content

| ID | Feature | Tests |
|---|---|---|
| RICH-001 | Markdown formatting (bold, italic, code, quote, lists, links) | todo |
| RICH-002 | Mentions, @me highlighted | todo |
| RICH-003 | Unicode and custom emoji | todo |
| RICH-004 | Avatars (cached) | todo |
| RICH-005 | Image attachments inline, other files as links | todo |
| RICH-006 | Live message edits and deletes | todo |
| RICH-007 | Reactions displayed | todo |

## Interaction

| ID | Feature | Tests |
|---|---|---|
| ACT-001 | Edit own message | todo |
| ACT-002 | Delete own message | todo |
| ACT-003 | Add and remove reactions | todo |
| ACT-004 | Threads: view and reply | todo |
| ACT-005 | Reply/quote a message | todo |
| ACT-006 | Typing indicators | todo |
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
| INFRA-002 | GUI test harness reports an app that can't start, exits early or won't quit; passes environment overrides; waits for the UI with a timeout; addresses room rows and message rows by id | unit: `crates/rc_gui_tests/src/app.rs`, `crates/rc_gui_tests/src/gui.rs`, `crates/rc_gui_tests/src/paths.rs` |
| INFRA-003 | Developer builds' `--dev` fills in the login page for the local test server, with environment overrides; release builds have no such option | unit: `crates/rc_qt/src/dev_login.rs` |
