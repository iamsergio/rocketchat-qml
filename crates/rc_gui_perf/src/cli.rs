// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The command line: which scenarios run in which variant, on which platform, how often.

use std::fmt;
use std::path::PathBuf;

/// The scenarios, each of which runs in every variant.
pub const SCENARIOS: [&str; 2] = ["scroll_main_channel", "drag_main_channel"];

/// Environment variables that name the app binaries, for `--qt` and `--slint`.
pub const QT_ENV: &str = "RC_PERF_QT_APP";
pub const SLINT_ENV: &str = "RC_PERF_SLINT_APP";

const DEFAULT_RUNS: u32 = 1;
const DEFAULT_OUT: &str = "target/perf";

pub const HELP: &str = "\
Runs micro performance scenarios against the Qt and the Slint app under `perf stat` and writes one
JSON file per run. Every run is one variant: qt, slint-femtovg, slint-skia-opengl, slint-skia-vulkan or
slint-software (the Slint ones are the same app with another renderer, picked through
RC_SLINT_RENDERER).

Usage: rc_gui_perf [options] [scenario...]
       rc_gui_perf report <dir> [--baseline <dir>] [-o <file>]

  --list             print every scenario once per variant (<variant>.<name>) and exit
  scenario           a full name (slint-software.scroll_main_channel) runs that one;
                     slint.<name> runs it in every Slint variant; a bare name
                     (scroll_main_channel) runs it in all variants; no name runs everything
  --qt <binary>      the Qt app, built with Spix (or set RC_PERF_QT_APP)
  --slint <binary>   the Slint app, built with Spix, for every Slint variant (or set
                     RC_PERF_SLINT_APP)
  --platform <name>  wayland (default) or xcb: the apps run visibly with GPU rendering.
                     offscreen is a smoke mode only (Qt offscreen, Slint testing backend, which
                     draws nothing, whatever the Slint variant): it checks the code path without
                     a display, and its numbers mean nothing. The JSON says so (\"smoke\": true).
  --runs <n>         runs per scenario and variant (default 1)
  --out <dir>        where the JSON files go (default target/perf)
  --record           sample the measured part with `perf record` (user space, frame pointer call
                     graphs) and write <variant>.<scenario>.<run>.perf.data, .folded and .svg (a
                     flame graph) instead of the JSON: sampling skews the counters
  --kernel           also count the kernel side (cycles:k, instructions:k): needs
                     perf_event_paranoid 1 or lower (`sudo sysctl kernel.perf_event_paranoid=1`);
                     not with --record
  --help             this text

report reads the JSON files of a directory and writes one self-contained HTML file (default
<dir>/report.html): the scroll facts, the counters per pixel and per frame with their spread,
each Slint variant's ratio to Qt and, with --baseline, the change against the runs of another directory.

The test server is the one RC_TEST_SERVER_URL names.";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CliError {
    #[error("{0} needs a value")]
    MissingValue(String),
    #[error("unknown option {0}")]
    UnknownOption(String),
    #[error("unknown scenario {0:?}: --list shows them")]
    UnknownScenario(String),
    #[error("unknown platform {0:?}: wayland, xcb or offscreen")]
    UnknownPlatform(String),
    #[error("report needs the directory of the runs")]
    NoReportDir,
    #[error("--kernel and --record don't go together: recording stays in user space")]
    KernelWithRecord,
    #[error("--runs wants a positive number, not {0:?}")]
    BadRuns(String),
    #[error("the {frontend} app is selected but has no binary: pass {flag} or set {env}")]
    NoBinary {
        frontend: Frontend,
        flag: &'static str,
        env: &'static str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Frontend {
    Qt,
    Slint,
}

impl Frontend {
    const ALL: [Self; 2] = [Self::Qt, Self::Slint];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Qt => "qt",
            Self::Slint => "slint",
        }
    }
}

impl fmt::Display for Frontend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a run measures: the Qt app, or the Slint app with one of its renderers. A Slint renderer
/// that needs another build (Skia, which only the perf build has) fails when the app starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Variant {
    Qt,
    SlintFemtovg,
    SlintSkiaOpenGl,
    SlintSkiaVulkan,
    SlintSoftware,
}

impl Variant {
    /// In the order of `--list` and of the report.
    pub const ALL: [Self; 5] = [
        Self::Qt,
        Self::SlintFemtovg,
        Self::SlintSkiaOpenGl,
        Self::SlintSkiaVulkan,
        Self::SlintSoftware,
    ];

    /// `qt`, `slint-femtovg`: how the command line, the JSON and the file names say it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Qt => "qt",
            Self::SlintFemtovg => "slint-femtovg",
            Self::SlintSkiaOpenGl => "slint-skia-opengl",
            Self::SlintSkiaVulkan => "slint-skia-vulkan",
            Self::SlintSoftware => "slint-software",
        }
    }

    /// The app the variant runs, whose binary it needs.
    #[must_use]
    pub fn frontend(self) -> Frontend {
        match self {
            Self::Qt => Frontend::Qt,
            Self::SlintFemtovg
            | Self::SlintSkiaOpenGl
            | Self::SlintSkiaVulkan
            | Self::SlintSoftware => Frontend::Slint,
        }
    }

    /// The value of the Slint app's `RC_SLINT_RENDERER`; none for Qt.
    #[must_use]
    pub fn renderer(self) -> Option<&'static str> {
        match self {
            Self::Qt => None,
            Self::SlintFemtovg => Some("femtovg"),
            Self::SlintSkiaOpenGl => Some("skia-opengl"),
            Self::SlintSkiaVulkan => Some("skia-vulkan"),
            Self::SlintSoftware => Some("software"),
        }
    }

    /// The variant an `app` field of a JSON file names. Files from before the variants say `slint`
    /// for the femtovg renderer, the only one they ran. Files from before Skia was split say
    /// `slint-skia`, which was Skia's first choice, Vulkan through wgpu.
    #[must_use]
    pub fn from_stored(app: &str) -> Option<Self> {
        match app {
            "slint" => Some(Self::SlintFemtovg),
            "slint-skia" => Some(Self::SlintSkiaVulkan),
            other => Self::ALL.into_iter().find(|variant| variant.id() == other),
        }
    }

    /// For the report: `Qt`, `Slint femtovg`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Qt => "Qt",
            Self::SlintFemtovg => "Slint femtovg",
            Self::SlintSkiaOpenGl => "Slint Skia OpenGL",
            Self::SlintSkiaVulkan => "Slint Skia Vulkan",
            Self::SlintSoftware => "Slint software",
        }
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Wayland,
    Xcb,
    /// Qt's offscreen platform and Slint's testing backend: no display, no GPU.
    Offscreen,
}

impl Platform {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::Xcb => "xcb",
            Self::Offscreen => "offscreen",
        }
    }

    /// Whether the numbers of this platform mean nothing: it only checks the code path.
    #[must_use]
    pub fn is_smoke(self) -> bool {
        self == Self::Offscreen
    }

    fn parse(name: &str) -> Result<Self, CliError> {
        match name {
            "wayland" => Ok(Self::Wayland),
            "xcb" => Ok(Self::Xcb),
            "offscreen" => Ok(Self::Offscreen),
            _ => Err(CliError::UnknownPlatform(name.to_owned())),
        }
    }
}

/// One scenario in one variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub variant: Variant,
    pub scenario: &'static str,
}

impl Job {
    /// `qt.scroll_main_channel`: how `--list` and the command line name it.
    #[must_use]
    pub fn id(self) -> String {
        format!("{}.{}", self.variant, self.scenario)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub jobs: Vec<Job>,
    pub qt: Option<PathBuf>,
    pub slint: Option<PathBuf>,
    pub platform: Platform,
    pub runs: u32,
    pub out: PathBuf,
    /// Sample with `perf record` and write flame graphs, not counter JSON.
    pub record: bool,
    /// Also count kernel space (`--kernel`).
    pub kernel: bool,
}

impl Options {
    /// The binary of an app. Present for every app that has a job: [`parse`] checks.
    #[must_use]
    pub fn binary(&self, frontend: Frontend) -> Option<&PathBuf> {
        match frontend {
            Frontend::Qt => self.qt.as_ref(),
            Frontend::Slint => self.slint.as_ref(),
        }
    }
}

/// `rc_gui_perf report`: the runs of `dir` as HTML.
#[derive(Debug, PartialEq, Eq)]
pub struct ReportOptions {
    pub dir: PathBuf,
    pub baseline: Option<PathBuf>,
    pub output: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Cli {
    Help,
    List,
    Report(ReportOptions),
    Run(Options),
}

/// Every scenario once per variant, one id per line.
#[must_use]
pub fn list() -> String {
    all_jobs().iter().map(|job| job.id() + "\n").collect()
}

fn all_jobs() -> Vec<Job> {
    Variant::ALL
        .into_iter()
        .flat_map(|variant| {
            SCENARIOS
                .into_iter()
                .map(move |scenario| Job { variant, scenario })
        })
        .collect()
}

/// The jobs the names select: a full name (`slint-software.drag_main_channel`) one job, `slint.<name>`
/// that scenario in every Slint variant, a bare name in all variants, no name everything. Each job once, in the order they were asked for.
fn select(names: &[String]) -> Result<Vec<Job>, CliError> {
    if names.is_empty() {
        return Ok(all_jobs());
    }
    let mut jobs = Vec::new();
    for name in names {
        let unknown = || CliError::UnknownScenario(name.clone());
        let (prefix, bare) = name.split_once('.').unzip();
        let selected: Vec<Job> = all_jobs()
            .into_iter()
            .filter(|job| {
                job.scenario == bare.unwrap_or(name)
                    && prefix.is_none_or(|prefix| {
                        job.variant.id() == prefix || job.variant.frontend().name() == prefix
                    })
            })
            .collect();
        if selected.is_empty() {
            return Err(unknown());
        }
        for job in selected {
            if !jobs.contains(&job) {
                jobs.push(job);
            }
        }
    }
    Ok(jobs)
}

fn parse_report(mut args: impl Iterator<Item = String>) -> Result<Cli, CliError> {
    let mut dir = None;
    let mut baseline = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        let (flag, mut inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let mut value = || {
            inline
                .take()
                .or_else(|| args.next())
                .ok_or_else(|| CliError::MissingValue(flag.clone()))
        };
        match flag.as_str() {
            "--help" | "-h" => return Ok(Cli::Help),
            "--baseline" => baseline = Some(PathBuf::from(value()?)),
            "-o" | "--output" => output = Some(PathBuf::from(value()?)),
            _ if flag.starts_with('-') => return Err(CliError::UnknownOption(flag)),
            _ => dir = Some(PathBuf::from(arg)),
        }
    }
    let dir = dir.ok_or(CliError::NoReportDir)?;
    let output = output.unwrap_or_else(|| dir.join("report.html"));
    Ok(Cli::Report(ReportOptions {
        dir,
        baseline,
        output,
    }))
}

/// Reads the command line (without the program name). `env` looks up an environment variable.
///
/// # Errors
///
/// For an unknown option or scenario, a missing or bad value, or a selected app without binary.
pub fn parse(
    args: impl IntoIterator<Item = String>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Cli, CliError> {
    let mut args = args.into_iter().peekable();
    if args.next_if(|arg| arg == "report").is_some() {
        return parse_report(args);
    }
    let mut names = Vec::new();
    let mut list_only = false;
    let mut qt = env(QT_ENV)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let mut slint = env(SLINT_ENV)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let mut platform = Platform::Wayland;
    let mut runs = DEFAULT_RUNS;
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut record = false;
    let mut kernel = false;
    while let Some(arg) = args.next() {
        // `--flag=value` is the same as `--flag value`.
        let (flag, mut inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let mut value = || {
            inline
                .take()
                .or_else(|| args.next())
                .ok_or_else(|| CliError::MissingValue(flag.clone()))
        };
        match flag.as_str() {
            "--help" | "-h" => return Ok(Cli::Help),
            "--list" => list_only = true,
            "--qt" => qt = Some(PathBuf::from(value()?)),
            "--slint" => slint = Some(PathBuf::from(value()?)),
            "--platform" => platform = Platform::parse(&value()?)?,
            "--runs" => {
                let text = value()?;
                runs = text
                    .parse()
                    .ok()
                    .filter(|runs| *runs > 0)
                    .ok_or(CliError::BadRuns(text))?;
            }
            "--out" => out = PathBuf::from(value()?),
            "--record" => record = true,
            "--kernel" => kernel = true,
            _ if flag.starts_with('-') => return Err(CliError::UnknownOption(flag)),
            _ => names.push(arg),
        }
    }
    if list_only {
        return Ok(Cli::List);
    }
    if kernel && record {
        return Err(CliError::KernelWithRecord);
    }
    let jobs = select(&names)?;
    for frontend in Frontend::ALL {
        let (binary, flag, env) = match frontend {
            Frontend::Qt => (&qt, "--qt", QT_ENV),
            Frontend::Slint => (&slint, "--slint", SLINT_ENV),
        };
        if binary.is_none() && jobs.iter().any(|job| job.variant.frontend() == frontend) {
            return Err(CliError::NoBinary {
                frontend,
                flag,
                env,
            });
        }
    }
    Ok(Cli::Run(Options {
        jobs,
        qt,
        slint,
        platform,
        runs,
        out,
        record,
        kernel,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    // INFRA-007
    #[test]
    fn report_takes_a_directory_a_baseline_and_an_output() {
        let Ok(Cli::Report(options)) = parse(args(&["report", "d", "--baseline", "b"]), no_env)
        else {
            panic!("not a report");
        };
        assert_eq!(options.dir, PathBuf::from("d"));
        assert_eq!(options.baseline, Some(PathBuf::from("b")));
        assert_eq!(options.output, PathBuf::from("d/report.html"));
        let Ok(Cli::Report(options)) = parse(args(&["report", "-o", "x.html", "d"]), no_env) else {
            panic!("not a report");
        };
        assert_eq!(options.output, PathBuf::from("x.html"));
        assert_eq!(parse(args(&["report"]), no_env), Err(CliError::NoReportDir));
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn run(args_: &[&str]) -> Result<Options, CliError> {
        match parse(args(args_), no_env)? {
            Cli::Run(options) => Ok(options),
            other => panic!("not a run: {other:?}"),
        }
    }

    // INFRA-007
    #[test]
    fn list_prints_every_scenario_once_per_variant() {
        assert_eq!(
            list(),
            "qt.scroll_main_channel\nqt.drag_main_channel\n\
             slint-femtovg.scroll_main_channel\nslint-femtovg.drag_main_channel\n\
             slint-skia-opengl.scroll_main_channel\nslint-skia-opengl.drag_main_channel\n\
             slint-skia-vulkan.scroll_main_channel\nslint-skia-vulkan.drag_main_channel\n\
             slint-software.scroll_main_channel\nslint-software.drag_main_channel\n"
        );
        assert_eq!(parse(args(&["--list"]), no_env), Ok(Cli::List));
    }

    // INFRA-007
    #[test]
    fn a_prefixed_name_runs_one_app() {
        let options = run(&["--qt", "q", "qt.scroll_main_channel"]).unwrap();
        assert_eq!(
            options.jobs,
            [Job {
                variant: Variant::Qt,
                scenario: "scroll_main_channel"
            }]
        );
    }

    // INFRA-007
    #[test]
    fn a_full_name_runs_one_variant_and_slint_runs_every_slint_one() {
        let one = run(&["--slint", "s", "slint-software.drag_main_channel"]).unwrap();
        assert_eq!(one.jobs.len(), 1);
        assert_eq!(one.jobs[0].variant, Variant::SlintSoftware);
        let slint = run(&["--slint", "s", "slint.scroll_main_channel"]).unwrap();
        let ids: Vec<String> = slint.jobs.iter().map(|job| job.id()).collect();
        assert_eq!(
            ids,
            [
                "slint-femtovg.scroll_main_channel",
                "slint-skia-opengl.scroll_main_channel",
                "slint-skia-vulkan.scroll_main_channel",
                "slint-software.scroll_main_channel"
            ]
        );
        let skia = run(&["--slint", "s", "slint-skia-opengl.drag_main_channel"]).unwrap();
        assert_eq!(skia.jobs.len(), 1);
        assert_eq!(skia.jobs[0].variant, Variant::SlintSkiaOpenGl);
        // The unsplit name is only for old files.
        assert_eq!(
            run(&["slint-skia.drag_main_channel"]),
            Err(CliError::UnknownScenario(
                "slint-skia.drag_main_channel".into()
            ))
        );
        // The Qt app isn't needed for them.
        assert_eq!(slint.qt, None);
        assert_eq!(
            run(&["slint-gl.scroll_main_channel"]),
            Err(CliError::UnknownScenario(
                "slint-gl.scroll_main_channel".into()
            ))
        );
    }

    // INFRA-007
    #[test]
    fn variants_know_their_app_renderer_and_old_names() {
        assert_eq!(Variant::Qt.frontend(), Frontend::Qt);
        assert_eq!(Variant::SlintSoftware.frontend(), Frontend::Slint);
        assert_eq!(Variant::Qt.renderer(), None);
        assert_eq!(Variant::SlintFemtovg.renderer(), Some("femtovg"));
        assert_eq!(Variant::SlintSkiaOpenGl.renderer(), Some("skia-opengl"));
        assert_eq!(Variant::SlintSkiaVulkan.renderer(), Some("skia-vulkan"));
        assert_eq!(Variant::SlintSoftware.renderer(), Some("software"));
        for variant in Variant::ALL {
            assert_eq!(Variant::from_stored(variant.id()), Some(variant));
        }
        // Files from before the variants.
        assert_eq!(Variant::from_stored("slint"), Some(Variant::SlintFemtovg));
        // Files from before Skia was split measured Skia's first choice, Vulkan.
        assert_eq!(
            Variant::from_stored("slint-skia"),
            Some(Variant::SlintSkiaVulkan)
        );
        assert_eq!(Variant::from_stored("gtk"), None);
    }

    // INFRA-007
    #[test]
    fn a_bare_name_runs_every_variant_and_no_name_runs_everything() {
        let all = run(&["--qt", "q", "--slint", "s", "scroll_main_channel"]).unwrap();
        let none = run(&["--qt", "q", "--slint", "s"]).unwrap();
        assert_eq!(all.jobs.len(), 5);
        assert_eq!(none.jobs.len(), 10);
        assert_eq!(
            all.jobs,
            [
                none.jobs[0],
                none.jobs[2],
                none.jobs[4],
                none.jobs[6],
                none.jobs[8]
            ]
        );
        let drag = run(&["--qt", "q", "--slint", "s", "drag_main_channel"]).unwrap();
        assert_eq!(
            drag.jobs,
            [
                none.jobs[1],
                none.jobs[3],
                none.jobs[5],
                none.jobs[7],
                none.jobs[9]
            ]
        );
        assert_eq!(drag.jobs[0].id(), "qt.drag_main_channel");
    }

    // INFRA-007
    #[test]
    fn a_job_named_twice_runs_once() {
        let options = run(&[
            "--qt",
            "q",
            "--slint",
            "s",
            "qt.scroll_main_channel",
            "scroll_main_channel",
        ])
        .unwrap();
        assert_eq!(options.jobs.len(), 5);
        assert_eq!(options.jobs[0].variant, Variant::Qt);
    }

    // INFRA-007
    #[test]
    fn unknown_names_are_errors() {
        assert_eq!(
            run(&["nope"]),
            Err(CliError::UnknownScenario("nope".into()))
        );
        assert_eq!(
            run(&["gtk.scroll_main_channel"]),
            Err(CliError::UnknownScenario("gtk.scroll_main_channel".into()))
        );
    }

    // INFRA-007
    #[test]
    fn a_selected_app_needs_its_binary() {
        let error = run(&["--qt", "q", "slint.scroll_main_channel"]).unwrap_err();
        assert!(matches!(
            error,
            CliError::NoBinary {
                frontend: Frontend::Slint,
                ..
            }
        ));
        // The other app needs none.
        assert!(run(&["--qt", "q", "qt.scroll_main_channel"]).is_ok());
    }

    // INFRA-007
    #[test]
    fn binaries_come_from_the_environment_too() {
        let env = |name: &str| (name == SLINT_ENV).then(|| "/s".to_owned());
        let Ok(Cli::Run(options)) = parse(args(&["slint.scroll_main_channel"]), env) else {
            panic!("not a run");
        };
        assert_eq!(options.slint, Some(PathBuf::from("/s")));
        // A flag wins over the environment.
        let Ok(Cli::Run(options)) = parse(args(&["--slint=/f", "slint.scroll_main_channel"]), env)
        else {
            panic!("not a run");
        };
        assert_eq!(options.slint, Some(PathBuf::from("/f")));
    }

    // INFRA-007
    #[test]
    fn defaults_and_values() {
        let options = run(&["--qt", "q", "qt.scroll_main_channel"]).unwrap();
        assert_eq!(options.platform, Platform::Wayland);
        assert_eq!(options.runs, 1);
        assert_eq!(options.out, PathBuf::from("target/perf"));
        let options = run(&[
            "--qt",
            "q",
            "--platform",
            "offscreen",
            "--runs",
            "2",
            "--out",
            "/o",
            "qt.scroll_main_channel",
        ])
        .unwrap();
        assert!(options.platform.is_smoke());
        assert_eq!(options.runs, 2);
        assert_eq!(options.out, PathBuf::from("/o"));
    }

    // INFRA-008
    #[test]
    fn record_is_an_option_of_runs_and_off_by_default() {
        let options = run(&["--qt", "q", "qt.scroll_main_channel"]).unwrap();
        assert!(!options.record);
        let options = run(&["--qt", "q", "--record", "qt.scroll_main_channel"]).unwrap();
        assert!(options.record);
    }

    // INFRA-007
    #[test]
    fn kernel_is_an_option_of_runs_off_by_default_and_not_with_record() {
        assert!(
            !run(&["--qt", "q", "qt.scroll_main_channel"])
                .unwrap()
                .kernel
        );
        let options = run(&["--qt", "q", "--kernel", "qt.scroll_main_channel"]).unwrap();
        assert!(options.kernel);
        assert_eq!(
            run(&[
                "--qt",
                "q",
                "--kernel",
                "--record",
                "qt.scroll_main_channel"
            ]),
            Err(CliError::KernelWithRecord)
        );
    }

    // INFRA-007
    #[test]
    fn bad_values_are_errors() {
        assert_eq!(
            run(&["--platform", "gdi"]),
            Err(CliError::UnknownPlatform("gdi".into()))
        );
        assert_eq!(run(&["--runs", "0"]), Err(CliError::BadRuns("0".into())));
        assert_eq!(
            run(&["--runs"]),
            Err(CliError::MissingValue("--runs".into()))
        );
        assert_eq!(
            run(&["--bogus"]),
            Err(CliError::UnknownOption("--bogus".into()))
        );
    }
}
