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

fn init_logging() {
    rc_core::logging::init_app(cfg!(feature = "developer-mode"));
}
