// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Starting and stopping the core, called by the C++ host.

#[cxx_qt::bridge(namespace = "rc_qt")]
pub mod ffi {
    extern "Rust" {
        /// Starts the core. Call on the GUI thread after `QGuiApplication` exists and before the
        /// QML is loaded. `RC_SECRET_STORE` picks where the session is kept: the OS keyring by
        /// default, `memory`, or `file:<path>` (test builds only). Downloaded files (avatars) are
        /// kept in `cache_dir`, which is created when needed.
        fn start_core(cache_dir: &str);

        /// Stops the core, waiting a moment for a pending logout to finish. Call on the GUI
        /// thread after the event loop has ended.
        fn stop_core();

        /// `--dev`: the login page starts filled in for the local test server (`RC_DEV_SERVER`,
        /// `RC_DEV_USER` and `RC_DEV_PASS` override it). Only builds with `developer-mode` do it.
        /// Call before the QML is loaded.
        fn enable_dev_login();

        /// `--gh`: the GitHub integration, which is off by default: without it the GitHub service
        /// doesn't start and the rail has no GitHub tile. Call before `start_core`.
        fn enable_github();
    }
}

use std::sync::atomic::{AtomicBool, Ordering};

// Set once, before the core starts and the QML is loaded.
static GITHUB: AtomicBool = AtomicBool::new(false);

/// Whether `--gh` was given.
pub(crate) fn github_enabled() -> bool {
    GITHUB.load(Ordering::Relaxed)
}

fn start_core(cache_dir: &str) {
    crate::hub::start(cache_dir);
}

fn stop_core() {
    crate::hub::stop();
}

fn enable_dev_login() {
    #[cfg(feature = "developer-mode")]
    rc_view::dev_login::enable();
    #[cfg(not(feature = "developer-mode"))]
    tracing::warn!("--dev needs a developer-mode build: ignored");
}

fn enable_github() {
    tracing::info!("--gh: the GitHub integration is on");
    GITHUB.store(true, Ordering::Relaxed);
}
