// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Its own test binary: logging is set up once per process.

use std::{env, fs, process};

use rc_core::logging;

// APP-002
#[test]
fn logs_to_the_file_and_initializes_once() {
    let path = env::temp_dir().join(format!("rc_core-logging-test-{}.log", process::id()));
    let _ = fs::remove_file(&path);

    logging::init(Some(&path)).unwrap();
    // error! passes any RUST_LOG filter but "off".
    tracing::error!("written to the log file");
    let contents = fs::read_to_string(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(
        contents.contains("written to the log file"),
        "log file has {contents:?}"
    );

    assert!(logging::init(None).is_err());
}
