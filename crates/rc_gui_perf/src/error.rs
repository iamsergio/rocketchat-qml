// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::io;
use std::path::PathBuf;

use rc_gui_tests::{GuiError, LaunchError, QuitError};

use crate::cli::CliError;
use crate::perf::PerfError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Cli(#[from] CliError),
    #[error(transparent)]
    Perf(#[from] PerfError),
    #[error(transparent)]
    Flamegraph(#[from] crate::flamegraph::FlamegraphError),
    #[error("can't start the app")]
    Launch(#[from] LaunchError),
    #[error("can't stop the app")]
    Quit(#[from] QuitError),
    #[error("the app doesn't do what the scenario expects")]
    Gui(#[from] GuiError),
    #[error("can't make the test content")]
    Image(#[from] png::EncodingError),
    #[error("{} is not a file: build the app with Spix (see docs/design/performance-tests.md)", .0.display())]
    NoSuchBinary(PathBuf),
    #[error("{path} is not a run: {what}")]
    BadRun { path: String, what: String },
    #[error("{} has no run files (*.json)", .0.display())]
    NoRuns(PathBuf),
    #[error("{0}")]
    Scenario(String),
    #[error("{what}: {path}")]
    Io {
        what: &'static str,
        path: String,
        #[source]
        source: io::Error,
    },
}

impl Error {
    pub fn io(what: &'static str, path: &std::path::Path, source: io::Error) -> Self {
        Self::Io {
            what,
            path: path.display().to_string(),
            source,
        }
    }
}
