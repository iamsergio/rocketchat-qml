// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The numbers of the HTML report: medians, spreads, ratios between the apps and changes against
//! a baseline. Plain functions on plain data, so that they can be tested without HTML or files.

use serde_json::Value;

/// One counter of one run, as the JSON has it.
#[derive(Clone, Debug, PartialEq)]
pub struct CounterValue {
    pub event: String,
    /// `counted`, `not_supported`, `not_counted` or `missing`.
    pub state: String,
    pub value: Option<f64>,
    pub unit: String,
}

/// One run, as far as the report needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub app: String,
    pub scenario: String,
    pub platform: String,
    pub smoke: bool,
    pub git_revision: String,
    pub window: Option<(f64, f64)>,
    pub distance_px: f64,
    pub frames: Option<f64>,
    /// What was sent: the wheel scenarios have batches, the drag ones strokes.
    pub wheel_batches: Option<f64>,
    pub drag_strokes: Option<f64>,
    pub wall_time_ms: f64,
    pub counters: Vec<CounterValue>,
}

impl Run {
    /// Reads the JSON of a run (schema 1, see docs/design/performance-tests.md).
    ///
    /// # Errors
    ///
    /// Says which field is missing or has the wrong type.
    pub fn from_json(json: &Value) -> Result<Self, String> {
        let text = |key: &str| {
            json[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{key}` is missing"))
        };
        let number = |key: &str| {
            json[key]
                .as_f64()
                .ok_or_else(|| format!("`{key}` is missing"))
        };
        if json["schema"] != 1 {
            return Err("not schema 1".to_owned());
        }
        let window = match (
            json["window"]["width"].as_f64(),
            json["window"]["height"].as_f64(),
        ) {
            (Some(width), Some(height)) => Some((width, height)),
            _ => None,
        };
        let counters = json["counters"]
            .as_array()
            .ok_or("`counters` is missing")?
            .iter()
            .map(|counter| CounterValue {
                event: counter["event"].as_str().unwrap_or_default().to_owned(),
                state: counter["state"].as_str().unwrap_or("missing").to_owned(),
                value: counter["value"].as_f64(),
                unit: counter["unit"].as_str().unwrap_or_default().to_owned(),
            })
            .collect();
        Ok(Self {
            app: text("app")?,
            scenario: text("scenario")?,
            platform: text("platform")?,
            smoke: json["smoke"].as_bool().unwrap_or(false),
            git_revision: text("git_revision")?,
            window,
            distance_px: number("distance_px")?,
            frames: json["frames"].as_f64(),
            wheel_batches: json["wheel_batches"].as_f64(),
            drag_strokes: json["drag_strokes"].as_f64(),
            wall_time_ms: number("wall_time_ms")?,
            counters,
        })
    }
}

/// The middle of the runs, and how far they are apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stat {
    pub median: f64,
    /// (max - min) / median in %; none if the median is 0.
    pub spread_pct: Option<f64>,
}

/// The median and spread of the values; none without values.
#[must_use]
pub fn stat(values: &[f64]) -> Option<Stat> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(f64::total_cmp);
    let (min, max) = (*sorted.first()?, *sorted.last()?);
    let middle = sorted.len() / 2;
    let median = if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        f64::midpoint(sorted[middle - 1], sorted[middle])
    };
    Some(Stat {
        median,
        spread_pct: (median != 0.0).then(|| (max - min) / median.abs() * 100.0),
    })
}

/// What a counter says for a normalisation: a value, or why there is none.
#[derive(Clone, Debug, PartialEq)]
pub enum Measure {
    Counted(Stat),
    /// Not counted, not supported, missing, or (per frame) the app can't count frames.
    Not(&'static str),
}

impl Measure {
    #[must_use]
    pub fn stat(&self) -> Option<&Stat> {
        match self {
            Self::Counted(stat) => Some(stat),
            Self::Not(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CounterSummary {
    pub event: String,
    pub unit: String,
    pub per_pixel: Measure,
    pub per_frame: Measure,
}

/// Facts about the scroll or drag, each the median of the runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    pub distance_px: Option<Stat>,
    pub frames: Option<Stat>,
    pub px_per_frame: Option<Stat>,
    pub wall_time_ms: Option<Stat>,
    pub wheel_batches: Option<Stat>,
    pub drag_strokes: Option<Stat>,
}

/// All runs of one app in one scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct AppSummary {
    pub runs: usize,
    pub facts: Facts,
    pub counters: Vec<CounterSummary>,
}

fn why_not(state: &str) -> &'static str {
    match state {
        "not_supported" => "not supported",
        "not_counted" => "not counted",
        _ => "missing",
    }
}

/// The counters of the runs divided by the pixels scrolled and the frames drawn. A run that has
/// no value for an event, or no frames, is left out of that column; with no run left the column
/// says why.
#[must_use]
pub fn summarize(runs: &[Run]) -> AppSummary {
    let mut events: Vec<&str> = Vec::new();
    for counter in runs.iter().flat_map(|run| &run.counters) {
        if !events.contains(&counter.event.as_str()) {
            events.push(&counter.event);
        }
    }
    let counters = events
        .into_iter()
        .map(|event| {
            let found: Vec<(&Run, &CounterValue)> = runs
                .iter()
                .filter_map(|run| {
                    run.counters
                        .iter()
                        .find(|counter| counter.event == event)
                        .map(|counter| (run, counter))
                })
                .collect();
            let reason = found
                .iter()
                .find(|(_, counter)| counter.value.is_none())
                .map_or("missing", |(_, counter)| why_not(&counter.state));
            let measure = |per: fn(&Run) -> Option<f64>, none: &'static str| {
                let values: Vec<f64> = found
                    .iter()
                    .filter_map(|(run, counter)| {
                        let divisor = per(run).filter(|d| *d > 0.0)?;
                        counter.value.map(|value| value / divisor)
                    })
                    .collect();
                stat(&values).map_or(Measure::Not(none), Measure::Counted)
            };
            CounterSummary {
                event: event.to_owned(),
                unit: found
                    .iter()
                    .map(|(_, counter)| counter.unit.clone())
                    .find(|unit| !unit.is_empty())
                    .unwrap_or_default(),
                per_pixel: measure(|run| Some(run.distance_px), reason),
                per_frame: measure(
                    |run| run.frames,
                    if found.iter().any(|(_, counter)| counter.value.is_some()) {
                        "no frame count"
                    } else {
                        reason
                    },
                ),
            }
        })
        .collect();
    let fact =
        |get: fn(&Run) -> Option<f64>| stat(&runs.iter().filter_map(get).collect::<Vec<_>>());
    AppSummary {
        runs: runs.len(),
        facts: Facts {
            distance_px: fact(|run| Some(run.distance_px)),
            frames: fact(|run| run.frames),
            px_per_frame: fact(|run| run.frames.filter(|f| *f > 0.0).map(|f| run.distance_px / f)),
            wall_time_ms: fact(|run| Some(run.wall_time_ms)),
            wheel_batches: fact(|run| run.wheel_batches),
            drag_strokes: fact(|run| run.drag_strokes),
        },
        counters,
    }
}

/// Slint over Qt: below 1 the Slint app needs less. None if Qt's median is 0.
#[must_use]
pub fn ratio(slint: &Stat, qt: &Stat) -> Option<f64> {
    (qt.median != 0.0).then(|| slint.median / qt.median)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Better,
    Worse,
    WithinNoise,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub pct: f64,
    pub verdict: Verdict,
}

/// The change of a median against the baseline's in %. For these counters less is better. It is
/// better or worse only when it is larger than the two spreads added up (a missing spread counts
/// as 0), otherwise it is within noise. None if the baseline's median is 0.
#[must_use]
pub fn change(current: &Stat, baseline: &Stat) -> Option<Change> {
    if baseline.median == 0.0 {
        return None;
    }
    let pct = (current.median - baseline.median) / baseline.median.abs() * 100.0;
    let noise = current.spread_pct.unwrap_or(0.0) + baseline.spread_pct.unwrap_or(0.0);
    let verdict = if pct.abs() <= noise {
        Verdict::WithinNoise
    } else if pct < 0.0 {
        Verdict::Better
    } else {
        Verdict::Worse
    };
    Some(Change { pct, verdict })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter(event: &str, value: Option<f64>, state: &str) -> CounterValue {
        CounterValue {
            event: event.to_owned(),
            state: state.to_owned(),
            value,
            unit: "count".to_owned(),
        }
    }

    fn run(distance: f64, frames: Option<f64>, counters: Vec<CounterValue>) -> Run {
        Run {
            app: "qt".to_owned(),
            scenario: "s".to_owned(),
            platform: "xcb".to_owned(),
            smoke: false,
            git_revision: "abc".to_owned(),
            window: None,
            distance_px: distance,
            frames,
            wheel_batches: Some(10.0),
            drag_strokes: None,
            wall_time_ms: 1000.0,
            counters,
        }
    }

    fn stat_of(median: f64, spread: f64) -> Stat {
        Stat {
            median,
            spread_pct: Some(spread),
        }
    }

    // INFRA-007
    #[test]
    fn median_and_spread_of_odd_and_even_counts() {
        assert_eq!(stat(&[3.0, 1.0, 2.0]), Some(stat_of(2.0, 100.0)));
        let even = stat(&[10.0, 12.0, 11.0, 13.0]).unwrap_or_else(|| stat_of(0.0, 0.0));
        assert!((even.median - 11.5).abs() < 1e-9);
        assert!((even.spread_pct.unwrap_or(0.0) - 3.0 / 11.5 * 100.0).abs() < 1e-9);
        assert_eq!(stat(&[]), None);
        assert_eq!(stat(&[0.0, 0.0]).map(|s| s.spread_pct), Some(None));
    }

    // INFRA-007
    #[test]
    fn ratio_is_slint_over_qt_and_none_for_a_zero_qt() {
        assert_eq!(ratio(&stat_of(1.0, 0.0), &stat_of(4.0, 0.0)), Some(0.25));
        assert_eq!(ratio(&stat_of(1.0, 0.0), &stat_of(0.0, 0.0)), None);
    }

    // INFRA-007
    #[test]
    fn a_change_is_a_verdict_only_beyond_the_combined_spread() {
        let base = stat_of(100.0, 2.0);
        let worse = change(&stat_of(110.0, 3.0), &base);
        assert_eq!(worse.map(|c| c.verdict), Some(Verdict::Worse));
        assert!((worse.map_or(0.0, |c| c.pct) - 10.0).abs() < 1e-9);
        assert_eq!(
            change(&stat_of(90.0, 3.0), &base).map(|c| c.verdict),
            Some(Verdict::Better)
        );
        // 4% is not more than 2% + 3%.
        assert_eq!(
            change(&stat_of(104.0, 3.0), &base).map(|c| c.verdict),
            Some(Verdict::WithinNoise)
        );
        assert_eq!(change(&base, &stat_of(0.0, 0.0)), None);
    }

    // INFRA-007
    #[test]
    fn counters_are_divided_by_pixels_and_frames() {
        let runs = [
            run(
                100.0,
                Some(10.0),
                vec![counter("instructions:u", Some(1000.0), "counted")],
            ),
            run(
                200.0,
                Some(10.0),
                vec![counter("instructions:u", Some(4000.0), "counted")],
            ),
            run(
                100.0,
                Some(20.0),
                vec![counter("instructions:u", Some(3000.0), "counted")],
            ),
        ];
        let summary = summarize(&runs);
        assert_eq!(summary.runs, 3);
        // 10, 20 and 30 per px; 100, 400 and 150 per frame.
        let instructions = &summary.counters[0];
        assert_eq!(instructions.per_pixel.stat().map(|s| s.median), Some(20.0));
        assert_eq!(instructions.per_frame.stat().map(|s| s.median), Some(150.0));
        assert_eq!(summary.facts.px_per_frame.map(|s| s.median), Some(10.0));
    }

    // INFRA-007
    #[test]
    fn uncounted_counters_and_unknown_frames_are_no_numbers() {
        let runs = [run(
            100.0,
            None,
            vec![
                counter("instructions:u", Some(1000.0), "counted"),
                counter("ref-cycles:u", None, "not_supported"),
            ],
        )];
        let summary = summarize(&runs);
        assert_eq!(
            summary.counters[0].per_frame,
            Measure::Not("no frame count")
        );
        assert_eq!(summary.counters[1].per_pixel, Measure::Not("not supported"));
        assert_eq!(summary.counters[1].per_frame, Measure::Not("not supported"));
        assert_eq!(summary.facts.frames, None);
    }

    // INFRA-007
    #[test]
    fn a_run_is_read_from_its_json() {
        let json = serde_json::json!({
            "schema": 1, "app": "slint", "scenario": "s", "platform": "xcb", "smoke": true,
            "git_revision": "abc", "window": {"width": 964.0, "height": 600.0},
            "distance_px": 8000.0, "frames": null, "wheel_batches": 14, "wall_time_ms": 3000.5,
            "counters": [{"event": "instructions:u", "state": "counted", "value": 5.0,
                          "unit": "count"}, {"event": "cycles:u", "state": "not_counted"}],
        });
        let run = Run::from_json(&json).unwrap_or_else(|e| panic!("{e}"));
        assert!(run.smoke);
        assert_eq!(run.frames, None);
        assert_eq!(run.window, Some((964.0, 600.0)));
        assert_eq!(run.counters[1].value, None);
        assert_eq!(run.wheel_batches, Some(14.0));
        assert_eq!(run.drag_strokes, None);
        assert!(Run::from_json(&serde_json::json!({"schema": 2})).is_err());
    }
}
