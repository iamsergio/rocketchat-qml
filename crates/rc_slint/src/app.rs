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
use rc_core::domain::{AccountId, RoomId};
use rc_core::{AccountHandle, Client, Event};
use rc_view::accounts::AccountChange;
use rc_view::dev_login;
use rc_view::session_view::{LoginMode, SessionView, Status};
use rc_view::startup;
use rc_view::switchboard::{Shown, Switchboard};
use secrecy::SecretString;
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

use crate::room_model::RoomModel;
use crate::rows::tile;
use crate::ui::{self, AccountTile, MainWindow};

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
    window.set_tiles(ModelRc::from(Rc::clone(&tiles)));
    window.set_rooms(ModelRc::from(Rc::clone(&rooms)));
    window.set_prefill_password(dev_login::prefill().password.into());
    connect_commands(window);

    APP.with(|app| {
        *app.borrow_mut() = Some(App {
            client,
            switchboard: Switchboard::new(),
            window: window.as_weak(),
            tiles,
            rooms,
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
    /// show yet (messages, threads, emoji, images, answers to actions) is ignored.
    fn show(&self, event: Event) {
        match event {
            Event::Session(_) | Event::Connection(_) | Event::Identity(_) => {
                self.show_session(&self.switchboard.accounts().session_view());
            }
            Event::Rooms(changes) => {
                self.rooms.apply(&changes);
                // The open room's name may have changed.
                self.show_room();
            }
            Event::TimelineStatus(_) => self.show_room(),
            Event::Avatars(updates) => {
                let cache = self.switchboard.accounts().current_cache();
                self.rooms.set_avatars(&cache.avatar_changes(&updates));
            }
            _ => {}
        }
    }

    /// Replaces everything with what the current account says: its rooms and their avatars, the
    /// session and the open room.
    fn show_all(&self) {
        let cache = self.switchboard.accounts().current_cache();
        let mut rooms = Vec::new();
        cache.rooms_snapshot().apply(&mut rooms);
        self.rooms.show(rooms, cache.avatars());
        self.show_session(&self.switchboard.accounts().session_view());
        self.show_room();
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
        // The topic is markdown: shown as written, for now.
        window.set_room_topic(state.topic.into());
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

    fn open_room(&self, id: &str) {
        self.current_account("open_room", |account| {
            account.open_room(RoomId::from(id.to_owned()));
        });
    }
}
