// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The app on the GUI thread: the core, the switchboard that decides what the screens show, and the
//! Slint models they are shown in.
//!
//! Everything lives on the GUI thread, in one [`App`] in a thread local, so there are no locks. The
//! core's event callback runs on a runtime thread and only hops to the GUI thread with
//! `slint::invoke_from_event_loop`, which keeps the order of the events. The window's callbacks
//! (the commands) run on the GUI thread already. What to show for an event, or for selecting,
//! adding or removing an account, is the switchboard's call (`rc_view::switchboard`, as for the Qt
//! frontend's hub); [`App::carry_out`] does it on the Slint models, applying each change to the
//! rows incrementally.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rc_core::adapters::DiskFileCache;
use rc_core::domain::{AccountId, MessageId, RoomId};
use rc_core::{AccountHandle, Client, Event};
use rc_view::accounts::AccountChange;
use rc_view::dev_login;
use rc_view::rich_html;
use rc_view::room_view::outgoing;
use rc_view::session_view::{LoginMode, SessionView, Status};
use rc_view::startup;
use rc_view::switchboard::{Shown, Switchboard};
use secrecy::SecretString;
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

use crate::message_model::MessageModel;
use crate::rich_markdown::{self, Palette};
use crate::room_model::RoomModel;
use crate::rows::tile;
use crate::ui::{self, AccountTile, MainWindow, Theme};

thread_local! {
    // The one app of the GUI thread: set by `start`, taken by `stop`.
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

struct App {
    client: Client,
    /// The accounts, and what to show when they change.
    switchboard: Switchboard,
    window: Weak<MainWindow>,
    /// The tiles of the rail, one per account.
    tiles: Rc<VecModel<AccountTile>>,
    /// The room list of the current account, in the core's order, favorites first, with the
    /// headings of its sections.
    rooms: Rc<RoomModel>,
    /// The messages of the open room, oldest first.
    messages: Rc<MessageModel>,
}

/// The directory for downloaded files: `$XDG_CACHE_HOME/rocketchat_slint`, else
/// `$HOME/.cache/rocketchat_slint`; empty if neither is set.
fn cache_dir() -> String {
    let non_empty = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    non_empty("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| non_empty("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map_or_else(String::new, |dir| {
            dir.join("rocketchat_slint").to_string_lossy().into_owned()
        })
}

/// Starts the core, and connects it and the window's commands to the app. Call on the GUI thread
/// with the window made, so that the core's events have an event loop to hop to, and before
/// running it.
pub fn start(window: &MainWindow) {
    let client = Client::start(
        startup::secret_store(),
        DiskFileCache::new(startup::cache_dir(&cache_dir())),
        |account, event| {
            if let Err(error) = slint::invoke_from_event_loop(move || {
                App::with(|app| app.dispatch(&account, event));
            }) {
                tracing::debug!(%error, "event dropped: the event loop is gone");
            }
        },
    );
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "can't start the core");
            return;
        }
    };

    let tiles: Rc<VecModel<AccountTile>> = Rc::new(VecModel::default());
    let rooms = Rc::new(RoomModel::default());
    let messages = Rc::new(MessageModel::new(Palette::from_theme(
        &window.global::<Theme>(),
    )));
    window.set_tiles(ModelRc::from(Rc::clone(&tiles)));
    window.set_rooms(ModelRc::from(Rc::clone(&rooms)));
    window.set_messages(ModelRc::from(Rc::clone(&messages)));
    window.set_prefill_password(dev_login::prefill().password.into());
    connect_commands(window);

    APP.with(|app| {
        *app.borrow_mut() = Some(App {
            client,
            switchboard: Switchboard::new(),
            window: window.as_weak(),
            tiles,
            rooms,
            messages,
        });
    });
}

/// Stops the core, once the event loop has ended.
pub fn stop() {
    // Taken out of the cell before it is dropped: stopping the core waits for a pending logout.
    let app = APP.with(|app| app.borrow_mut().take());
    drop(app);
}

/// Wires the window's callbacks to the commands.
fn connect_commands(window: &MainWindow) {
    window.on_login(|server, username, password| {
        // The password is copied out of the GUI's string, which can't be wiped; the Rust side
        // wraps its copy right away.
        let password = SecretString::from(password.to_string());
        App::with(|app| app.login(&server, &username, password));
    });
    window.on_logout(|| App::with(|app| app.logout()));
    window.on_select_account(|id| App::with(|app| app.select_account(&id)));
    window.on_add_account(|| App::with(App::add_account));
    window.on_remove_account(|id| App::with(|app| app.remove_account(&id)));
    window.on_open_room(|id| App::with(|app| app.open_room(&id)));
    window.on_send(|text| {
        let mut sent = false;
        App::with(|app| sent = app.send(&text));
        sent
    });
    window.on_retry(|id| App::with(|app| app.retry(&id)));
    window.on_set_favorite(|id, favorite| App::with(|app| app.set_favorite(&id, favorite)));
    window.on_mark_read(|id| App::with(|app| app.mark_read(&id)));
    window.on_mark_unread(|id| App::with(|app| app.mark_unread(&id)));
    window.on_leave_room(|id| App::with(|app| app.leave_room(&id)));
    window.on_dismiss_room_action_error(|| App::with(|app| app.show_room_action_error("")));
    window.on_load_older(|| App::with(|app| app.load_older()));
    window.on_open_link(|url| open_link(&url));
    let weak = window.as_weak();
    window.on_dump_info(move || {
        if let Some(window) = weak.upgrade() {
            dump_info(&window);
        }
    });
}

/// Opens the address in the default browser. The renderer only makes links of the web and mail.
fn open_link(url: &str) {
    if let Err(error) = open::that_detached(url) {
        tracing::warn!(%error, url, "can't open the link");
    }
}

/// Logs what Slint knows about the display: the window's size in both units and where it is, and
/// the scale between them.
fn dump_info(window: &MainWindow) {
    let window = window.window();
    let scale_factor = window.scale_factor();
    let physical = window.size();
    let logical = physical.to_logical(scale_factor);
    let position = window.position();
    tracing::info!(
        scale_factor,
        physical_width = physical.width,
        physical_height = physical.height,
        logical_width = logical.width,
        logical_height = logical.height,
        x = position.x,
        y = position.y,
        fullscreen = window.is_fullscreen(),
        maximized = window.is_maximized(),
        minimized = window.is_minimized(),
        "display info"
    );
}

/// What a login page starts with: `value`, or `prefill` (the developer-mode default) if empty.
fn or_prefill(value: &str, prefill: &str) -> String {
    if value.is_empty() { prefill } else { value }.to_owned()
}

impl App {
    /// Runs `command` on the app, if there is one and it isn't running a command already (neither
    /// can happen: Slint calls back one thing at a time, and the models never call back).
    fn with(command: impl FnOnce(&mut Self)) {
        APP.with(|app| {
            if let Ok(mut app) = app.try_borrow_mut() {
                if let Some(app) = app.as_mut() {
                    command(app);
                }
            } else {
                tracing::error!("the app is busy: command dropped");
            }
        });
    }

    // The core's events

    /// Takes in an event of the core, on the GUI thread, and shows what it changes.
    fn dispatch(&mut self, account: &AccountId, event: Event) {
        let shown = self.switchboard.dispatch(account, event);
        self.carry_out(shown);
    }

    /// Does what the switchboard says to show, in order.
    fn carry_out(&mut self, shown: Vec<Shown>) {
        for one in shown {
            match one {
                Shown::AccountRow(change) => self.show_account_row(&change),
                Shown::SessionView(view) => self.show_session(&view),
                Shown::All => self.show_all(),
                Shown::Event(event) => self.show(event),
            }
        }
    }

    /// Applies the change to the tiles, and says whether one of them waits for its login.
    fn show_account_row(&self, change: &AccountChange) {
        match change {
            AccountChange::Inserted { index, view } => self.tiles.insert(*index, tile(view)),
            AccountChange::Updated { index, view } => self.tiles.set_row_data(*index, tile(view)),
            AccountChange::Removed { index } => {
                self.tiles.remove(*index);
            }
        }
        if let Some(window) = self.window.upgrade() {
            window.set_has_new_account(self.tiles.iter().any(|tile| tile.fresh));
        }
    }

    /// Shows an event of the current account, which its cache has taken in. What the GUI doesn't
    /// show yet (threads, emoji, answers to message actions) is ignored.
    fn show(&self, event: Event) {
        match event {
            Event::Session(_) | Event::Connection(_) => {
                self.messages
                    .set_viewer(self.switchboard.accounts().current_cache().viewer());
                self.show_session(&self.switchboard.accounts().session_view());
            }
            Event::Identity(_) => {
                self.show_session(&self.switchboard.accounts().session_view());
            }
            Event::MessagePolicy(_) => self
                .messages
                .set_viewer(self.switchboard.accounts().current_cache().viewer()),
            Event::Rooms(changes) => {
                self.rooms.apply(&changes);
                // The open room's name may have changed.
                self.show_room();
            }
            Event::Timeline(changes) => {
                self.messages.apply(&changes);
                self.show_counters();
            }
            Event::TimelineStatus(_) => self.show_room(),
            Event::RoomAction { outcome, .. } => {
                // A failure is shown until dismissed or the next command; a success clears it.
                let text = outcome
                    .err()
                    .map_or_else(String::new, |error| error.to_string());
                self.show_room_action_error(&text);
            }
            Event::Avatars(updates) => {
                let cache = self.switchboard.accounts().current_cache();
                let changes = cache.avatar_changes(&updates);
                self.rooms.set_avatars(&changes);
                self.messages.set_avatars(&changes);
            }
            Event::Images(updates) => {
                let cache = self.switchboard.accounts().current_cache();
                self.messages.set_images(&cache.image_changes(&updates));
            }
            _ => {}
        }
    }

    /// Replaces everything with what the current account says: its rooms and their avatars, the
    /// open room's messages, the session and the open room.
    fn show_all(&self) {
        let cache = self.switchboard.accounts().current_cache();
        let mut rooms = Vec::new();
        cache.rooms_snapshot().apply(&mut rooms);
        self.rooms.show(rooms, cache.avatars());
        self.messages.show(
            &cache.timeline_snapshot(),
            cache.viewer(),
            cache.avatars(),
            cache.images(),
        );
        self.show_counters();
        self.show_session(&self.switchboard.accounts().session_view());
        self.show_room();
    }

    // The messages

    /// Tells the list how many times messages went in at its top and how many times it was
    /// replaced, which is how it knows what a change of its height means.
    fn show_counters(&self) {
        if let Some(window) = self.window.upgrade() {
            window.set_prepends(self.messages.prepends());
            window.set_resets(self.messages.resets());
        }
    }

    // The session

    /// Shows the session of the current account, on the login page and in the sidebar.
    fn show_session(&self, view: &SessionView) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let prefill = dev_login::prefill();
        window.set_status(match view.status {
            Status::Unknown => ui::SessionStatus::Unknown,
            Status::LoggedOut => ui::SessionStatus::LoggedOut,
            Status::Connecting => ui::SessionStatus::Connecting,
            Status::Resuming => ui::SessionStatus::Resuming,
            Status::LoggedIn => ui::SessionStatus::LoggedIn,
        });
        window.set_busy(view.busy);
        window.set_error_text(view.error_text.as_str().into());
        window.set_username(view.username.as_str().into());
        window.set_host(view.host.as_str().into());
        window.set_connection_text(view.connection_text.as_str().into());
        window.set_login_mode(match view.login_mode {
            LoginMode::Login => ui::LoginMode::Login,
            LoginMode::Adding => ui::LoginMode::Adding,
            LoginMode::Again => ui::LoginMode::Again,
        });
        window.set_login_title(view.login_title.as_str().into());
        window.set_login_lede(view.login_lede.as_str().into());
        window.set_login_notice(view.login_notice.as_str().into());
        window.set_login_server(or_prefill(&view.login_server, &prefill.server).into());
        window.set_login_username(or_prefill(&view.login_username, &prefill.username).into());
        // Last: the login fields start again when the account changes, from the values above.
        window.set_account_id(view.account_id.as_str().into());
    }

    /// Shows the open room's header, as the current account's cache says.
    fn show_room(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let state = self.switchboard.accounts().current_cache().room_state();
        window.set_room_id(state.room_id.into());
        window.set_room_name(state.name.into());
        window.set_room_kind(state.kind.into());
        // The topic is markdown. The GUI tests read it as the HTML the Qt app shows.
        let viewer = self.switchboard.accounts().current_cache().viewer();
        let palette = Palette::from_theme(&window.global::<Theme>());
        window.set_room_topic(rich_markdown::topic(&state.topic, &viewer, &palette));
        window.set_room_topic_html(rich_html::topic_html(&state.topic, &viewer).into());
        window.set_reached_start(state.reached_start);
        window.set_loading_older(state.loading_older);
    }

    // The commands

    /// Runs `command` for the current account, or logs that it was ignored: `what` names it.
    fn current_account(&self, what: &str, command: impl FnOnce(AccountHandle<'_>)) {
        if let Some(id) = self.switchboard.accounts().current() {
            command(self.client.account(id));
        } else {
            tracing::warn!("{what} ignored: there is no current account");
        }
    }

    fn login(&self, server: &str, username: &str, password: SecretString) {
        self.current_account("login", |account| account.login(server, username, password));
    }

    /// Logs out and forgets the current account. The next account becomes current, or a new
    /// logged-out one takes its place if it was the last.
    fn logout(&self) {
        if let Some(id) = self.switchboard.accounts().current() {
            self.client.remove_account(id);
        } else {
            tracing::warn!("logout ignored: there is no current account");
        }
    }

    /// Makes the account current: the views show it from now on.
    fn select_account(&mut self, id: &str) {
        let shown = self.switchboard.select(&AccountId::from(id.to_owned()));
        self.carry_out(shown);
    }

    /// Adds an account, logged out, and makes it current, so that the login page shows for it. The
    /// account is listed at once, which makes its `Added` a no-op.
    fn add_account(&mut self) {
        let id = self.client.add_account();
        let shown = self.switchboard.add(&id);
        self.carry_out(shown);
    }

    /// Asks the core to forget the account, logging it out first if it is logged in. It leaves the
    /// rail, and the screen if it was on it, when the core says `Removed`.
    fn remove_account(&self, id: &str) {
        self.client.remove_account(&AccountId::from(id.to_owned()));
    }

    /// Asks for the messages before the first one loaded. The core ignores it when there are none
    /// or a load is going on already.
    fn load_older(&self) {
        self.current_account("load_older", |account| account.load_older());
    }

    /// Sends `text` to the open room. False if there is nothing to send (no room is open or the
    /// text is blank), so that the composer keeps its text.
    fn send(&self, text: &str) -> bool {
        let room_id = self
            .switchboard
            .accounts()
            .current_cache()
            .room_state()
            .room_id;
        let Some((room, text, quote)) = outgoing(&room_id, text, "") else {
            return false;
        };
        self.current_account("send_message", |account| {
            account.send_message(room, &text, quote, None, false);
        });
        true
    }

    /// Sends a message that failed to send again.
    fn retry(&self, id: &str) {
        self.current_account("retry_message", |account| {
            account.retry_message(MessageId::from(id.to_owned()));
        });
    }

    /// Shows why the last command on a room failed, or nothing for the empty text.
    fn show_room_action_error(&self, text: &str) {
        if let Some(window) = self.window.upgrade() {
            window.set_room_action_error(text.into());
        }
    }

    /// Marks the room as a favorite, or takes it back.
    fn set_favorite(&self, id: &str, favorite: bool) {
        self.show_room_action_error("");
        self.current_account("set_favorite", |account| {
            account.set_favorite(RoomId::from(id.to_owned()), favorite);
        });
    }

    fn mark_read(&self, id: &str) {
        self.show_room_action_error("");
        self.current_account("mark_read", |account| {
            account.mark_read(RoomId::from(id.to_owned()));
        });
    }

    fn mark_unread(&self, id: &str) {
        self.show_room_action_error("");
        self.current_account("mark_unread", |account| {
            account.mark_unread(RoomId::from(id.to_owned()));
        });
    }

    /// Leaves a channel or a private group. The row leaves the list when the server agrees.
    fn leave_room(&self, id: &str) {
        self.show_room_action_error("");
        self.current_account("leave_room", |account| {
            account.leave_room(RoomId::from(id.to_owned()));
        });
    }

    fn open_room(&self, id: &str) {
        self.current_account("open_room", |account| {
            account.open_room(RoomId::from(id.to_owned()));
        });
    }
}
