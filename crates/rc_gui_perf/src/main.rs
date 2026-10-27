// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rc_gui_perf`: micro performance scenarios against the Qt and the Slint app, each run a fresh
//! app under `perf stat` that counts only the measured part. See docs/design/performance-tests.md.

mod cli;
mod error;
mod perf;
mod report;
mod scroll_main_channel;
mod seed;

use std::collections::BTreeMap;
use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use rc_gui_tests::{App, LaunchOptions};

use crate::cli::{Cli, Frontend, Job, Options, Platform};
use crate::error::Error;
use crate::perf::Control;
use crate::report::RunInfo;
use crate::seed::Channel;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rc_gui_perf: {error}");
            let mut source = error.source();
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Error> {
    match cli::parse(env::args().skip(1), |name| env::var(name).ok())? {
        Cli::Help => println!("{}", cli::HELP),
        Cli::List => print!("{}", cli::list()),
        Cli::Run(options) => run_all(&options)?,
    }
    Ok(())
}

fn run_all(options: &Options) -> Result<(), Error> {
    for job in &options.jobs {
        if let Some(binary) = options.binary(job.frontend)
            && !binary.is_file()
        {
            return Err(Error::NoSuchBinary(binary.clone()));
        }
    }
    perf::check_available()?;
    fs::create_dir_all(&options.out)
        .map_err(|source| Error::io("can't make", &options.out, source))?;
    let server = rc_test_server::server_url();
    let git_revision = git_revision();
    if options.platform.is_smoke() {
        eprintln!("smoke mode: offscreen draws nothing real, the numbers mean nothing");
    }

    // Seeding first, once per scenario: no app is running while the server is busy.
    let mut channels: BTreeMap<&'static str, Channel> = BTreeMap::new();
    for job in &options.jobs {
        if !channels.contains_key(job.scenario) {
            eprintln!("seeding {}", job.scenario);
            channels.insert(job.scenario, seed_for(job.scenario)?);
        }
    }

    // Run by run across the jobs, so that drift of the machine falls on both apps alike.
    for run in 0..options.runs {
        for job in &options.jobs {
            let Some(channel) = channels.get(job.scenario) else {
                continue;
            };
            let file = run_once(options, *job, run, channel, &server, &git_revision)?;
            eprintln!("{} run {run}: {}", job.id(), file.display());
        }
    }
    Ok(())
}

fn seed_for(scenario: &str) -> Result<Channel, Error> {
    match scenario {
        "scroll_main_channel" => Ok(seed::scroll_main_channel(|line| eprintln!("  {line}"))?),
        _ => Err(Error::Scenario(format!("no seeding for {scenario}"))),
    }
}

/// The environment that picks the platform for an app.
fn platform_env(frontend: Frontend, platform: Platform) -> Vec<(&'static str, &'static str)> {
    // Spix's own defaults are Qt's offscreen platform and Slint's testing backend.
    match (frontend, platform) {
        (Frontend::Qt, platform) => vec![("QT_QPA_PLATFORM", platform.name())],
        (Frontend::Slint, Platform::Offscreen) => Vec::new(),
        // Winit with the GPU renderer the app builds (femtovg). Winit takes Wayland whenever a
        // compositor is there and has no variable to pick X11 (WINIT_UNIX_BACKEND went in 0.29),
        // but it treats empty Wayland variables as unset.
        (Frontend::Slint, Platform::Xcb) => vec![
            ("SLINT_BACKEND", "winit-femtovg"),
            ("WAYLAND_DISPLAY", ""),
            ("WAYLAND_SOCKET", ""),
        ],
        (Frontend::Slint, _) => vec![("SLINT_BACKEND", "winit-femtovg")],
    }
}

fn run_once(
    options: &Options,
    job: Job,
    run: u32,
    channel: &Channel,
    server: &str,
    git_revision: &str,
) -> Result<PathBuf, Error> {
    let Some(binary) = options.binary(job.frontend) else {
        return Err(Error::Scenario(format!("no binary for {}", job.frontend)));
    };
    let work = TempDir::new(&format!("{}-{run}", job.id()))?;
    let csv = work.0.join("perf.csv");
    let mut control = Control::create(&work.0)?;
    // The session stays in memory (the harness default), never in the user's keyring.
    let launch = LaunchOptions::new()
        .binary(binary)
        .wrapper("perf", perf::stat_args(&control, &csv))
        .env(&[("TZ", "UTC")])
        .env(&platform_env(job.frontend, options.platform));
    let app = App::launch_with_options(&launch)?;
    control.connect()?;

    let measured = match job.scenario {
        "scroll_main_channel" => {
            let length = scroll_main_channel::prepare(app.gui(), app.cache_dir(), server, channel)?;
            eprintln!("  {}: the list is {length:.0} px long", job.id());
            scroll_main_channel::measure(app.gui(), &mut control)?
        }
        other => return Err(Error::Scenario(format!("unknown scenario {other}"))),
    };
    drop(control);

    // perf writes its result when the app exits.
    let status = app.quit_and_wait()?;
    if !status.success() {
        eprintln!("  {}: the app exited with {status}", job.id());
    }
    let text = fs::read_to_string(&csv).map_err(|source| perf::PerfError::NoOutput {
        path: csv.clone(),
        source,
    })?;
    let counters = perf::parse_csv(&text);
    // The file says where the binary was, wherever the runner was started from.
    let binary = binary.canonicalize().unwrap_or_else(|_| binary.clone());
    let info = RunInfo {
        job,
        platform: options.platform,
        run,
        binary: &binary,
        git_revision,
    };
    let json = report::to_json(&info, &measured, &counters);
    let file = report::path(&options.out, job, run);
    let pretty =
        serde_json::to_string_pretty(&json).map_err(|error| Error::Scenario(error.to_string()))?;
    fs::write(&file, pretty + "\n").map_err(|source| Error::io("can't write", &file, source))?;
    Ok(file)
}

/// The revision the apps were built from, as far as this checkout says: `git describe` with
/// `--dirty` for changes to tracked files.
fn git_revision() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    Command::new("git")
        .args(["describe", "--always", "--dirty"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unknown".to_owned(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )
}

/// A directory under the system's temporary one, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, Error> {
        let dir = env::temp_dir().join(format!("rc_gui_perf-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|source| Error::io("can't make", &dir, source))?;
        Ok(Self(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Nothing to do if it is gone.
        let _ = fs::remove_dir_all(&self.0);
    }
}
