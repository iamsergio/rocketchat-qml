// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Process-wide logging through `tracing`, set up once by the frontend at startup.

use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::util::TryInitError;

#[derive(Debug, thiserror::Error)]
#[error("logging is already initialized")]
pub struct AlreadyInitialized(#[from] TryInitError);

/// Logs to stderr and, if `log_file` is given, appends to that file as well. `RUST_LOG` sets the
/// filter, `info` by default. A log file that can't be opened is reported in the log, not as an
/// error: logging to stderr still works.
///
/// # Errors
///
/// If logging is already initialized in this process.
pub fn init(log_file: Option<&Path>) -> Result<(), AlreadyInitialized> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    let stderr = fmt::layer()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal());
    let (file, file_error) = match log_file.map(open_append).transpose() {
        Ok(file) => (file, None),
        Err(error) => (None, Some(error)),
    };
    let file = file.map(|file| fmt::layer().with_writer(Mutex::new(file)).with_ansi(false));

    tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file)
        .try_init()?;

    if let Some(path) = log_file {
        match file_error {
            None => tracing::info!(path = %path.display(), "logging to a file too"),
            Some(error) => {
                tracing::warn!(path = %path.display(), %error, "can't open the log file, logging to stderr only");
            }
        }
    }
    Ok(())
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}
