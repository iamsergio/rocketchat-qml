// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `scroll_main_channel`: wheel up through the seeded channel over a fixed distance.

use rc_gui_tests::Gui;

use crate::error::Error;
use crate::main_channel::{
    DISTANCE, Input, MAX_BATCHES, Measured, NOTCH, NOTCHES, offset, settle, wheel,
};
use crate::perf::Control;

/// The measured part: counters on, wheel up in batches until the list has moved [`DISTANCE`], wait
/// for it to stop, counters off.
///
/// # Errors
///
/// If perf or the app doesn't answer, or the wheel doesn't move the list.
pub fn measure(gui: &Gui, control: &mut Control) -> Result<Measured, Error> {
    crate::main_channel::measure(gui, control, scroll)
}

fn scroll(gui: &Gui, start: f64) -> Result<(f64, Input), Error> {
    let mut batches = 0;
    let mut now = start;
    while now - start < DISTANCE {
        if batches >= MAX_BATCHES {
            return Err(Error::Scenario(format!(
                "{MAX_BATCHES} batches moved the list only {:.0} px",
                now - start
            )));
        }
        wheel(gui, NOTCH, NOTCHES)?;
        batches += 1;
        now = offset(gui)?;
    }
    let input = Input::Wheel {
        batches,
        notches: batches * NOTCHES,
    };
    Ok((settle(gui)?, input))
}
