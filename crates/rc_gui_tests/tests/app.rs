// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! App-level GUI tests.

use rc_gui_tests::App;
use rc_gui_tests::paths::main_window;

/// The version the app shows, from the same file the app is built from.
const VERSION: &str = include_str!("../../../version.txt");

// APP-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn shows_the_version_from_rust() {
    let app = App::launch().unwrap();
    let text = app.gui().text(main_window::VERSION_LABEL).unwrap();
    assert!(
        text.contains(VERSION.trim_ascii()),
        "version label shows {text:?}"
    );
}
