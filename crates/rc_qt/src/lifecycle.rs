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
    }
}

fn start_core(cache_dir: &str) {
    crate::hub::start(cache_dir);
}

fn stop_core() {
    crate::hub::stop();
}

fn enable_dev_login() {
    #[cfg(feature = "developer-mode")]
    crate::dev_login::enable();
    #[cfg(not(feature = "developer-mode"))]
    tracing::warn!("--dev needs a developer-mode build: ignored");
}
