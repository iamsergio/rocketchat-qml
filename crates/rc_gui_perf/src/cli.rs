// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The command line: which scenarios run in which app, on which platform, how often.

use std::fmt;
use std::path::PathBuf;

/// The scenarios, each of which runs in both apps.
pub const SCENARIOS: [&str; 1] = ["scroll_main_channel"];

/// Environment variables that name the app binaries, for `--qt` and `--slint`.
pub const QT_ENV: &str = "RC_PERF_QT_APP";
pub const SLINT_ENV: &str = "RC_PERF_SLINT_APP";

const DEFAULT_RUNS: u32 = 5;
const DEFAULT_OUT: &str = "target/perf";

pub const HELP: &str = "\
Runs micro performance scenarios against the Qt and the Slint app under `perf stat` and writes one
JSON file per run.

Usage: rc_gui_perf [options] [scenario...]

  --list             print every scenario once per app (qt.<name>, slint.<name>) and exit
  scenario           a prefixed name (qt.scroll_main_channel) runs that one; a bare name
                     (scroll_main_channel) runs it in both apps; no name runs everything
  --qt <binary>      the Qt app, built with Spix (or set RC_PERF_QT_APP)
  --slint <binary>   the Slint app, built with Spix (or set RC_PERF_SLINT_APP)
  --platform <name>  wayland (default) or xcb: the apps run visibly with GPU rendering.
                     offscreen is a smoke mode only (Qt offscreen, Slint testing backend, which
                     draws nothing): it checks the code path without a display, and its numbers
                     mean nothing. The JSON says so (\"smoke\": true).
  --runs <n>         runs per scenario and app (default 5)
  --out <dir>        where the JSON files go (default target/perf)
  --help             this text

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

/// One scenario in one app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub frontend: Frontend,
    pub scenario: &'static str,
}

impl Job {
    /// `qt.scroll_main_channel`: how `--list` and the command line name it.
    #[must_use]
    pub fn id(self) -> String {
        format!("{}.{}", self.frontend, self.scenario)
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

#[derive(Debug, PartialEq, Eq)]
pub enum Cli {
    Help,
    List,
    Run(Options),
}

/// Every scenario once per app, one id per line.
#[must_use]
pub fn list() -> String {
    all_jobs().iter().map(|job| job.id() + "\n").collect()
}

fn all_jobs() -> Vec<Job> {
    Frontend::ALL
        .into_iter()
        .flat_map(|frontend| {
            SCENARIOS
                .into_iter()
                .map(move |scenario| Job { frontend, scenario })
        })
        .collect()
}

/// The jobs the names select: a prefixed name one job, a bare name that scenario in both apps,
/// no name everything. Each job once, in the order they were asked for.
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
                    && prefix.is_none_or(|prefix| job.frontend.name() == prefix)
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

/// Reads the command line (without the program name). `env` looks up an environment variable.
///
/// # Errors
///
/// For an unknown option or scenario, a missing or bad value, or a selected app without binary.
pub fn parse(
    args: impl IntoIterator<Item = String>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Cli, CliError> {
    let mut args = args.into_iter();
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
            _ if flag.starts_with('-') => return Err(CliError::UnknownOption(flag)),
            _ => names.push(arg),
        }
    }
    if list_only {
        return Ok(Cli::List);
    }
    let jobs = select(&names)?;
    for frontend in Frontend::ALL {
        let (binary, flag, env) = match frontend {
            Frontend::Qt => (&qt, "--qt", QT_ENV),
            Frontend::Slint => (&slint, "--slint", SLINT_ENV),
        };
        if binary.is_none() && jobs.iter().any(|job| job.frontend == frontend) {
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
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
    fn list_prints_every_scenario_once_per_app() {
        assert_eq!(
            list(),
            "qt.scroll_main_channel\nslint.scroll_main_channel\n"
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
                frontend: Frontend::Qt,
                scenario: "scroll_main_channel"
            }]
        );
    }

    // INFRA-007
    #[test]
    fn a_bare_name_runs_both_apps_and_no_name_runs_everything() {
        let both = run(&["--qt", "q", "--slint", "s", "scroll_main_channel"]).unwrap();
        let none = run(&["--qt", "q", "--slint", "s"]).unwrap();
        assert_eq!(both.jobs.len(), 2);
        assert_eq!(both.jobs, none.jobs);
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
        assert_eq!(options.jobs.len(), 2);
        assert_eq!(options.jobs[0].frontend, Frontend::Qt);
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
        assert_eq!(options.runs, 5);
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
