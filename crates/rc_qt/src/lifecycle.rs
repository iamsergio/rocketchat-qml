// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Starting and stopping the core, called by the C++ host.

#[cxx_qt::bridge(namespace = "rc_qt")]
pub mod ffi {
    extern "Rust" {
        /// Starts the core. Call on the GUI thread after `QGuiApplication` exists and before the
        /// QML is loaded. `RC_SECRET_STORE` picks where the session is kept: the OS keyring by
        /// default, `memory`, or `file:<path>` (test builds only).
        fn start_core();

        /// Stops the core, waiting a moment for a pending logout to finish. Call on the GUI
        /// thread after the event loop has ended.
        fn stop_core();
    }
}

fn start_core() {
    crate::hub::start();
}

fn stop_core() {
    crate::hub::stop();
}
