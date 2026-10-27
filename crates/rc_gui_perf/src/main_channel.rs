// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the scenarios on the seeded channel share: getting the app to the channel, the offset
//! reads, the measured part's frame around the input, and what it reports.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rc_gui_tests::paths::{main_window, message, message_row, room, room_row};
use rc_gui_tests::{Gui, Key, Modifiers};

use crate::error::Error;
use crate::perf::Control;
use crate::seed::Channel;

/// How far the measured part scrolls, in logical px up from the newest message. The 200 seeded
/// messages are about 100 px each in both apps, some 20000 px in all, and [`prepare`] checks
/// that the list is longer than this plus a batch. 8000 px is about 13 screens of the 600 px
/// window and under half the list, so no run reaches the oldest page (which would load more of
/// the history inside the counters); it is 14 batches in Slint and some 26 in Qt, and takes
/// seconds.
pub const DISTANCE: f64 = 8000.0;
/// A batch is `NOTCHES` wheel notches `STEP_INTERVAL` apart: 600 px in Slint (60 px a notch), what
/// Qt makes of it otherwise. The offset is read after each batch, so a batch is the resolution
/// with which the distance is met, and the part of the next one the list overshoots by.
pub const NOTCHES: u32 = 10;
pub const NOTCH: i32 = 120;
/// Slint's 600 px for a batch, doubled for the part a batch can overshoot by.
const BATCH_PX: f64 = 1200.0;
pub const STEP_INTERVAL: Duration = Duration::from_millis(20);
/// The most batches before the scroll is given up: a wheel that does nothing must not spin.
pub const MAX_BATCHES: u32 = 200;
/// How often the offset is read while waiting for the list to stop. Every read is app work inside
/// the counters (three Spix calls), so it is slow: a few reads in a run.
const SETTLE_POLL: Duration = Duration::from_millis(250);
/// Two reads this close are the same position.
const STILL: f64 = 0.5;
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long the downloaded files must stay the same for the images and avatars to be in.
/// Time for a page of history to arrive between two presses of Home.
const HOME_PAUSE: Duration = Duration::from_millis(300);
const CACHE_QUIET: Duration = Duration::from_millis(1500);
const CACHE_POLL: Duration = Duration::from_millis(250);

/// The input a scenario played in the measured part, and the numbers that say how much of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Wheel {
        batches: u32,
        notches: u32,
    },
    Drag {
        strokes: u32,
        length_px: f64,
        steps: u32,
        interval: Duration,
        hold: Duration,
    },
}

/// What the measured part did.
#[derive(Debug)]
pub struct Measured {
    pub start_offset: f64,
    pub end_offset: f64,
    pub frames: Option<u64>,
    pub input: Input,
    pub wall: Duration,
    pub window: Option<(f64, f64)>,
}

pub fn offset(gui: &Gui) -> Result<f64, Error> {
    Ok(gui.scroll_offset(main_window::MESSAGE_LIST, main_window::MESSAGE_LIST_OFFSET)?)
}

pub fn wheel(gui: &Gui, notches: i32, steps: u32) -> Result<(), Error> {
    Ok(gui.mouse_wheel(main_window::MESSAGE_LIST, 0, notches, steps, STEP_INTERVAL)?)
}

/// Waits until the list has stopped moving: two reads `SETTLE_POLL` apart agree.
pub fn settle(gui: &Gui) -> Result<f64, Error> {
    let deadline = Instant::now() + TIMEOUT;
    let mut last = offset(gui)?;
    loop {
        thread::sleep(SETTLE_POLL);
        let now = offset(gui)?;
        if (now - last).abs() < STILL {
            return Ok(now);
        }
        if Instant::now() >= deadline {
            return Err(Error::Scenario("the list never stopped moving".to_owned()));
        }
        last = now;
    }
}

/// The number of files below `dir` and their bytes.
fn files_in(dir: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    entries
        .flatten()
        .fold((0, 0), |(count, bytes), entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => {
                let (inner_count, inner_bytes) = files_in(&entry.path());
                (count + inner_count, bytes + inner_bytes)
            }
            Ok(meta) => (count + 1, bytes + meta.len()),
            Err(_) => (count, bytes),
        })
}

/// Waits until the app has stopped downloading: the files in its cache are the same for a while,
/// and there are at least `least` of them (an image each; the avatars come on top).
fn wait_for_downloads(cache: &Path, least: usize) -> Result<(), Error> {
    let deadline = Instant::now() + TIMEOUT;
    let mut before = files_in(cache);
    let mut quiet_since = Instant::now();
    loop {
        thread::sleep(CACHE_POLL);
        let now = files_in(cache);
        if now != before {
            before = now;
            quiet_since = Instant::now();
        } else if quiet_since.elapsed() >= CACHE_QUIET && now.0 >= least {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Scenario(format!(
                "the app's cache holds {} files, not the {least} images, and is not growing",
                now.0
            )));
        }
    }
}

/// Everything before the counters: log in, open the channel, load all of it, come back to the
/// newest message. Leaves the list at rest at the bottom. Returns how far the list is long, in px.
///
/// # Errors
///
/// If the app doesn't show what the scenario expects, or the list is too short for [`DISTANCE`].
pub fn prepare(gui: &Gui, cache: &Path, server: &str, channel: &Channel) -> Result<f64, Error> {
    let timeout = TIMEOUT;
    gui.wait_until_visible(main_window::LOGIN_BUTTON, timeout)?;
    gui.set_text(main_window::LOGIN_SERVER_FIELD, server)?;
    gui.set_text(main_window::LOGIN_USERNAME_FIELD, &channel.viewer.username)?;
    gui.set_text(main_window::LOGIN_PASSWORD_FIELD, &channel.viewer.password)?;
    gui.mouse_click(main_window::LOGIN_BUTTON)?;
    gui.wait_until_visible(&room(&channel.id, room_row::NAME), timeout)?;
    gui.mouse_click(&room(&channel.id, room_row::NAME))?;
    gui.wait_until_visible(&message(&channel.newest_id, message_row::BODY), timeout)?;

    // Home goes to the top, which loads the next page of history above. The list keeps what was
    // in view, so the new page is above it: Home again, until the start of the conversation is
    // in view.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        gui.enter_key(main_window::MESSAGE_LIST, Key::HOME, Modifiers::NONE)?;
        thread::sleep(HOME_PAUSE);
        let loaded = gui.count(main_window::MESSAGE_LIST)?;
        if loaded >= channel.messages
            && gui.exists_and_visible(&message(&channel.oldest_id, message_row::BODY))?
            && gui.exists_and_visible(main_window::HISTORY_START)?
        {
            break;
        }
        if Instant::now() >= deadline {
            return Err(Error::Scenario(format!(
                "the start of the conversation never came in view: the list has {loaded} of {} \
                 messages",
                channel.messages
            )));
        }
    }
    let length = settle(gui)?;
    wait_for_downloads(cache, channel.images)?;

    // Back to the newest message: wheel down until the list stops moving. Not until the offset is
    // 0: the Qt list reports a position that is off by a constant once older messages are loaded
    // above the view (it keeps the rows in view in place), so only differences between reads are
    // distances, and the bottom is where the offset stops changing.
    let mut batches = 0;
    let mut before = offset(gui)?;
    let bottom = loop {
        if batches >= MAX_BATCHES {
            return Err(Error::Scenario(
                "the wheel never reached the newest message".to_owned(),
            ));
        }
        wheel(gui, -NOTCH, NOTCHES * 2)?;
        batches += 1;
        let now = offset(gui)?;
        if (now - before).abs() < STILL {
            break settle(gui)?;
        }
        before = now;
    };
    let length = length - bottom;
    if length < DISTANCE + BATCH_PX {
        return Err(Error::Scenario(format!(
            "the list is {length:.0} px long, shorter than the {DISTANCE} px of the scenario \
             plus a batch: the content is too short to compare"
        )));
    }
    Ok(length)
}

/// The measured part: counters on, `input` (which plays the scenario's input until the list has
/// moved [`DISTANCE`], waits for it to stop and returns the offset then and what it played),
/// counters off.
///
/// # Errors
///
/// If perf or the app doesn't answer, or `input` fails.
pub fn measure(
    gui: &Gui,
    control: &mut Control,
    input: impl FnOnce(&Gui, f64) -> Result<(f64, Input), Error>,
) -> Result<Measured, Error> {
    let window = gui
        .bounding_box(main_window::WINDOW)
        .ok()
        .map(|rect| (rect.width, rect.height));
    let start_offset = offset(gui)?;
    let frames_before = gui.frame_count(main_window::WINDOW)?;
    let started = Instant::now();
    control.enable()?;
    let result = input(gui, start_offset);
    // Off even after a failure, so that the counters don't run on while the app shuts down.
    let disabled = control.disable();
    let wall = started.elapsed();
    let (end_offset, input) = result?;
    disabled?;
    let frames_after = gui.frame_count(main_window::WINDOW)?;
    Ok(Measured {
        start_offset,
        end_offset,
        frames: frames_drawn(frames_before, frames_after, start_offset, end_offset),
        input,
        wall,
        window,
    })
}

/// The frames drawn between two reads of the counter, or none when the app can't count them. A
/// count that didn't move while the list did is not 0 frames but a counter that doesn't fire (Skia
/// on wgpu, so far), so it is unknown too.
fn frames_drawn(
    before: Option<u64>,
    after: Option<u64>,
    start_offset: f64,
    end_offset: f64,
) -> Option<u64> {
    let frames = after?.saturating_sub(before?);
    (frames > 0 || (end_offset - start_offset).abs() < f64::EPSILON).then_some(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-007
    #[test]
    fn no_frames_while_the_list_moved_are_unknown_and_not_zero() {
        assert_eq!(frames_drawn(Some(10), Some(50), 0.0, 8000.0), Some(40));
        assert_eq!(frames_drawn(Some(0), Some(0), 0.0, 8000.0), None);
        assert_eq!(frames_drawn(Some(7), Some(7), 100.0, 0.0), None);
        // Nothing moved and nothing was drawn: that is a real 0.
        assert_eq!(frames_drawn(Some(7), Some(7), 100.0, 100.0), Some(0));
        assert_eq!(frames_drawn(None, None, 0.0, 8000.0), None);
        assert_eq!(frames_drawn(Some(1), None, 0.0, 8000.0), None);
    }

    // INFRA-007
    #[test]
    fn counts_the_files_below_a_directory() {
        let dir = std::env::temp_dir().join(format!("rc_gui_perf-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        std::fs::write(dir.join("a"), b"12").unwrap();
        std::fs::write(dir.join("inner/b"), b"345").unwrap();
        assert_eq!(files_in(&dir), (2, 5));
        assert_eq!(files_in(&dir.join("missing")), (0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
