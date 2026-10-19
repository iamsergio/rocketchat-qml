// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Logging setup, called by the C++ host.

#[cxx_qt::bridge(namespace = "rc_qt")]
pub mod ffi {
    extern "Rust" {
        /// Sets up logging (see `rc_core::logging`). Call once, first thing in `main()`.
        fn init_logging();
    }
}

use std::path::PathBuf;

fn init_logging() {
    let log_file = cfg!(feature = "developer-mode")
        .then(developer_log_file)
        .flatten();
    if let Err(error) = rc_core::logging::init(log_file.as_deref()) {
        tracing::warn!(%error);
    }
    tracing::info!(version = rc_core::VERSION, "starting");
}

/// `<executable>.log` next to the executable, so each build has its own.
fn developer_log_file() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_extension("log"))
}
