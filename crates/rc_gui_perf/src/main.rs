// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rc_gui_perf`: micro performance scenarios against the Qt and the Slint app, each run a fresh
//! app under `perf stat` that counts only the measured part. See docs/design/performance-tests.md.

mod aggregate;
mod cli;
mod drag_main_channel;
mod error;
mod flamegraph;
mod html;
mod main_channel;
mod perf;
mod report;
mod scroll_main_channel;
mod seed;

use std::collections::BTreeMap;
use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;
use std::{env, fs};

use rc_gui_tests::{App, LaunchOptions};

use crate::cli::{Cli, Job, Options, Platform, Variant};
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
        Cli::Report(options) => {
            html::write(&options.dir, options.baseline.as_deref(), &options.output)?;
            println!("{}", options.output.display());
        }
        Cli::Run(options) => run_all(&options)?,
    }
    Ok(())
}

fn run_all(options: &Options) -> Result<(), Error> {
    for job in &options.jobs {
        if let Some(binary) = options.binary(job.variant.frontend())
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

    // Seeding first, once per content: no app is running while the server is busy. Scenarios on
    // the same content share the channel.
    let mut channels: BTreeMap<&'static str, Channel> = BTreeMap::new();
    for job in &options.jobs {
        let content = content_of(job.scenario)?;
        if !channels.contains_key(content) {
            eprintln!("seeding {content}");
            channels.insert(content, seed_for(content)?);
        }
    }

    // Run by run across the jobs, so that drift of the machine falls on every variant alike.
    for run in 0..options.runs {
        for job in &options.jobs {
            let Some(channel) = content_of(job.scenario)
                .ok()
                .and_then(|content| channels.get(content))
            else {
                continue;
            };
            let file = run_once(options, *job, run, channel, &server, &git_revision)?;
            eprintln!("{} run {run}: {}", job.id(), file.display());
        }
    }
    Ok(())
}

/// The content a scenario runs on: the name of its seeding.
fn content_of(scenario: &str) -> Result<&'static str, Error> {
    match scenario {
        "scroll_main_channel" | "drag_main_channel" => Ok("main_channel"),
        _ => Err(Error::Scenario(format!("no content for {scenario}"))),
    }
}

fn seed_for(content: &str) -> Result<Channel, Error> {
    match content {
        "main_channel" => Ok(seed::main_channel(|line| eprintln!("  {line}"))?),
        _ => Err(Error::Scenario(format!("no seeding for {content}"))),
    }
}

/// The environment that picks the platform, and for Slint the renderer, of a variant.
fn platform_env(variant: Variant, platform: Platform) -> Vec<(&'static str, &'static str)> {
    // Spix's own defaults are Qt's offscreen platform and Slint's testing backend.
    let Some(renderer) = variant.renderer() else {
        return vec![("QT_QPA_PLATFORM", platform.name())];
    };
    match platform {
        // A smoke run keeps the testing backend whatever the variant: it has no window and
        // RC_SLINT_RENDERER would ask for winit, which needs a display.
        Platform::Offscreen => Vec::new(),
        // The app's winit backend with that renderer (`RC_SLINT_RENDERER`, which wins over
        // SLINT_BACKEND). Winit takes Wayland whenever a compositor is there and has no variable
        // to pick X11 (WINIT_UNIX_BACKEND went in 0.29), but it treats empty Wayland variables as
        // unset.
        Platform::Xcb => vec![
            ("RC_SLINT_RENDERER", renderer),
            ("WAYLAND_DISPLAY", ""),
            ("WAYLAND_SOCKET", ""),
        ],
        Platform::Wayland => vec![("RC_SLINT_RENDERER", renderer)],
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
    let Some(binary) = options.binary(job.variant.frontend()) else {
        return Err(Error::Scenario(format!("no binary for {}", job.variant)));
    };
    let work = TempDir::new(&format!("{}-{run}", job.id()))?;
    let csv = work.0.join("perf.csv");
    // perf writes straight to where the recording stays: it can be hundreds of MB.
    let data = report::recording_path(&options.out, job, run, "perf.data");
    let mut control = Control::create(&work.0)?;
    let wrapper_args = if options.record {
        perf::record_args(&control, &data, job.variant.frontend())
    } else {
        perf::stat_args(&control, &csv)
    };
    // The session stays in memory (the harness default), never in the user's keyring.
    let launch = LaunchOptions::new()
        .binary(binary)
        .wrapper("perf", wrapper_args)
        .env(&[("TZ", "UTC")])
        .env(&platform_env(job.variant, options.platform));
    let app = App::launch_with_options(&launch)?;
    control.connect()?;

    let measured = match job.scenario {
        "scroll_main_channel" | "drag_main_channel" => {
            let length = main_channel::prepare(app.gui(), app.cache_dir(), server, channel)?;
            eprintln!("  {}: the list is {length:.0} px long", job.id());
            if job.scenario == "drag_main_channel" {
                drag_main_channel::measure(app.gui(), &mut control)?
            } else {
                scroll_main_channel::measure(app.gui(), &mut control)?
            }
        }
        other => return Err(Error::Scenario(format!("unknown scenario {other}"))),
    };
    drop(control);

    // perf writes its result when the app exits.
    let status = app.quit_and_wait()?;
    if !status.success() {
        eprintln!("  {}: the app exited with {status}", job.id());
    }
    if options.record {
        return write_recording(options, job, run, &data);
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

/// Writes the collapsed stacks and the flame graph beside the recording `data`. No JSON: sampling
/// skews the counters, and the report must not mix such runs in. Returns the SVG.
fn write_recording(options: &Options, job: Job, run: u32, data: &Path) -> Result<PathBuf, Error> {
    let started = Instant::now();
    let written = flamegraph::convert(
        data,
        &report::recording_path(&options.out, job, run, "folded"),
        &report::recording_path(&options.out, job, run, "svg"),
        &format!("{} run {run}", job.id()),
        &format!(
            "cycles:u, {} Hz, measured part only",
            perf::RECORD_FREQUENCY
        ),
        "cycles",
    )?;
    eprintln!(
        "  {}: {} cycles sampled, perf script and fold took {:.1} s: {}",
        job.id(),
        written.cycles,
        started.elapsed().as_secs_f64(),
        written.folded.display()
    );
    Ok(written.svg)
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

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-007
    #[test]
    fn the_environment_of_a_variant_names_its_platform_and_renderer() {
        assert_eq!(
            platform_env(Variant::Qt, Platform::Xcb),
            [("QT_QPA_PLATFORM", "xcb")]
        );
        assert_eq!(
            platform_env(Variant::SlintSoftware, Platform::Wayland),
            [("RC_SLINT_RENDERER", "software")]
        );
        assert_eq!(
            platform_env(Variant::SlintSkiaOpenGl, Platform::Wayland),
            [("RC_SLINT_RENDERER", "skia-opengl")]
        );
        assert_eq!(
            platform_env(Variant::SlintSkiaVulkan, Platform::Wayland),
            [("RC_SLINT_RENDERER", "skia-vulkan")]
        );
        assert_eq!(
            platform_env(Variant::SlintFemtovg, Platform::Xcb),
            [
                ("RC_SLINT_RENDERER", "femtovg"),
                ("WAYLAND_DISPLAY", ""),
                ("WAYLAND_SOCKET", "")
            ]
        );
        // The testing backend, which needs no renderer name and would not take one.
        assert!(platform_env(Variant::SlintFemtovg, Platform::Offscreen).is_empty());
    }
}
