// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Launches the app under test with its Spix server on a free port.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use std::{env, io, thread};

use crate::gui::{Gui, GuiError};

/// Environment variable naming the app binary to test. It must accept `--spix-port <port>`.
pub const APP_ENV: &str = "RC_GUI_TEST_APP";

/// Set when the app is built for coverage: where it writes its profile on exit.
const PROFILE_ENV: &str = "LLVM_PROFILE_FILE";

/// How long the app may take to start answering Spix calls.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the app may take to exit by itself after it was asked to quit: it first waits for a
/// pending logout or store write to finish.
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a dropped app may take to exit after being asked to quit, before it is killed. Short:
/// nobody waits for the outcome.
const DROP_QUIT_TIMEOUT: Duration = Duration::from_secs(5);
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

/// The running app, killed on drop. When the app writes a coverage profile (`LLVM_PROFILE_FILE` is
/// set), it is asked to quit first, as it only writes the profile when it exits by itself. That
/// takes about half a second per test binary, so it is not done in a plain run.
///
/// Under a wrapper (see [`LaunchOptions::wrapper`]) `child` is the wrapper, not the app. Killing it
/// would discard what it writes when the app exits (`perf stat` prints its counts then) and can
/// leave the app running on its own, so a wrapped app is always asked to quit first. There are no
/// process groups, as they would stop Ctrl-C in the terminal from reaching the app: a wrapper that
/// is killed after the quit timeout may therefore leave the app behind.
pub struct App {
    child: Child,
    /// Whether `child` is a wrapper around the app.
    wrapped: bool,
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

/// How to start the app: which binary, under which wrapper, with which extra arguments and
/// environment.
#[derive(Debug, Default, Clone)]
pub struct LaunchOptions {
    binary: Option<OsString>,
    wrapper: Vec<OsString>,
    args: Vec<OsString>,
    env: Vec<(String, String)>,
}

impl LaunchOptions {
    /// The defaults: the binary named by [`APP_ENV`], no wrapper, no extra arguments or
    /// environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts this binary instead of the one named by [`APP_ENV`], so that one process can launch
    /// several apps without touching its own environment.
    #[must_use]
    pub fn binary(mut self, binary: impl Into<OsString>) -> Self {
        self.binary = Some(binary.into());
        self
    }

    /// Starts the app as `program args... <binary> --spix-port <port>`, for example
    /// `perf stat ... --`. The environment is inherited through the wrapper. Spix and the quit
    /// call work as without it, since a wrapper exits when the app does.
    #[must_use]
    pub fn wrapper<I, S>(mut self, program: impl Into<OsString>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.wrapper = std::iter::once(program.into())
            .chain(args.into_iter().map(Into::into))
            .collect();
        self
    }

    /// Adds `arg` to the app's arguments, after `--spix-port <port>`, e.g. `--gh`.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Adds to the app's environment, over the defaults and earlier additions.
    #[must_use]
    pub fn env(mut self, env: &[(&str, &str)]) -> Self {
        self.env
            .extend(env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
        self
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
        Self::launch_with_options(&LaunchOptions::new())
    }

    /// Like [`App::launch`], with `env` added to the app's environment. It overrides the defaults,
    /// so a test can set `RC_SECRET_STORE=file:<path>` to keep the session between two runs.
    ///
    /// # Errors
    ///
    /// As [`App::launch`].
    pub fn launch_with(env: &[(&str, &str)]) -> Result<Self, LaunchError> {
        Self::launch_with_options(&LaunchOptions::new().env(env))
    }

    /// Like [`App::launch`], with the binary, wrapper and environment of `options`.
    ///
    /// # Errors
    ///
    /// As [`App::launch`]; the binary is only required when `options` doesn't name one.
    pub fn launch_with_options(options: &LaunchOptions) -> Result<Self, LaunchError> {
        let binary = match &options.binary {
            Some(binary) => binary.clone(),
            None => env::var_os(APP_ENV).ok_or(LaunchError::NoAppBinary)?,
        };
        let port = free_port().map_err(LaunchError::NoPort)?;
        let cache = CacheDir::new(port);
        let mut command = app_command(options, &binary, port, &cache.0);
        let child = command.spawn().map_err(|source| LaunchError::Spawn {
            path: PathBuf::from(command.get_program()),
            source,
        })?;
        // Owned from here on, so every error below kills the app.
        let mut app = Self {
            child,
            wrapped: !options.wrapper.is_empty(),
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
        self.ask_to_quit(timeout)
    }

    fn ask_to_quit(&mut self, timeout: Duration) -> Result<ExitStatus, QuitError> {
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
        // Not already gone, as after `quit_and_wait` or a crash. A wrapper must see the app exit.
        let must_exit_by_itself = self.wrapped || env::var_os(PROFILE_ENV).is_some();
        if must_exit_by_itself && !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.ask_to_quit(DROP_QUIT_TIMEOUT);
        }
        // Errors mean it has already exited.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The command that starts `binary` (behind the wrapper of `options`, if any): offscreen, with
/// Spix on `port` and its cache in `cache`, and the environment of `options` over the defaults.
fn app_command(options: &LaunchOptions, binary: &OsStr, port: u16, cache: &Path) -> Command {
    let mut command = match options.wrapper.split_first() {
        Some((program, args)) => {
            let mut command = Command::new(program);
            command.args(args).arg(binary);
            command
        }
        None => Command::new(binary),
    };
    command
        .arg("--spix-port")
        .arg(port.to_string())
        .args(&options.args)
        .env("QT_QPA_PLATFORM", "offscreen")
        // The app starts the core at launch, and its default secret store is the OS keyring:
        // on a developer machine that is the user's real wallet, which may even ask them to
        // unlock it. Tests keep the session in memory unless they ask for a file.
        .env("RC_SECRET_STORE", "memory")
        // Where Qt puts the cache location on Linux: not the user's own.
        .env("XDG_CACHE_HOME", cache)
        // Channels show their news at once, as most tests expect; the batching ones override it.
        .env("RC_CHANNEL_BATCH_SECS", "0")
        // A QML warning (a binding loop, a TypeError) ends the Qt app, so the test fails on it.
        .env("RC_FATAL_QML_WARNINGS", "1")
        .envs(options.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null());
    command
}

/// Ports `free_port` has handed out. They're never reused, so parallel tests can't share one,
/// nor talk to an earlier app that is still shutting down.
static USED_PORTS: Mutex<BTreeSet<u16>> = Mutex::new(BTreeSet::new());

/// A port nothing listens on right now and that this process hasn't returned before. The app
/// binds it shortly after, so another process could take it in between.
fn free_port() -> io::Result<u16> {
    for _ in 0..100 {
        let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        // A panic elsewhere can't leave the set inconsistent, so a poisoned lock is fine.
        let mut used = USED_PORTS.lock().unwrap_or_else(PoisonError::into_inner);
        if used.insert(port) {
            return Ok(port);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        "no unused port found",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-002
    #[test]
    fn reports_a_missing_binary() {
        let result = App::launch_with_options(&LaunchOptions::new().binary("./no-such-app"));
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
        let result = App::launch_with_options(&LaunchOptions::new().binary(this));
        assert!(
            matches!(result, Err(LaunchError::Exited(_))),
            "{:?}",
            result.err()
        );
    }

    // INFRA-002
    #[cfg(unix)]
    #[test]
    fn reports_a_wrapped_app_that_exits_before_spix_answers() {
        // `env` runs this test binary, which rejects --spix-port, and exits with its status.
        let this = env::current_exe().unwrap();
        let options = LaunchOptions::new()
            .binary(this)
            .wrapper("env", ["WRAPPED=1"]);
        let result = App::launch_with_options(&options);
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
        let command = app_command(
            &LaunchOptions::new(),
            OsStr::new("app"),
            1234,
            Path::new("/c"),
        );
        assert_eq!(
            env_of(&command, "RC_SECRET_STORE").as_deref(),
            Some("memory")
        );
        assert_eq!(
            env_of(&command, "QT_QPA_PLATFORM").as_deref(),
            Some("offscreen")
        );
        assert_eq!(
            env_of(&command, "RC_FATAL_QML_WARNINGS").as_deref(),
            Some("1")
        );
    }

    // INFRA-002
    #[test]
    fn extra_environment_overrides_the_defaults() {
        let options = LaunchOptions::new().env(&[("RC_SECRET_STORE", "file:/x"), ("EXTRA", "1")]);
        let command = app_command(&options, OsStr::new("app"), 1234, Path::new("/c"));
        assert_eq!(
            env_of(&command, "RC_SECRET_STORE").as_deref(),
            Some("file:/x")
        );
        assert_eq!(env_of(&command, "EXTRA").as_deref(), Some("1"));
    }

    // INFRA-002
    #[test]
    fn a_wrapper_goes_in_front_of_the_app() {
        let options = LaunchOptions::new()
            .wrapper("perf", ["stat", "--"])
            .env(&[("EXTRA", "1")]);
        let command = app_command(&options, OsStr::new("/bin/app"), 1234, Path::new("/c"));
        assert_eq!(command.get_program(), "perf");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["stat", "--", "/bin/app", "--spix-port", "1234"]);
        assert_eq!(env_of(&command, "EXTRA").as_deref(), Some("1"));
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
    fn extra_arguments_follow_the_spix_port() {
        let options = LaunchOptions::new().arg("--gh");
        let command = app_command(&options, OsStr::new("/bin/app"), 1234, Path::new("/c"));
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["--spix-port", "1234", "--gh"]);
    }

    // INFRA-002
    #[test]
    fn without_a_wrapper_the_app_is_the_program() {
        let command = app_command(
            &LaunchOptions::new(),
            OsStr::new("/bin/app"),
            1234,
            Path::new("/c"),
        );
        assert_eq!(command.get_program(), "/bin/app");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["--spix-port", "1234"]);
    }

    // RICH-004
    #[test]
    fn the_app_gets_a_cache_of_its_own() {
        let command = app_command(
            &LaunchOptions::new(),
            OsStr::new("app"),
            1234,
            Path::new("/c/1234"),
        );
        assert_eq!(
            env_of(&command, "XDG_CACHE_HOME").as_deref(),
            Some("/c/1234")
        );
        assert_ne!(CacheDir::new(1234).0, CacheDir::new(1235).0);
    }

    // INFRA-002
    #[test]
    fn never_hands_out_a_port_twice() {
        let threads: Vec<_> = (0..4)
            .map(|_| thread::spawn(|| (0..50).map(|_| free_port().unwrap()).collect::<Vec<_>>()))
            .collect();
        let ports: Vec<u16> = threads
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect();
        let distinct: BTreeSet<_> = ports.iter().collect();
        assert_eq!(distinct.len(), ports.len());
    }

    /// An `App` around any process; nothing listens on its Spix port.
    fn app_around(command: &mut Command) -> App {
        let port = free_port().unwrap();
        App {
            child: command.stdin(Stdio::null()).spawn().unwrap(),
            wrapped: false,
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

    // INFRA-002
    #[cfg(unix)]
    #[test]
    fn dropping_a_wrapped_app_lets_it_exit_by_itself() {
        let marker = env::temp_dir().join(format!("rc_gui_tests-wrapper-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        // Stands in for `perf`: writes its result only when it exits by itself, a while after
        // the drop starts. The quit call fails (nothing listens), so the drop has to wait; a kill
        // would leave no marker. The wait allows several seconds for a delay of half a second.
        let mut app = app_around(Command::new("/bin/sh").args([
            "-c",
            "sleep 0.5; : > \"$1\"",
            "sh",
            marker.to_str().unwrap(),
        ]));
        app.wrapped = true;
        drop(app);
        let written = marker.exists();
        let _ = std::fs::remove_file(&marker);
        assert!(written, "the wrapper was killed before it could exit");
    }
}
