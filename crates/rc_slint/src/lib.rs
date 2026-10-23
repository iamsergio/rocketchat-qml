// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The Slint frontend: a pure Rust app on `rc_core`, with no Qt. The `rocketchat_slint` binary
//! (`main.rs`) runs it; the window is built here so that the tests build the same one, without a
//! core.

pub mod app;
pub mod rich_markdown;
mod room_model;
mod rows;

// The code Slint generates doesn't follow the workspace's lints (it unwraps).
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable
)]
mod ui {
    slint::include_modules!();
}

pub use ui::MainWindow;

/// The main window, with what Rust provides without a core set.
///
/// # Errors
///
/// If there is no platform backend to create the window with.
pub fn new_main_window() -> Result<MainWindow, slint::PlatformError> {
    let window = MainWindow::new()?;
    window.set_version(rc_core::VERSION.into());
    Ok(window)
}

#[cfg(test)]
mod tests {
    use i_slint_backend_testing::ElementQuery;

    use super::new_main_window;

    // APP-001
    #[test]
    fn shows_the_version() {
        i_slint_backend_testing::init_no_event_loop();
        let window = new_main_window().unwrap();

        let label = ElementQuery::from_root(&window)
            .match_predicate(|element| element.accessible_id().as_deref() == Some("versionLabel"))
            .find_first()
            .expect("an element with the accessible-id versionLabel");
        let text = label.accessible_label().unwrap();
        assert!(text.contains(rc_core::VERSION), "the label says {text:?}");
    }
}
