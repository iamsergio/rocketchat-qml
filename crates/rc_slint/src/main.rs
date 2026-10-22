// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rocketchat_slint`: the Slint frontend.

use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    rc_core::logging::init_app(cfg!(feature = "developer-mode"));
    if std::env::args().any(|arg| arg == "--dev") {
        enable_dev_login();
    }
    // The window first: the core's events hop to the event loop, which the backend provides.
    let window = rc_slint::new_main_window()?;
    rc_slint::app::start(&window);
    let result = window.run();
    // After the event loop: stopping the core waits for a pending logout.
    rc_slint::app::stop();
    result
}

/// `--dev`: the login page starts filled in for the local test server (`RC_DEV_SERVER`,
/// `RC_DEV_USER` and `RC_DEV_PASS` override it). Only builds with `developer-mode` do it.
fn enable_dev_login() {
    #[cfg(feature = "developer-mode")]
    rc_view::dev_login::enable();
    #[cfg(not(feature = "developer-mode"))]
    tracing::warn!("--dev needs a developer-mode build: ignored");
}
