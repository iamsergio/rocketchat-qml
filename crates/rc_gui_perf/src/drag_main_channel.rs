// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `drag_main_channel`: drag the seeded channel's content down in strokes, as many as it takes to
//! drag [`DISTANCE`]. The list follows the pointer 1:1 in both apps, so both get the same input in
//! the same time and cover about the same distance, unlike with the wheel.

use std::thread;
use std::time::Duration;

use rc_gui_tests::Gui;
use rc_gui_tests::paths::main_window;

use crate::error::Error;
use crate::main_channel::{DISTANCE, Input, Measured, settle};
use crate::perf::Control;

/// What the list's top and bottom edge are kept clear of by a stroke, in logical px: where it
/// presses and where it lets go.
const MARGIN: f64 = 40.0;
/// Moves in a stroke, `INTERVAL` apart: a constant speed of `length / (STEPS * INTERVAL)`, about
/// 770 px/s in the 450 px list, the same in both apps.
const STEPS: u32 = 30;
const INTERVAL: Duration = Duration::from_millis(16);
/// Still at the end of a stroke before the release. Over 100 ms stops Slint from flicking on, and
/// 250 is what the GUI test uses.
const HOLD: Duration = Duration::from_millis(250);
/// Between two strokes, so that the app is at rest before the next press.
const PAUSE: Duration = Duration::from_millis(100);
/// A list shorter than this has no room for a stroke worth the name.
const LEAST_LENGTH: f64 = 100.0;

/// How far a stroke drags in a list of this height: from `MARGIN` below the top to `MARGIN` above
/// the bottom. None if that is shorter than [`LEAST_LENGTH`].
#[must_use]
pub fn stroke_length(list_height: f64) -> Option<f64> {
    let length = list_height - 2.0 * MARGIN;
    (length >= LEAST_LENGTH).then_some(length)
}

/// How many strokes of `length` drag [`DISTANCE`]. Both apps get this many, not as many as each
/// needs to move the list that far: Qt loses the first moves of a stroke before its list follows,
/// and would otherwise drag for longer. The distance each list moved is measured anyway.
#[must_use]
pub fn strokes_for(length: f64) -> u32 {
    // At most DISTANCE / LEAST_LENGTH, so it fits.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let strokes = (DISTANCE / length).ceil() as u32;
    strokes
}

/// The measured part: counters on, the strokes of [`strokes_for`], wait for the list to stop,
/// counters off.
///
/// # Errors
///
/// If perf or the app doesn't answer, or the list is too short for a stroke.
pub fn measure(gui: &Gui, control: &mut Control) -> Result<Measured, Error> {
    // Read before the counters: it is a Spix call.
    let height = gui.bounding_box(main_window::MESSAGE_LIST)?.height;
    let length = stroke_length(height).ok_or_else(|| {
        Error::Scenario(format!(
            "the list is {height:.0} px high: no room for a stroke"
        ))
    })?;
    crate::main_channel::measure(gui, control, |gui, _start| drag(gui, length))
}

fn drag(gui: &Gui, length: f64) -> Result<(f64, Input), Error> {
    let x = main_window::MESSAGE_LIST_DRAG_X;
    let strokes = strokes_for(length);
    for _ in 0..strokes {
        // Down: the content follows the pointer, which shows older messages, the way the wheel
        // up does.
        gui.mouse_drag(
            main_window::MESSAGE_LIST,
            (x, MARGIN),
            (x, MARGIN + length),
            STEPS,
            INTERVAL,
            HOLD,
        )?;
        // The Qt app only queues the stroke; this read goes through its queue and so returns after
        // the release, before the pause.
        gui.frame_count(main_window::WINDOW)?;
        thread::sleep(PAUSE);
    }
    let input = Input::Drag {
        strokes,
        length_px: length,
        steps: STEPS,
        interval: INTERVAL,
        hold: HOLD,
    };
    Ok((settle(gui)?, input))
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-007
    #[test]
    fn a_stroke_keeps_a_margin_at_both_ends_of_the_list() {
        assert_eq!(stroke_length(450.0), Some(370.0));
        assert_eq!(stroke_length(180.0), Some(100.0));
        assert_eq!(stroke_length(179.0), None);
        assert_eq!(stroke_length(0.0), None);
    }

    // INFRA-007
    #[test]
    fn both_apps_get_the_strokes_that_drag_the_distance() {
        assert_eq!(strokes_for(370.0), 22);
        assert_eq!(strokes_for(400.0), 20);
        assert!(f64::from(strokes_for(LEAST_LENGTH)) * LEAST_LENGTH >= DISTANCE);
    }
}
