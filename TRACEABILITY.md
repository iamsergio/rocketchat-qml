# Traceability

Every test carries a `// <ID>` comment. A feature can have tests at several levels (unit, core integration against `rc-fake-server`, GUI through Spix).

## App

| ID | Feature | Tests |
|---|---|---|
| APP-001 | App starts and shows the version from Rust | todo |

## Authentication

| ID | Feature | Tests |
|---|---|---|
| AUTH-001 | Log in with server URL, username and password | todo |
| AUTH-002 | Invalid credentials show an error | todo |
| AUTH-003 | Unreachable or non-Rocket.Chat server shows an error | todo |
| AUTH-004 | Session resumes at startup from the stored token | todo |
| AUTH-005 | Logout clears the stored token and returns to login | todo |
| AUTH-006 | 2FA (TOTP / email code) | todo |
| AUTH-007 | Personal access token login | todo |
| AUTH-008 | OAuth/SSO login | todo |
| AUTH-009 | Multiple server accounts | todo |

## Rooms

| ID | Feature | Tests |
|---|---|---|
| ROOM-001 | List subscribed rooms after login | todo |
| ROOM-002 | Room shows name and type (channel, private, DM) | todo |
| ROOM-003 | Room shows unread and mention counts | todo |
| ROOM-004 | Opening a room marks it read | todo |
| ROOM-005 | Rooms ordered by last activity | todo |

## Messages

| ID | Feature | Tests |
|---|---|---|
| MSG-001 | Opening a room shows its latest messages | todo |
| MSG-002 | Scrolling to the top loads older messages | todo |
| MSG-003 | Message shows author and timestamp | todo |
| MSG-004 | Day separators between days | todo |
| MSG-005 | System messages rendered distinctly | todo |

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
