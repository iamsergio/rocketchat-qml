// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The JSON file of one run.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::cli::{Job, Platform};
use crate::perf::Counter;
use crate::scroll_main_channel::{DISTANCE, Measured};

/// What the file says about the run besides what was measured.
pub struct RunInfo<'a> {
    pub job: Job,
    pub platform: Platform,
    pub run: u32,
    pub binary: &'a Path,
    pub git_revision: &'a str,
}

/// `<app>.<scenario>.<run>.json`
#[must_use]
pub fn file_name(job: Job, run: u32) -> String {
    format!("{}.{run}.json", job.id())
}

#[must_use]
pub fn path(out: &Path, job: Job, run: u32) -> PathBuf {
    out.join(file_name(job, run))
}

/// The run as JSON. Distances are logical px, times ms, counters as perf reported them: one that
/// perf couldn't count has a `state` other than `counted` and no value.
#[must_use]
pub fn to_json(info: &RunInfo<'_>, measured: &Measured, counters: &[Counter]) -> Value {
    json!({
        "schema": 1,
        "app": info.job.frontend.name(),
        "scenario": info.job.scenario,
        "platform": info.platform.name(),
        // A smoke run only checked the code path: its numbers mean nothing.
        "smoke": info.platform.is_smoke(),
        "run": info.run,
        "binary": info.binary.display().to_string(),
        "git_revision": info.git_revision,
        "window": measured.window.map(|(width, height)| json!({ "width": width, "height": height })),
        "target_distance_px": DISTANCE,
        "distance_px": measured.end_offset - measured.start_offset,
        "frames": measured.frames,
        "wheel_batches": measured.batches,
        "wheel_notches": measured.notches,
        "wall_time_ms": measured.wall.as_secs_f64() * 1000.0,
        "counters": counters.iter().map(Counter::to_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::cli::Frontend;
    use crate::perf::parse_csv;

    fn job() -> Job {
        Job {
            frontend: Frontend::Slint,
            scenario: "scroll_main_channel",
        }
    }

    // INFRA-007
    #[test]
    fn one_file_per_run_named_after_app_scenario_and_run() {
        assert_eq!(file_name(job(), 3), "slint.scroll_main_channel.3.json");
        assert_eq!(
            path(Path::new("/o"), job(), 0),
            Path::new("/o/slint.scroll_main_channel.0.json")
        );
    }

    // INFRA-007
    #[test]
    fn the_json_has_the_measurement_and_marks_a_smoke_run() {
        let info = RunInfo {
            job: job(),
            platform: Platform::Offscreen,
            run: 1,
            binary: Path::new("/b/app"),
            git_revision: "abc123",
        };
        let measured = Measured {
            start_offset: 0.0,
            end_offset: 8100.0,
            frames: None,
            batches: 14,
            notches: 140,
            wall: Duration::from_millis(2500),
            window: Some((964.0, 600.0)),
        };
        let json = to_json(
            &info,
            &measured,
            &parse_csv("<not supported>,,instructions:u,0,100.00,,\n"),
        );
        assert_eq!(json["app"], "slint");
        assert_eq!(json["smoke"], true);
        assert_eq!(json["distance_px"], 8100.0);
        assert_eq!(json["frames"], Value::Null);
        assert_eq!(json["wall_time_ms"], 2500.0);
        assert_eq!(json["window"]["width"], 964.0);
        assert_eq!(json["counters"][0]["state"], "not_supported");
        assert_eq!(json["counters"].as_array().map(Vec::len), Some(5));
    }
}
