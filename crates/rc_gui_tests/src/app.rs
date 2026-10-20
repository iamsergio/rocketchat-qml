// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Launches the app under test with its Spix server on a free port.

use std::ffi::OsStr;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use std::{env, io, thread};

use crate::gui::{Gui, GuiError};

/// Environment variable naming the app binary to test. It must accept `--spix-port <port>`.
pub const APP_ENV: &str = "RC_GUI_TEST_APP";

/// How long the app may take to start answering Spix calls.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the app may take to exit by itself after it was asked to quit: it first waits for a
/// pending logout or store write to finish.
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);
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

#[derive(Debug, thiserror::Error)]
pub enum QuitError {
    #[error("can't check whether the app is still running")]
    Wait(#[source] io::Error),
    /// The app was killed. If Spix refused the `quit` call, that is the likely reason.
    #[error("the app didn't exit within {timeout:?} of being asked to quit")]
    Timeout {
        timeout: Duration,
        #[source]
        quit: Option<GuiError>,
    },
}

/// The running app, killed on drop.
pub struct App {
    child: Child,
    gui: Gui,
    cache: CacheDir,
}

/// The directory the app keeps downloaded files in, empty at launch and removed with the app.
struct CacheDir(PathBuf);

impl CacheDir {
    fn new(port: u16) -> Self {
        let name = format!("rc_gui_tests-cache-{}-{port}", std::process::id());
        Self(env::temp_dir().join(name))
    }
}

impl Drop for CacheDir {
    fn drop(&mut self) {
        // Nothing to do if it was never made.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl App {
    /// Starts the binary named by [`APP_ENV`] with `QT_QPA_PLATFORM=offscreen`,
    /// `RC_SECRET_STORE=memory` and `XDG_CACHE_HOME` set to a directory of its own, and waits until
    /// its Spix server answers.
    ///
    /// # Errors
    ///
    /// If [`APP_ENV`] is unset, the app can't be started, or it exits or doesn't answer in time.
    pub fn launch() -> Result<Self, LaunchError> {
        Self::launch_with(&[])
    }

    /// Like [`App::launch`], with `env` added to the app's environment. It overrides the defaults,
    /// so a test can set `RC_SECRET_STORE=file:<path>` to keep the session between two runs.
    ///
    /// # Errors
    ///
    /// As [`App::launch`].
    pub fn launch_with(env: &[(&str, &str)]) -> Result<Self, LaunchError> {
        let path = env::var_os(APP_ENV).ok_or(LaunchError::NoAppBinary)?;
        Self::launch_binary(&path, env)
    }

    fn launch_binary(path: &OsStr, env: &[(&str, &str)]) -> Result<Self, LaunchError> {
        let port = free_port().map_err(LaunchError::NoPort)?;
        let cache = CacheDir::new(port);
        let child = app_command(path, port, &cache.0, env)
            .spawn()
            .map_err(|source| LaunchError::Spawn {
                path: path.into(),
                source,
            })?;
        // Owned from here on, so every error below kills the app.
        let mut app = Self {
            child,
            gui: Gui::new(port),
            cache,
        };
        app.wait_for_spix()?;
        Ok(app)
    }

    pub fn gui(&self) -> &Gui {
        &self.gui
    }

    /// Where the app keeps the files it downloads (its `XDG_CACHE_HOME`, on Linux).
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache.0
    }

    /// Asks the app to quit and waits for it to exit by itself, so that it stops its core first: a
    /// pending logout or store write finishes. Returns how it exited.
    ///
    /// # Errors
    ///
    /// [`QuitError::Timeout`] if it is still running after a while; it is then killed.
    pub fn quit_and_wait(self) -> Result<ExitStatus, QuitError> {
        self.quit_and_wait_for(QUIT_TIMEOUT)
    }

    fn quit_and_wait_for(mut self, timeout: Duration) -> Result<ExitStatus, QuitError> {
        // The app may go away before it answers, so a failed call only matters if it stays up.
        let quit = self.gui.quit();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().map_err(QuitError::Wait)? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(QuitError::Timeout {
                    timeout,
                    quit: quit.err(),
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
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

/// The command that starts the app: offscreen, with Spix on `port` and its cache in `cache`, and
/// `env` over the defaults.
fn app_command(path: &OsStr, port: u16, cache: &Path, env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(path);
    command
        .arg("--spix-port")
        .arg(port.to_string())
        .env("QT_QPA_PLATFORM", "offscreen")
        // The app starts the core at launch, and its default secret store is the OS keyring:
        // on a developer machine that is the user's real wallet, which may even ask them to
        // unlock it. Tests keep the session in memory unless they ask for a file.
        .env("RC_SECRET_STORE", "memory")
        // Where Qt puts the cache location on Linux: not the user's own.
        .env("XDG_CACHE_HOME", cache)
        .envs(env.iter().copied())
        .stdin(Stdio::null());
    command
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
        let result = App::launch_binary(OsStr::new("./no-such-app"), &[]);
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
        let result = App::launch_binary(this.as_os_str(), &[]);
        assert!(
            matches!(result, Err(LaunchError::Exited(_))),
            "{:?}",
            result.err()
        );
    }

    /// The value the command sets for `name`.
    fn env_of(command: &Command, name: &str) -> Option<String> {
        command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new(name))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }

    // INFRA-002
    #[test]
    fn keeps_the_session_in_memory_by_default() {
        let command = app_command(OsStr::new("app"), 1234, Path::new("/c"), &[]);
        assert_eq!(
            env_of(&command, "RC_SECRET_STORE").as_deref(),
            Some("memory")
        );
        assert_eq!(
            env_of(&command, "QT_QPA_PLATFORM").as_deref(),
            Some("offscreen")
        );
    }

    // INFRA-002
    #[test]
    fn extra_environment_overrides_the_defaults() {
        let command = app_command(
            OsStr::new("app"),
            1234,
            Path::new("/c"),
            &[("RC_SECRET_STORE", "file:/x"), ("EXTRA", "1")],
        );
        assert_eq!(
            env_of(&command, "RC_SECRET_STORE").as_deref(),
            Some("file:/x")
        );
        assert_eq!(env_of(&command, "EXTRA").as_deref(), Some("1"));
    }

    // RICH-004
    #[test]
    fn the_app_gets_a_cache_of_its_own() {
        let command = app_command(OsStr::new("app"), 1234, Path::new("/c/1234"), &[]);
        assert_eq!(
            env_of(&command, "XDG_CACHE_HOME").as_deref(),
            Some("/c/1234")
        );
        assert_ne!(CacheDir::new(1234).0, CacheDir::new(1235).0);
    }

    /// An `App` around any process; nothing listens on its Spix port.
    fn app_around(command: &mut Command) -> App {
        let port = free_port().unwrap();
        App {
            child: command.stdin(Stdio::null()).spawn().unwrap(),
            gui: Gui::new(port),
            cache: CacheDir::new(port),
        }
    }

    // INFRA-002
    #[cfg(unix)]
    #[test]
    fn quitting_returns_how_the_app_exited() {
        // Exits by itself with a status of its own; the quit call fails, as the app is gone.
        let app = app_around(Command::new("/bin/sh").args(["-c", "exit 3"]));
        let status = app.quit_and_wait_for(Duration::from_secs(10)).unwrap();
        assert_eq!(status.code(), Some(3));
    }

    // INFRA-002
    #[cfg(unix)]
    #[test]
    fn quitting_reports_an_app_that_stays_up() {
        let app = app_around(Command::new("/bin/sh").args(["-c", "sleep 60"]));
        let result = app.quit_and_wait_for(Duration::from_millis(200));
        // The failed quit call is reported with the timeout, as the likely reason.
        assert!(
            matches!(result, Err(QuitError::Timeout { quit: Some(_), .. })),
            "{result:?}"
        );
    }
}
