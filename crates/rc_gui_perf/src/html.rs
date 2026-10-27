// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rc_gui_perf report`: reads the JSON files of a directory and writes one self-contained HTML
//! file (inline CSS, no script, no external resource).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::aggregate::{
    self, AppSummary, Change, CounterSummary, Measure, Run, Stat, Verdict, summarize,
};
use crate::cli::Variant;
use crate::error::Error;

/// The runs of a directory, and when the newest was written.
pub struct RunDir {
    pub runs: Vec<Run>,
    pub modified: Option<SystemTime>,
}

/// Reads every `*.json` file of the directory that is a run.
///
/// # Errors
///
/// If the directory can't be read, a file isn't a run, or there is none.
pub fn load(dir: &Path) -> Result<RunDir, Error> {
    let entries = fs::read_dir(dir).map_err(|source| Error::io("can't read", dir, source))?;
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    let mut runs = Vec::new();
    let mut modified: Option<SystemTime> = None;
    for path in paths {
        let text =
            fs::read_to_string(&path).map_err(|source| Error::io("can't read", &path, source))?;
        let bad = |what: String| Error::BadRun {
            path: path.display().to_string(),
            what,
        };
        let json: Value = serde_json::from_str(&text).map_err(|error| bad(error.to_string()))?;
        runs.push(Run::from_json(&json).map_err(bad)?);
        let time = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
        modified = modified.max(time);
    }
    if runs.is_empty() {
        return Err(Error::NoRuns(dir.to_path_buf()));
    }
    Ok(RunDir { runs, modified })
}

/// Writes the report of `dir`, against the runs of `baseline` if given.
///
/// # Errors
///
/// If a directory can't be read or the file can't be written.
pub fn write(dir: &Path, baseline: Option<&Path>, output: &Path) -> Result<(), Error> {
    let current = load(dir)?;
    let baseline = baseline.map(load).transpose()?;
    let date = current.modified.map_or_else(|| "unknown".to_owned(), utc);
    let html = render(
        &current.runs,
        baseline.as_ref().map(|dir| dir.runs.as_slice()),
        &date,
    );
    fs::write(output, html).map_err(|source| Error::io("can't write", output, source))
}

/// `2026-10-27 13:05 UTC`
fn utc(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (i64::try_from(secs / 86400).unwrap_or(0), secs % 86400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Three significant digits, thousands separated.
fn num(value: f64) -> String {
    let magnitude = value.abs();
    // Whole numbers, such as a median of frames, without a fraction.
    let text = if magnitude >= 100.0 || value.fract() == 0.0 {
        format!("{value:.0}")
    } else if magnitude >= 10.0 {
        format!("{value:.1}")
    } else if magnitude >= 1.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.3}")
    };
    let (integer, fraction) = text
        .split_once('.')
        .map_or((text.as_str(), ""), |(i, f)| (i, f));
    let (sign, digits) = integer
        .strip_prefix('-')
        .map_or(("", integer), |d| ("-", d));
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{fraction}")
    }
}

fn app_name(app: &str) -> String {
    Variant::from_stored(app).map_or_else(|| app.to_owned(), |variant| variant.label().to_owned())
}

/// The variants in the order of the report.
fn variant_ids() -> [&'static str; Variant::ALL.len()] {
    Variant::ALL.map(Variant::id)
}

const CSS: &str = "\
:root { color-scheme: light dark; --bg: #fafafa; --fg: #1d2127; --muted: #636b76;
  --line: #d5d9de; --panel: #ffffff; --bar: #4c7fc4; --good: #1b7f3b; --bad: #b3261e;
  --warn-bg: #fff1c2; --warn-fg: #4d3b00; }
@media (prefers-color-scheme: dark) {
  :root { --bg: #14171b; --fg: #e3e6ea; --muted: #99a1ac; --line: #343a42; --panel: #1b1f24;
    --bar: #6c9be0; --good: #5fcf80; --bad: #ff8a80; --warn-bg: #4a3b08; --warn-fg: #ffe9a3; } }
body { background: var(--bg); color: var(--fg); font: 14px/1.45 system-ui, sans-serif;
  margin: 0 auto; max-width: 62rem; padding: 1.5rem 1rem 3rem; }
h1 { font-size: 1.5rem; margin: 0 0 .25rem; }
h2 { font-size: 1.2rem; margin: 2.5rem 0 .5rem; border-bottom: 1px solid var(--line); }
h3 { font-size: 1rem; margin: 1.5rem 0 .4rem; }
.meta, .note { color: var(--muted); }
.meta { margin: 0 0 .75rem; }
.banner { background: var(--warn-bg); color: var(--warn-fg); border-radius: 6px;
  padding: .6rem .9rem; font-weight: 600; margin: .75rem 0; }
.table { overflow-x: auto; }
table { border-collapse: collapse; background: var(--panel); min-width: 100%; }
th, td { border: 1px solid var(--line); padding: .3rem .6rem; text-align: right;
  font-variant-numeric: tabular-nums; }
th:first-child, td:first-child, th.l, td.l { text-align: left; }
th { font-weight: 600; }
td.no { color: var(--muted); font-style: italic; }
td.better { color: var(--good); font-weight: 600; }
td.worse { color: var(--bad); font-weight: 600; }
td.noise { color: var(--muted); }
.track { position: relative; display: inline-block; width: 8rem; height: .6rem;
  background: var(--line); border-radius: 3px; vertical-align: middle; margin-left: .5rem; }
.bar { position: absolute; left: 0; top: 0; bottom: 0; background: var(--bar);
  border-radius: 3px; }
.one { position: absolute; top: -.2rem; bottom: -.2rem; width: 2px; background: var(--fg); }
details { background: var(--panel); border: 1px solid var(--line); border-radius: 6px;
  padding: .5rem .9rem; margin: 1rem 0; }
";

const HOW_TO_READ: &str = "\
<details open><summary>How to read this</summary>
<ul>
<li><b>Instructions</b> is the stable metric. Cycles, ref-cycles and task-clock depend on the
clock speed and on what else the machine does; look at their spread before trusting them.</li>
<li><b>scroll_main_channel</b>: the apps scroll differently, Qt glides after each wheel notch,
Slint jumps per notch. They are compared over the same distance, not the same input, and since
the number of frames differs a lot, both <b>per pixel scrolled</b> and <b>per frame drawn</b> are
shown. Per frame exists only where the app can count its frames.</li>
<li><b>drag_main_channel</b>: both apps get the same strokes in the same time and their lists
follow the pointer 1:1, so they cover about the same distance and on a real display draw about
the same number of frames. Here the numbers <b>per frame</b> compare directly (and so do wall
time and frames); per pixel is the same number divided by about the same distance.</li>
<li>Each number is the <b>median</b> of the runs; the <b>spread</b> is (max - min) / median.
Less is better for every counter.</li>
<li>The variants are the Qt app and the Slint app with each of its renderers. The ratio is
<b>each Slint variant / Qt</b>: below 1 Slint needs less. The software renderer has no rendering
notifier, so its frames are not counted, and a run that counted no frame while the list moved
(a rendering notifier that doesn't fire) counts as not counted too: the per frame numbers, and so
the ratios per frame, are unknown, not 0.</li>
<li>The change against a baseline is called better or worse only when it is larger than the two
spreads added up, and than 3%; otherwise it is within noise.</li>
<li>Qt's Spix has a 10 ms timer in the GUI thread, which wakes the Qt app 100 times a second
also when idle; that work is inside Qt's counters. Slint's Spix has no such timer.</li>
<li>A counter shown as <i>not counted</i> or <i>not supported</i> is unknown, not 0.</li>
</ul></details>
";

fn stat_cells(out: &mut String, measure: &Measure) {
    match measure {
        Measure::Counted(stat) => {
            let spread = stat
                .spread_pct
                .map_or_else(|| "-".to_owned(), |s| format!("{s:.1} %"));
            let _ = write!(out, "<td>{}</td><td>{spread}</td>", num(stat.median));
        }
        Measure::Not(why) => {
            let _ = write!(out, "<td class=\"no\" colspan=\"2\">{why}</td>");
        }
    }
}

fn fact_cell(stat: Option<&Stat>, suffix: &str) -> String {
    stat.map_or_else(
        || "<td class=\"no\">unknown</td>".to_owned(),
        |stat| format!("<td>{}{suffix}</td>", num(stat.median)),
    )
}

fn ratio_cell(slint: &Measure, qt: &Measure, scale: f64) -> String {
    let ratio = slint
        .stat()
        .zip(qt.stat())
        .and_then(|(slint, qt)| aggregate::ratio(slint, qt));
    let Some(ratio) = ratio else {
        return "<td class=\"no\">-</td>".to_owned();
    };
    let width = (ratio / scale * 100.0).clamp(0.0, 100.0);
    let one = 100.0 / scale;
    format!(
        "<td>{ratio:.2}<span class=\"track\"><span class=\"bar\" style=\"width:{width:.1}%\">\
         </span><span class=\"one\" style=\"left:{one:.1}%\"></span></span></td>"
    )
}

fn change_cell(current: &Measure, baseline: Option<&Measure>) -> String {
    let Some(current) = current.stat() else {
        return "<td class=\"no\">not counted</td>".to_owned();
    };
    let Some(change) = baseline
        .and_then(Measure::stat)
        .and_then(|baseline| aggregate::change(current, baseline))
    else {
        return "<td class=\"no\">no baseline</td>".to_owned();
    };
    let Change { pct, verdict } = change;
    let (class, text) = match verdict {
        Verdict::Better => ("better", "better"),
        Verdict::Worse => ("worse", "worse"),
        Verdict::WithinNoise => ("noise", "within noise"),
    };
    format!("<td class=\"{class}\">{pct:+.1} %, {text}</td>")
}

fn find<'a>(summary: Option<&'a AppSummary>, event: &str) -> Option<&'a CounterSummary> {
    summary?
        .counters
        .iter()
        .find(|counter| counter.event == event)
}

fn label(counter: &CounterSummary) -> String {
    if counter.unit.is_empty() || counter.unit == "count" {
        escape(&counter.event)
    } else {
        format!("{} ({})", escape(&counter.event), escape(&counter.unit))
    }
}

fn runs_of(runs: &[Run], app: &str, scenario: &str) -> Vec<Run> {
    runs.iter()
        .filter(|run| run.app == app && run.scenario == scenario)
        .cloned()
        .collect()
}

/// Entries of `values` once, in order.
fn unique<'a>(values: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

fn header(out: &mut String, runs: &[Run], date: &str, counts: &[(&str, usize)]) {
    let join = |values: Vec<&str>| escape(&values.join(", "));
    let mut sizes: Vec<String> = Vec::new();
    for (width, height) in runs.iter().filter_map(|run| run.window) {
        let size = format!("{width:.0} x {height:.0}");
        if !sizes.contains(&size) {
            sizes.push(size);
        }
    }
    let window = if sizes.is_empty() {
        "unknown".to_owned()
    } else {
        sizes.join(", ")
    };
    let runs_text = counts
        .iter()
        .map(|(app, n)| format!("{n} {}", app_name(app)))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "<p class=\"meta\">Date {} &middot; revision {} &middot; platform {} &middot; window {} px \
         &middot; runs {}</p>",
        escape(date),
        join(unique(runs.iter().map(|run| run.git_revision.as_str()))),
        join(unique(runs.iter().map(|run| run.platform.as_str()))),
        escape(&window),
        escape(&runs_text),
    );
    if runs.iter().any(|run| run.smoke) {
        out.push_str(
            "<div class=\"banner\">Smoke runs: nothing real was drawn, the numbers mean \
             nothing. They only show that the code path works.</div>\n",
        );
    }
}

fn facts_table(out: &mut String, apps: &[(&str, &AppSummary)]) {
    // What was sent: strokes for a drag scenario, wheel batches otherwise (and in old files).
    let drag = apps.iter().any(|(_, s)| s.facts.drag_strokes.is_some());
    let _ = writeln!(
        out,
        "<h3>The scroll</h3>\n<div class=\"table\"><table><tr><th>App</th><th>Distance</th>\
         <th>Frames</th><th>px per frame</th><th>Wall time</th><th>{}</th></tr>",
        if drag {
            "Drag strokes"
        } else {
            "Wheel batches"
        },
    );
    for (app, summary) in apps {
        let facts = &summary.facts;
        let _ = writeln!(
            out,
            "<tr><td>{}</td>{}{}{}{}{}</tr>",
            app_name(app),
            fact_cell(facts.distance_px.as_ref(), " px"),
            fact_cell(facts.frames.as_ref(), ""),
            fact_cell(facts.px_per_frame.as_ref(), ""),
            fact_cell(
                facts
                    .wall_time_ms
                    .as_ref()
                    .map(|s| Stat {
                        median: s.median / 1000.0,
                        ..*s
                    })
                    .as_ref(),
                " s"
            ),
            fact_cell(
                if drag {
                    facts.drag_strokes.as_ref()
                } else {
                    facts.wheel_batches.as_ref()
                },
                ""
            ),
        );
    }
    out.push_str("</table></div>\n");
}

fn counters_table(out: &mut String, apps: &[(&str, &AppSummary)]) {
    out.push_str(
        "<h3>Counters, median of the runs</h3>\n<div class=\"table\"><table><tr><th>Counter</th>\
         <th class=\"l\">App</th><th>per px</th><th>spread</th><th>per frame</th><th>spread</th></tr>\n",
    );
    let events = unique(
        apps.iter()
            .flat_map(|(_, s)| s.counters.iter().map(|c| c.event.as_str())),
    );
    for event in events {
        for (app, summary) in apps {
            let Some(counter) = find(Some(summary), event) else {
                continue;
            };
            let _ = write!(
                out,
                "<tr><td>{}</td><td class=\"l\">{}</td>",
                label(counter),
                app_name(app)
            );
            stat_cells(out, &counter.per_pixel);
            stat_cells(out, &counter.per_frame);
            out.push_str("</tr>\n");
        }
    }
    out.push_str("</table></div>\n");
}

fn ratio_table(out: &mut String, qt: &AppSummary, slint: &[(&str, &AppSummary)]) {
    let ratios: Vec<f64> = slint
        .iter()
        .flat_map(|(_, summary)| {
            qt.counters
                .iter()
                .filter_map(|q| find(Some(summary), &q.event).map(|s| (q, s)))
        })
        .flat_map(|(q, s)| [(&s.per_pixel, &q.per_pixel), (&s.per_frame, &q.per_frame)])
        .filter_map(|(s, q)| aggregate::ratio(s.stat()?, q.stat()?))
        .collect();
    let scale = ratios.iter().copied().fold(2.0, f64::max).ceil();
    out.push_str(
        "<h3>Slint / Qt</h3>\n<p class=\"note\">Below 1 Slint needs less; the line in each bar \
         marks 1.</p>\n<div class=\"table\"><table><tr><th rowspan=\"2\">Counter</th>",
    );
    for (app, _) in slint {
        let _ = write!(out, "<th colspan=\"2\">{}</th>", app_name(app));
    }
    out.push_str("</tr>\n<tr>");
    for _ in slint {
        out.push_str("<th>per px</th><th>per frame</th>");
    }
    out.push_str("</tr>\n");
    for q in &qt.counters {
        let _ = write!(out, "<tr><td>{}</td>", label(q));
        for (_, summary) in slint {
            match find(Some(summary), &q.event) {
                Some(s) => {
                    out.push_str(&ratio_cell(&s.per_pixel, &q.per_pixel, scale));
                    out.push_str(&ratio_cell(&s.per_frame, &q.per_frame, scale));
                }
                None => out.push_str("<td class=\"no\">-</td><td class=\"no\">-</td>"),
            }
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table></div>\n");
}

fn baseline_table(
    out: &mut String,
    apps: &[(&str, &AppSummary)],
    baseline: &[(&str, Option<AppSummary>)],
) {
    out.push_str(
        "<h3>Change against the baseline</h3>\n<div class=\"table\"><table><tr><th>Counter</th>\
         <th class=\"l\">App</th><th>per px</th><th>per frame</th></tr>\n",
    );
    let events = unique(
        apps.iter()
            .flat_map(|(_, s)| s.counters.iter().map(|c| c.event.as_str())),
    );
    for event in events {
        for (app, summary) in apps {
            let Some(counter) = find(Some(summary), event) else {
                continue;
            };
            let old = baseline
                .iter()
                .find(|(name, _)| name == app)
                .and_then(|(_, old)| find(old.as_ref(), event));
            let _ = writeln!(
                out,
                "<tr><td>{}</td><td class=\"l\">{}</td>{}{}</tr>",
                label(counter),
                app_name(app),
                change_cell(&counter.per_pixel, old.map(|c| &c.per_pixel)),
                change_cell(&counter.per_frame, old.map(|c| &c.per_frame)),
            );
        }
    }
    out.push_str("</table></div>\n");
}

/// The whole page. `date` is when the runs were made.
#[must_use]
pub fn render(runs: &[Run], baseline: Option<&[Run]>, date: &str) -> String {
    let mut out = String::new();
    out.push_str(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Performance report</title>\n<style>",
    );
    out.push_str(CSS);
    out.push_str("</style></head><body>\n<h1>Performance report</h1>\n");
    out.push_str(HOW_TO_READ);
    for scenario in unique(runs.iter().map(|run| run.scenario.as_str())) {
        let in_scenario: Vec<Run> = runs
            .iter()
            .filter(|r| r.scenario == scenario)
            .cloned()
            .collect();
        let _ = writeln!(out, "<h2>{}</h2>", escape(scenario));
        let summaries: Vec<(&str, AppSummary)> = variant_ids()
            .into_iter()
            .filter_map(|app| {
                let app_runs = runs_of(runs, app, scenario);
                (!app_runs.is_empty()).then(|| (app, summarize(&app_runs)))
            })
            .collect();
        let counts: Vec<(&str, usize)> = summaries.iter().map(|(app, s)| (*app, s.runs)).collect();
        header(&mut out, &in_scenario, date, &counts);
        let apps: Vec<(&str, &AppSummary)> = summaries.iter().map(|(app, s)| (*app, s)).collect();
        facts_table(&mut out, &apps);
        counters_table(&mut out, &apps);
        let slint: Vec<(&str, &AppSummary)> = apps
            .iter()
            .copied()
            .filter(|(app, _)| *app != "qt")
            .collect();
        if let Some((_, qt)) = apps.iter().find(|(app, _)| *app == "qt")
            && !slint.is_empty()
        {
            ratio_table(&mut out, qt, &slint);
        }
        if let Some(baseline) = baseline {
            let old: Vec<(&str, Option<AppSummary>)> = variant_ids()
                .into_iter()
                .map(|app| {
                    let app_runs = runs_of(baseline, app, scenario);
                    (app, (!app_runs.is_empty()).then(|| summarize(&app_runs)))
                })
                .collect();
            let revisions = unique(
                baseline
                    .iter()
                    .filter(|r| r.scenario == scenario)
                    .map(|r| r.git_revision.as_str()),
            );
            let _ = writeln!(
                out,
                "<p class=\"note\">Baseline: revision {}.</p>",
                escape(&revisions.join(", "))
            );
            baseline_table(&mut out, &apps, &old);
        }
    }
    out.push_str("</body></html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::CounterValue;

    fn run(app: &str, smoke: bool, instructions: f64, frames: Option<f64>) -> Run {
        Run {
            app: app.to_owned(),
            scenario: "scroll_main_channel".to_owned(),
            platform: if smoke { "offscreen" } else { "xcb" }.to_owned(),
            smoke,
            git_revision: "abc123".to_owned(),
            window: Some((964.0, 600.0)),
            distance_px: 1000.0,
            frames,
            wheel_batches: Some(14.0),
            drag_strokes: None,
            wall_time_ms: 3000.0,
            counters: vec![
                CounterValue {
                    event: "instructions:u".to_owned(),
                    state: "counted".to_owned(),
                    value: Some(instructions),
                    unit: "count".to_owned(),
                },
                CounterValue {
                    event: "ref-cycles:u".to_owned(),
                    state: "not_supported".to_owned(),
                    value: None,
                    unit: String::new(),
                },
            ],
        }
    }

    // INFRA-007
    #[test]
    fn the_report_has_the_values_the_ratio_and_the_baseline_change() {
        let now = [
            run("qt", false, 4e9, Some(400.0)),
            run("slint-femtovg", false, 1e9, None),
        ];
        let before = [run("qt", false, 2e9, Some(400.0))];
        let html = render(&now, Some(&before), "2026-10-27 13:05 UTC");
        assert!(html.contains("4,000,000"), "qt per px");
        assert!(html.contains("0.25"), "the ratio per px");
        assert!(html.contains("+100.0 %, worse"));
        assert!(html.contains("no baseline"), "slint has no baseline");
        assert!(html.contains("not supported"));
        assert!(html.contains("no frame count"));
        assert!(html.contains("2026-10-27 13:05 UTC"));
        assert!(html.contains("964 x 600"));
        assert!(!html.contains("Smoke runs"));
        assert!(!html.contains("<script"));
    }

    // INFRA-007
    #[test]
    fn three_variants_side_by_side_each_slint_one_against_qt() {
        let mut software = run("slint-software", false, 2e9, None);
        software.counters[0].value = Some(2e9);
        let runs = [
            run("qt", false, 4e9, Some(400.0)),
            run("slint-femtovg", false, 1e9, Some(500.0)),
            software,
        ];
        // The baseline has the file of before the variants, which said `slint`.
        let old = [Run::from_json(&serde_json::json!({
            "schema": 1, "app": "slint", "scenario": "scroll_main_channel", "platform": "xcb",
            "git_revision": "abc123", "distance_px": 1000.0, "frames": 500.0,
            "wall_time_ms": 3000.0,
            "counters": [{"event": "instructions:u", "state": "counted", "value": 2e9,
                          "unit": "count"}],
        }))
        .unwrap_or_else(|e| panic!("{e}"))];
        let html = render(&runs, Some(&old), "d");
        for name in ["Qt", "Slint femtovg", "Slint software"] {
            assert!(
                html.contains(&format!("<td>{name}</td>")),
                "{name} in the facts"
            );
            assert!(html.contains(&format!("<th colspan=\"2\">{name}</th>")) || name == "Qt");
        }
        // 1e9 / 4e9 and 2e9 / 4e9 per px, in one row of the ratio table.
        assert!(html.contains("0.25") && html.contains("0.50"));
        // Frames are known for two variants and unknown, not 0, for the software one.
        assert!(html.contains("no frame count"));
        assert!(html.contains("<td class=\"no\">unknown</td>"));
        // `slint` of the baseline is the femtovg variant: -50 %, and software has none.
        assert!(html.contains("-50.0 %, better"));
        assert!(html.contains("no baseline"));
    }

    // INFRA-007
    #[test]
    fn a_drag_scenario_shows_its_strokes_and_a_wheel_one_its_batches() {
        let mut drag = run("qt", false, 4e9, Some(400.0));
        drag.scenario = "drag_main_channel".to_owned();
        drag.wheel_batches = None;
        drag.drag_strokes = Some(23.0);
        let html = render(
            &[drag, run("slint-femtovg", false, 1e9, Some(400.0))],
            None,
            "d",
        );
        assert!(html.contains("<th>Drag strokes</th>"));
        assert!(html.contains("<th>Wheel batches</th>"));
        assert!(html.contains("drag_main_channel"));
        assert!(html.contains("compare directly"));
    }

    // INFRA-007
    #[test]
    fn smoke_runs_get_the_banner() {
        let html = render(&[run("qt", true, 4e9, None)], None, "d");
        assert!(html.contains("numbers mean"));
        assert!(!html.contains("Change against the baseline"));
    }

    // INFRA-007
    #[test]
    fn numbers_and_dates_are_formatted() {
        assert_eq!(num(1_234_567.4), "1,234,567");
        assert_eq!(num(12.345), "12.3");
        assert_eq!(num(0.12345), "0.123");
        assert_eq!(num(-1500.0), "-1,500");
        assert_eq!(num(85.0), "85");
        assert_eq!(
            utc(UNIX_EPOCH + std::time::Duration::from_mins(29_885_105)),
            "2026-10-27 13:05 UTC"
        );
    }
}
