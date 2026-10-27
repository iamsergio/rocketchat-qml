// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! From a `perf record` file to collapsed stacks and a flame graph SVG: `perf script` piped into
//! inferno's perf collapser, then inferno's flame graph.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inferno::collapse::Collapse;
use inferno::collapse::perf::{Folder, Options as FoldOptions};
use inferno::flamegraph::{self, Options as SvgOptions};

#[derive(Debug, thiserror::Error)]
pub enum FlamegraphError {
    #[error("`perf script` can't be run ({0})")]
    Script(#[source] io::Error),
    #[error("`perf script` failed: {0}")]
    ScriptFailed(String),
    #[error("can't collapse the stacks of perf script")]
    Fold(#[source] io::Error),
    #[error("can't draw the flame graph")]
    Svg(#[source] io::Error),
    #[error("no samples in {}: the measured part was never switched on, or too short", .0.display())]
    NoSamples(PathBuf),
    #[error("can't write {}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Collapses the text of `perf script` into one `a;b;c count` line per distinct stack. The
/// process id and the thread name stay out of the stacks: the leaf-most frames are what a
/// comparison wants, and the thread name is the first frame anyway.
///
/// # Errors
///
/// If reading or writing fails.
pub fn fold(script: impl Read, folded: impl Write) -> Result<(), FlamegraphError> {
    let mut collapser = Folder::from(FoldOptions::default());
    collapser
        .collapse(BufReader::new(script), folded)
        .map_err(FlamegraphError::Fold)
}

/// Draws collapsed stacks as an SVG. `count_name` names what a sample counts, `subtitle` goes
/// under the title.
///
/// # Errors
///
/// If the stacks can't be read or the SVG can't be written.
pub fn svg(
    folded: impl Read,
    svg: impl Write,
    title: &str,
    subtitle: &str,
    count_name: &str,
) -> Result<(), FlamegraphError> {
    let mut options = SvgOptions::default();
    title.clone_into(&mut options.title);
    options.subtitle = Some(subtitle.to_owned());
    count_name.clone_into(&mut options.count_name);
    flamegraph::from_reader(&mut options, BufReader::new(folded), svg).map_err(FlamegraphError::Svg)
}

/// What [`convert`] wrote.
pub struct Written {
    pub folded: PathBuf,
    pub svg: PathBuf,
    /// The sum of the counts of the collapsed stacks. perf script gives each sample the cycles it
    /// stands for, so this is the cycles of the measured part, not the number of samples.
    pub cycles: u64,
}

/// Runs `perf script` on `perf_data`, writes the collapsed stacks to `folded` and the flame graph
/// to `svg`.
///
/// # Errors
///
/// If `perf script` fails, has no samples, or a file can't be written.
pub fn convert(
    perf_data: &Path,
    folded: &Path,
    svg_path: &Path,
    title: &str,
    subtitle: &str,
    count_name: &str,
) -> Result<Written, FlamegraphError> {
    let mut child = Command::new("perf")
        .arg("script")
        .arg("-i")
        .arg(perf_data)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(FlamegraphError::Script)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| FlamegraphError::ScriptFailed("no standard output".to_owned()))?;
    // perf script's complaints about missing symbols can be many: read them on a thread so the
    // pipe never fills up while the output is being folded.
    let stderr = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    });
    let write_error = |path: &Path| {
        let path = path.to_path_buf();
        move |source| FlamegraphError::Write { path, source }
    };
    let file = File::create(folded).map_err(write_error(folded))?;
    let mut out = BufWriter::new(file);
    let folded_result = fold(stdout, &mut out);
    out.flush().map_err(write_error(folded))?;
    let status = child.wait().map_err(FlamegraphError::Script)?;
    let errors = errors.join().unwrap_or_default();
    if !status.success() {
        let last: Vec<&str> = errors.lines().rev().take(3).collect();
        return Err(FlamegraphError::ScriptFailed(format!(
            "{status}: {}",
            last.into_iter().rev().collect::<Vec<_>>().join(" / ")
        )));
    }
    folded_result?;

    let cycles = count_cycles(folded).map_err(write_error(folded))?;
    if cycles == 0 {
        return Err(FlamegraphError::NoSamples(perf_data.to_path_buf()));
    }
    let input = File::open(folded).map_err(write_error(folded))?;
    let output = File::create(svg_path).map_err(write_error(svg_path))?;
    let mut output = BufWriter::new(output);
    self::svg(input, &mut output, title, subtitle, count_name)?;
    output.flush().map_err(write_error(svg_path))?;
    Ok(Written {
        folded: folded.to_path_buf(),
        svg: svg_path.to_path_buf(),
        cycles,
    })
}

/// The sum of the counts of collapsed lines (`stack count`).
fn count_cycles(folded: &Path) -> io::Result<u64> {
    let mut text = String::new();
    File::open(folded)?.read_to_string(&mut text)?;
    Ok(sum_counts(&text))
}

fn sum_counts(folded: &str) -> u64 {
    folded
        .lines()
        .filter_map(|line| line.rsplit_once(' '))
        .filter_map(|(_, count)| count.parse::<u64>().ok())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four samples of a real `perf script` of the Slint app (frames trimmed, paths made
    /// generic): the first two in the Spix accept loop, the other two the same malloc stack.
    const SCRIPT: &str = include_str!("testdata/perf_script.txt");

    fn folded() -> String {
        let mut out = Vec::new();
        fold(SCRIPT.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    // INFRA-008
    #[test]
    fn the_fixture_has_no_path_of_a_home_directory() {
        assert!(!SCRIPT.contains("/home/"));
        assert!(!SCRIPT.contains("/pub_data"));
    }

    // INFRA-008
    #[test]
    fn stacks_fold_to_one_line_per_distinct_stack_weighted_by_cycles() {
        let folded = folded();
        let lines: Vec<&str> = folded.lines().collect();
        // Two accept stacks, and the malloc stack once, counted twice (78 cycles each).
        assert_eq!(lines.len(), 3, "{folded}");
        let malloc = lines.iter().find(|line| line.contains("malloc")).unwrap();
        assert!(malloc.starts_with("spix;"), "{malloc}");
        assert!(malloc.ends_with(";malloc 156"), "{malloc}");
        assert!(malloc.contains("alloc::raw_vec::RawVecInner"), "{malloc}");
        // The root first, the sampled function last, inlined frames included.
        assert!(lines.contains(
            &"spix;spix_slint::server::serve;<std::net::tcp::Incoming as \
              core::iter::traits::iterator::Iterator>::next;<std::net::tcp::TcpListener>::accept;\
              <std::sys::net::connection::socket::TcpListener>::accept 1"
        ));
        assert!(
            lines
                .iter()
                .any(|line| line.contains(";accept4 ") || line.ends_with(";accept4 1"))
        );
        assert_eq!(sum_counts(&folded), 1 + 1 + 156);
    }

    // INFRA-008
    #[test]
    fn the_flame_graph_is_an_svg_with_the_frames_the_title_and_the_count_name() {
        let folded = folded();
        let mut out = Vec::new();
        svg(
            folded.as_bytes(),
            &mut out,
            "slint-femtovg.scroll_main_channel run 0",
            "cycles:u, 4000 Hz",
            "cycles",
        )
        .unwrap();
        let svg = String::from_utf8(out).unwrap();
        assert!(svg.starts_with("<?xml"), "{}", &svg[..60]);
        assert!(svg.contains("slint-femtovg.scroll_main_channel run 0"));
        assert!(svg.contains("cycles:u, 4000 Hz"));
        assert!(svg.contains("malloc"));
        assert!(svg.contains("spix_slint::server::serve"));
    }

    // INFRA-008: without perf installed (as on CI) the error is that perf can't be run.
    #[test]
    fn a_missing_recording_is_an_error_not_an_empty_graph() {
        let dir = std::env::temp_dir().join(format!("rc_gui_perf-fg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let result = convert(
            &dir.join("absent.data"),
            &dir.join("a.folded"),
            &dir.join("a.svg"),
            "t",
            "s",
            "cycles",
        );
        assert!(matches!(
            result,
            Err(FlamegraphError::Script(_)
                | FlamegraphError::ScriptFailed(_)
                | FlamegraphError::NoSamples(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
