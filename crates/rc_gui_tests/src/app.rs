// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Launches the app under test with its Spix server on a free port.

use std::ffi::OsStr;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use std::{env, io, thread};

use crate::gui::{Gui, GuiError};

/// Environment variable naming the app binary to test. It must accept `--spix-port <port>`.
pub const APP_ENV: &str = "RC_GUI_TEST_APP";

/// How long the app may take to start answering Spix calls.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("{APP_ENV} is not set: point it at the app binary (ctest does this)")]
    NoAppBinary,
    #[error("no free port for Spix")]
    NoPort(#[source] io::Error),
    #[error("can't start {}", .path.display())]
    Spawn {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("can't check whether the app is still running")]
    Wait(#[source] io::Error),
    #[error("the app exited before Spix answered: {0}")]
    Exited(ExitStatus),
    #[error("Spix didn't answer within {STARTUP_TIMEOUT:?}")]
    Timeout(#[source] GuiError),
}

/// The running app, killed on drop.
pub struct App {
    child: Child,
    gui: Gui,
}

impl App {
    /// Starts the binary named by [`APP_ENV`] with `QT_QPA_PLATFORM=offscreen` and waits until its
    /// Spix server answers.
    ///
    /// # Errors
    ///
    /// If [`APP_ENV`] is unset, the app can't be started, or it exits or doesn't answer in time.
    pub fn launch() -> Result<Self, LaunchError> {
        let path = env::var_os(APP_ENV).ok_or(LaunchError::NoAppBinary)?;
        Self::launch_binary(&path)
    }

    fn launch_binary(path: &OsStr) -> Result<Self, LaunchError> {
        let port = free_port().map_err(LaunchError::NoPort)?;
        let child = Command::new(path)
            .arg("--spix-port")
            .arg(port.to_string())
            .env("QT_QPA_PLATFORM", "offscreen")
            .stdin(Stdio::null())
            .spawn()
            .map_err(|source| LaunchError::Spawn {
                path: path.into(),
                source,
            })?;
        // Owned from here on, so every error below kills the app.
        let mut app = Self {
            child,
            gui: Gui::new(port),
        };
        app.wait_for_spix()?;
        Ok(app)
    }

    pub fn gui(&self) -> &Gui {
        &self.gui
    }

    fn wait_for_spix(&mut self) -> Result<(), LaunchError> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().map_err(LaunchError::Wait)? {
                return Err(LaunchError::Exited(status));
            }
            match self.gui.ping() {
                Ok(()) => return Ok(()),
                Err(error) if Instant::now() >= deadline => {
                    return Err(LaunchError::Timeout(error));
                }
                Err(_) => thread::sleep(POLL_INTERVAL),
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Errors mean it has already exited.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A port nothing listens on right now. The app binds it shortly after, so another process could
/// take it in between; with the kernel handing out ephemeral ports in turn, that's unlikely.
fn free_port() -> io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-002
    #[test]
    fn reports_a_missing_binary() {
        let result = App::launch_binary(OsStr::new("./no-such-app"));
        assert!(
            matches!(result, Err(LaunchError::Spawn { .. })),
            "{:?}",
            result.err()
        );
    }

    // INFRA-002
    #[test]
    fn reports_an_app_that_exits_before_spix_answers() {
        // This test binary: it rejects --spix-port and exits without opening the port.
        let this = env::current_exe().unwrap();
        let result = App::launch_binary(this.as_os_str());
        assert!(
            matches!(result, Err(LaunchError::Exited(_))),
            "{:?}",
            result.err()
        );
    }
}
