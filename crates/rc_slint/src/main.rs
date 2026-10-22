// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rocketchat_slint`: the Slint frontend.

use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    rc_core::logging::init_app(cfg!(feature = "developer-mode"));
    rc_slint::new_main_window()?.run()
}
