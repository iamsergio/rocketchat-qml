// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The JSON file of one run.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::cli::{Job, Platform};
use crate::main_channel::{DISTANCE, Input, Measured};
use crate::perf::Counter;

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
    let mut json = json!({
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
        "wall_time_ms": measured.wall.as_secs_f64() * 1000.0,
        "counters": counters.iter().map(Counter::to_json).collect::<Vec<_>>(),
    });
    // What was sent depends on the scenario: a wheel scenario has the wheel fields (what older
    // files have), a drag scenario the stroke fields.
    let fields = match measured.input {
        Input::Wheel { batches, notches } => {
            json!({ "wheel_batches": batches, "wheel_notches": notches })
        }
        Input::Drag {
            strokes,
            length_px,
            steps,
            interval,
            hold,
        } => json!({
            "drag_strokes": strokes,
            "drag_length_px": length_px,
            "drag_steps": steps,
            "drag_interval_ms": interval.as_secs_f64() * 1000.0,
            "drag_hold_ms": hold.as_secs_f64() * 1000.0,
        }),
    };
    if let (Some(json), Value::Object(fields)) = (json.as_object_mut(), fields) {
        json.extend(fields);
    }
    json
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
            input: Input::Wheel {
                batches: 14,
                notches: 140,
            },
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
        assert_eq!(json["wheel_batches"], 14);
        assert_eq!(json["wheel_notches"], 140);
        assert_eq!(json["drag_strokes"], Value::Null);
    }

    // INFRA-007
    #[test]
    fn a_drag_run_has_the_stroke_fields_instead_of_the_wheel_ones() {
        let info = RunInfo {
            job: Job {
                frontend: Frontend::Qt,
                scenario: "drag_main_channel",
            },
            platform: Platform::Xcb,
            run: 0,
            binary: Path::new("/b/app"),
            git_revision: "abc123",
        };
        let measured = Measured {
            start_offset: 100.0,
            end_offset: 8200.0,
            frames: Some(900),
            input: Input::Drag {
                strokes: 23,
                length_px: 370.0,
                steps: 30,
                interval: Duration::from_millis(16),
                hold: Duration::from_millis(250),
            },
            wall: Duration::from_secs(20),
            window: None,
        };
        let json = to_json(&info, &measured, &[]);
        assert_eq!(json["scenario"], "drag_main_channel");
        assert_eq!(json["smoke"], false);
        assert_eq!(json["distance_px"], 8100.0);
        assert_eq!(json["frames"], 900);
        assert_eq!(json["drag_strokes"], 23);
        assert_eq!(json["drag_length_px"], 370.0);
        assert_eq!(json["drag_steps"], 30);
        assert_eq!(json["drag_interval_ms"], 16.0);
        assert_eq!(json["drag_hold_ms"], 250.0);
        assert_eq!(json["wheel_batches"], Value::Null);
        assert_eq!(json["wheel_notches"], Value::Null);
        // The report reads it, with the same schema as the wheel runs.
        let run = crate::aggregate::Run::from_json(&json);
        assert_eq!(run.map(|run| run.drag_strokes), Ok(Some(23.0)));
    }
}
