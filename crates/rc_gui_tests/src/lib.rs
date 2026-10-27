// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! GUI tests that drive the running app through Spix's XML-RPC interface. Frontend-agnostic: no Qt,
//! no link to the app, only element paths and the Spix protocol.
//!
//! The tests are `#[ignore]`d because they need a built app: ctest runs them with `--ignored` and
//! [`APP_ENV`] set to the app's absolute path (cargo runs tests from the crate's directory).

#![forbid(unsafe_code)]

mod app;
mod gui;
pub mod paths;
#[cfg(test)]
mod slint_list;
mod xmlrpc;

pub use app::{APP_ENV, App, LaunchError, LaunchOptions, QuitError};
pub use gui::{Gui, GuiError, Key, Modifiers, MouseButton, Rect};
