// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `perf stat`: its command line, the control FIFOs that switch the counters on and off, and its
//! CSV output.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::cli::Frontend;

/// The events, in the order the report shows them. `:u` counts user space only, which
/// `perf_event_paranoid` 2 allows without root. `instructions` is the stable metric; `ref-cycles`
/// and `task-clock` are noisy.
pub const EVENTS: [&str; 5] = [
    "instructions:u",
    "branch-misses:u",
    "ref-cycles:u",
    "cycles:u",
    "task-clock",
];

/// How long perf may take to acknowledge a command, and to open its end of the control FIFOs.
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(2);

#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(not(target_os = "linux"))]
const O_NONBLOCK: i32 = 0x4;

#[derive(Debug, thiserror::Error)]
pub enum PerfError {
    #[error("perf can't be run ({0}): is it installed and on PATH?")]
    NotFound(#[source] io::Error),
    #[error("`perf --version` failed: {0}")]
    Broken(String),
    #[error("can't make the control FIFO {}", .path.display())]
    Fifo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("perf doesn't answer on its control FIFOs: {0}")]
    Control(String),
    #[error("perf wrote no result to {}", .path.display())]
    NoOutput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Fails with the reason unless `perf` runs.
///
/// # Errors
///
/// If it isn't on `PATH` or doesn't start.
pub fn check_available() -> Result<(), PerfError> {
    let output = Command::new("perf")
        .arg("--version")
        .output()
        .map_err(PerfError::NotFound)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(PerfError::Broken(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// The arguments after `perf`, up to and including the `--` before the app: counters off at the
/// start (`-D -1`), driven through the FIFOs, CSV into `output`.
#[must_use]
pub fn stat_args(control: &Control, output: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["stat", "-D", "-1", "--control"].map(Into::into).to_vec();
    args.push(
        format!(
            "fifo:{},{}",
            control.ctl_path().display(),
            control.ack_path().display()
        )
        .into(),
    );
    args.extend(["-x,", "-o"].map(OsString::from));
    args.push(output.into());
    args.extend(["-e".into(), EVENTS.join(",").into(), "--".into()]);
    args
}

/// Samples per second of `perf record`. A scroll run of 3 to 12 s gives thousands of samples
/// and a perf.data of a few MB.
pub const RECORD_FREQUENCY: u32 = 4000;

/// The arguments after `perf` of `perf record`, up to and including the `--` before the app. Like
/// [`stat_args`] it starts with sampling off (`-D -1`) and is driven through the FIFOs. `cycles:u`
/// samples user space only, which `perf_event_paranoid` 2 allows. The call graphs unwind as
/// [`call_graph`] says for the app.
#[must_use]
pub fn record_args(control: &Control, output: &Path, frontend: Frontend) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["record", "-D", "-1", "--control"].map(Into::into).to_vec();
    args.push(
        format!(
            "fifo:{},{}",
            control.ctl_path().display(),
            control.ack_path().display()
        )
        .into(),
    );
    args.extend(
        [
            "-e",
            "cycles:u",
            "--call-graph",
            call_graph(frontend),
            "-F",
            &RECORD_FREQUENCY.to_string(),
            "-o",
        ]
        .map(OsString::from),
    );
    args.push(output.into());
    args.push("--".into());
    args
}

/// How `perf record` unwinds the stacks of an app. The Slint app is Rust through and through, and
/// the perf presets keep its frame pointers (`ROCKETCHATQML_FRAME_POINTERS`); dwarf unwinding loses
/// its deep stacks. Qt's own libraries come without frame pointers, so the Qt app unwinds with
/// dwarf and the largest stack copy perf takes, which completes nearly all of its stacks where the
/// default 8 kB left a fifth short.
#[must_use]
pub fn call_graph(frontend: Frontend) -> &'static str {
    match frontend {
        Frontend::Qt => "dwarf,65528",
        Frontend::Slint => "fp",
    }
}

/// The pair of FIFOs `perf stat --control` reads commands from and acknowledges on.
///
/// Opening them takes care: perf opens the acknowledgement FIFO for writing and waits until
/// somebody has it open for reading, and it does so before it starts the app, which the harness
/// waits for. So [`Control::create`] opens that end first, without blocking, and
/// [`Control::connect`] opens the command end once the app is up and perf has its end open.
pub struct Control {
    ctl_path: PathBuf,
    ack_path: PathBuf,
    ctl: Option<File>,
    ack: File,
    /// Bytes of the acknowledgement read but not yet used.
    pending: Vec<u8>,
    timeout: Duration,
}

impl Control {
    /// Makes the FIFOs in `dir`, which must exist, and opens the acknowledgement end.
    ///
    /// # Errors
    ///
    /// If `mkfifo` or the open fails.
    pub fn create(dir: &Path) -> Result<Self, PerfError> {
        let ctl_path = dir.join("ctl");
        let ack_path = dir.join("ack");
        for path in [&ctl_path, &ack_path] {
            let status = Command::new("mkfifo")
                .arg(path)
                .status()
                .map_err(|source| PerfError::Fifo {
                    path: path.clone(),
                    source,
                })?;
            if !status.success() {
                return Err(PerfError::Fifo {
                    path: path.clone(),
                    source: io::Error::other(status.to_string()),
                });
            }
        }
        let ack = OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(&ack_path)
            .map_err(|source| PerfError::Fifo {
                path: ack_path.clone(),
                source,
            })?;
        Ok(Self {
            ctl_path,
            ack_path,
            ctl: None,
            ack,
            pending: Vec::new(),
            timeout: ACK_TIMEOUT,
        })
    }

    /// How long a command may wait for its acknowledgement, and the connection for perf. Tests
    /// shorten it.
    #[cfg(test)]
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn ctl_path(&self) -> &Path {
        &self.ctl_path
    }

    #[must_use]
    pub fn ack_path(&self) -> &Path {
        &self.ack_path
    }

    /// Opens the command end, which fails (`ENXIO`) until perf has the FIFO open for reading.
    ///
    /// # Errors
    ///
    /// If perf hasn't opened it in time.
    pub fn connect(&mut self) -> Result<(), PerfError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            match OpenOptions::new()
                .write(true)
                .custom_flags(O_NONBLOCK)
                .open(&self.ctl_path)
            {
                Ok(file) => {
                    self.ctl = Some(file);
                    return Ok(());
                }
                Err(error) if Instant::now() >= deadline => {
                    return Err(PerfError::Control(format!(
                        "{} has no reader: {error}",
                        self.ctl_path.display()
                    )));
                }
                Err(_) => thread::sleep(POLL_INTERVAL),
            }
        }
    }

    /// Starts counting, and returns once perf has said so.
    ///
    /// # Errors
    ///
    /// If perf doesn't acknowledge.
    pub fn enable(&mut self) -> Result<(), PerfError> {
        self.command("enable")
    }

    /// Stops counting, and returns once perf has said so.
    ///
    /// # Errors
    ///
    /// If perf doesn't acknowledge.
    pub fn disable(&mut self) -> Result<(), PerfError> {
        self.command("disable")
    }

    fn command(&mut self, word: &str) -> Result<(), PerfError> {
        let ctl = self
            .ctl
            .as_mut()
            .ok_or_else(|| PerfError::Control("not connected".to_owned()))?;
        ctl.write_all(format!("{word}\n").as_bytes())
            .map_err(|error| PerfError::Control(format!("can't send {word:?}: {error}")))?;
        self.wait_for_ack(word)
    }

    fn wait_for_ack(&mut self, word: &str) -> Result<(), PerfError> {
        let deadline = Instant::now() + self.timeout;
        let mut chunk = [0_u8; 64];
        loop {
            if let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=end).collect();
                if line.starts_with(b"ack") {
                    return Ok(());
                }
                return Err(PerfError::Control(format!(
                    "{:?} answered {word:?}",
                    String::from_utf8_lossy(&line).trim()
                )));
            }
            // Nothing yet reads as "would block" with a writer, and as end of file before perf
            // has the FIFO open: both mean to wait.
            match self.ack.read(&mut chunk) {
                // perf writes its "ack\n" with the terminating NUL of the C string.
                Ok(count) => self
                    .pending
                    .extend(chunk[..count].iter().filter(|byte| **byte != 0)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    return Err(PerfError::Control(format!(
                        "can't read the answer: {error}"
                    )));
                }
            }
            if Instant::now() >= deadline {
                return Err(PerfError::Control(format!(
                    "no answer to {word:?} within {:?}",
                    self.timeout
                )));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

/// What perf reported for one event.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    Counted {
        value: f64,
        unit: String,
        /// How long the counter ran, in ns, and what share of the enabled time that was. Less
        /// than 100 means perf multiplexed the counter and the value is scaled.
        running_ns: f64,
        running_pct: f64,
    },
    /// The CPU or the kernel has no such event (`<not supported>`).
    NotSupported,
    /// It was never on (`<not counted>`).
    NotCounted,
    /// perf's output has no line for it.
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Counter {
    pub event: String,
    pub reading: Reading,
}

impl Counter {
    #[must_use]
    pub fn to_json(&self) -> Value {
        match &self.reading {
            Reading::Counted {
                value,
                unit,
                running_ns,
                running_pct,
            } => json!({
                "event": self.event, "state": "counted", "value": value, "unit": unit,
                "running_ns": running_ns, "running_pct": running_pct,
            }),
            Reading::NotSupported => json!({ "event": self.event, "state": "not_supported" }),
            Reading::NotCounted => json!({ "event": self.event, "state": "not_counted" }),
            Reading::Missing => json!({ "event": self.event, "state": "missing" }),
        }
    }
}

/// The event without its modifiers: perf writes `task-clock:u` for the `task-clock` it was given.
fn base(event: &str) -> &str {
    event.split(':').next().unwrap_or(event)
}

/// Reads the CSV of `perf stat -x,`: `value,unit,event,run time,percent running,...`, one line per
/// event, after `#` comments. Returns one counter for each of [`EVENTS`], in that order.
#[must_use]
pub fn parse_csv(text: &str) -> Vec<Counter> {
    EVENTS
        .iter()
        .map(|requested| {
            let reading = text
                .lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .map(|line| line.split(',').collect::<Vec<_>>())
                .find(|fields| {
                    fields
                        .get(2)
                        .is_some_and(|name| base(name) == base(requested))
                })
                .map_or(Reading::Missing, |fields| reading(&fields));
            Counter {
                event: (*requested).to_owned(),
                reading,
            }
        })
        .collect()
}

fn reading(fields: &[&str]) -> Reading {
    let value = fields.first().map_or("", |value| value.trim());
    match value {
        "<not supported>" => return Reading::NotSupported,
        "<not counted>" => return Reading::NotCounted,
        _ => {}
    }
    let Ok(parsed) = value.parse::<f64>() else {
        return Reading::Missing;
    };
    let number = |index: usize| {
        fields
            .get(index)
            .and_then(|field| field.trim().parse::<f64>().ok())
    };
    let unit = match fields.get(1).map(|unit| unit.trim()) {
        Some(unit) if !unit.is_empty() => unit.to_owned(),
        // perf writes the task clock in ns and no unit.
        _ if base(fields.get(2).copied().unwrap_or_default()) == "task-clock" => "ns".to_owned(),
        _ => "count".to_owned(),
    };
    match (number(3), number(4)) {
        (Some(running_ns), Some(running_pct)) => Reading::Counted {
            value: parsed,
            unit,
            running_ns,
            running_pct,
        },
        _ => Reading::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# started on Mon Oct 26 23:40:00 2026

1123974438,,instructions:u,44863879,100.00,4.59,insn per cycle
8057,,branch-misses:u,44863879,100.00,,
<not supported>,,ref-cycles:u,0,100.00,,
245076014,,cycles:u,44863879,52.50,5.463,GHz
44863879,,task-clock:u,44863879,100.00,0.992,CPUs utilized
";

    // INFRA-007
    #[test]
    fn reads_perfs_csv() {
        let counters = parse_csv(SAMPLE);
        let events: Vec<_> = counters.iter().map(|c| c.event.as_str()).collect();
        assert_eq!(events, EVENTS);
        assert_eq!(
            counters[0].reading,
            Reading::Counted {
                value: 1_123_974_438.0,
                unit: "count".into(),
                running_ns: 44_863_879.0,
                running_pct: 100.0
            }
        );
        // Multiplexed: the share is kept.
        assert!(matches!(
            counters[3].reading,
            Reading::Counted { running_pct, .. } if (running_pct - 52.5).abs() < 1e-9
        ));
        // `task-clock` matches the `task-clock:u` perf wrote, in ns.
        assert!(matches!(
            &counters[4].reading,
            Reading::Counted { unit, .. } if unit == "ns"
        ));
    }

    // INFRA-007
    #[test]
    fn an_unsupported_event_is_not_a_zero() {
        let counters = parse_csv(SAMPLE);
        assert_eq!(counters[2].reading, Reading::NotSupported);
        assert_eq!(
            counters[2].to_json(),
            json!({ "event": "ref-cycles:u", "state": "not_supported" })
        );
    }

    // INFRA-007
    #[test]
    fn not_counted_and_missing_events_are_told_apart() {
        let counters = parse_csv("<not counted>,,instructions:u,0,100.00,,\n");
        assert_eq!(counters[0].reading, Reading::NotCounted);
        assert_eq!(counters[1].reading, Reading::Missing);
        assert_eq!(parse_csv("").len(), EVENTS.len());
    }

    // INFRA-007
    #[test]
    fn the_counted_json_has_value_unit_and_shares() {
        let json = parse_csv(SAMPLE)[1].to_json();
        assert_eq!(json["state"], "counted");
        assert_eq!(json["value"], 8057.0);
        assert_eq!(json["unit"], "count");
        assert_eq!(json["running_pct"], 100.0);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rc_gui_perf-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // INFRA-007
    #[test]
    fn the_arguments_start_with_the_counters_off_and_name_the_fifos() {
        let dir = temp_dir("args");
        let control = Control::create(&dir).unwrap();
        let args = stat_args(&control, Path::new("/o.csv"));
        let args: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
        assert_eq!(&args[..4], ["stat", "-D", "-1", "--control"]);
        assert_eq!(
            args[4],
            format!("fifo:{}/ctl,{}/ack", dir.display(), dir.display())
        );
        assert_eq!(args.last().map(|arg| &**arg), Some("--"));
        assert!(args.iter().any(|arg| arg == "instructions:u,branch-misses:u,ref-cycles:u,cycles:u,task-clock"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // INFRA-008
    #[test]
    fn the_record_arguments_start_sampling_off_and_name_the_fifos_and_the_file() {
        let dir = temp_dir("record_args");
        let control = Control::create(&dir).unwrap();
        let args = record_args(&control, Path::new("/o.data"), Frontend::Slint);
        let args: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
        assert_eq!(&args[..4], ["record", "-D", "-1", "--control"]);
        assert_eq!(
            args[4],
            format!("fifo:{}/ctl,{}/ack", dir.display(), dir.display())
        );
        assert_eq!(
            &args[5..],
            [
                "-e",
                "cycles:u",
                "--call-graph",
                "fp",
                "-F",
                &RECORD_FREQUENCY.to_string(),
                "-o",
                "/o.data",
                "--"
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // INFRA-008
    #[test]
    fn the_qt_app_unwinds_with_dwarf_and_the_slint_app_with_frame_pointers() {
        assert_eq!(call_graph(Frontend::Qt), "dwarf,65528");
        assert_eq!(call_graph(Frontend::Slint), "fp");
        let dir = temp_dir("record_args_qt");
        let control = Control::create(&dir).unwrap();
        let args = record_args(&control, Path::new("/o.data"), Frontend::Qt);
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--call-graph" && pair[1] == "dwarf,65528")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stands in for perf: opens the FIFOs the way perf does (the command one read-write, the
    /// acknowledgement one for writing, which blocks until the harness reads) and answers `count`
    /// commands. Returns them.
    fn fake_perf(
        dir: &Path,
        count: usize,
        answer: &'static str,
    ) -> thread::JoinHandle<Vec<String>> {
        let (ctl, ack) = (dir.join("ctl"), dir.join("ack"));
        thread::spawn(move || {
            let ctl = OpenOptions::new().read(true).write(true).open(ctl).unwrap();
            let mut ack = OpenOptions::new().write(true).open(ack).unwrap();
            let mut lines = io::BufRead::lines(io::BufReader::new(ctl));
            let mut seen = Vec::new();
            for _ in 0..count {
                seen.push(lines.next().unwrap().unwrap());
                ack.write_all(answer.as_bytes()).unwrap();
            }
            seen
        })
    }

    // INFRA-007
    #[test]
    fn enable_and_disable_wait_for_the_acknowledgement() {
        let dir = temp_dir("fifo");
        let mut control = Control::create(&dir).unwrap();
        let perf = fake_perf(&dir, 2, "ack\n\0");
        control.connect().unwrap();
        control.enable().unwrap();
        control.disable().unwrap();
        assert_eq!(perf.join().unwrap(), ["enable", "disable"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // INFRA-007
    #[test]
    fn a_perf_that_never_answers_is_an_error() {
        let dir = temp_dir("silent");
        let mut control = Control::create(&dir)
            .unwrap()
            .with_timeout(Duration::from_millis(300));
        // Reads the command and never answers.
        let perf = fake_perf(&dir, 1, "");
        control.connect().unwrap();
        let error = control.enable().unwrap_err();
        assert!(error.to_string().contains("no answer"), "{error}");
        perf.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // INFRA-007
    #[test]
    fn a_missing_perf_is_an_error_when_connecting() {
        let dir = temp_dir("absent");
        let mut control = Control::create(&dir)
            .unwrap()
            .with_timeout(Duration::from_millis(100));
        let error = control.connect().unwrap_err();
        assert!(error.to_string().contains("no reader"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
