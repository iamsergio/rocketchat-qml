// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Client for the running app's Spix server, limited to the calls both frontends' servers support
//! (docs/design/gui-testing.md).

use std::cell::Cell;
use std::ops::BitOr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::xmlrpc::{self, Value};

/// How long one call may take, including any queued [`Gui::wait`].
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// How often [`Gui::wait_until`] looks again.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum GuiError {
    #[error("Spix call {method} failed")]
    Rpc {
        method: &'static str,
        #[source]
        source: xmlrpc::Error,
    },
    #[error("Spix reported: {}", .0.join("; "))]
    Spix(Vec<String>),
    #[error("Spix call {method} returned an unexpected value: {value:?}")]
    UnexpectedValue { method: &'static str, value: Value },
    #[error("can't read the screenshot {}", .path.display())]
    Screenshot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("timed out after {timeout:?} waiting for {what}{}", .last.as_ref().map_or_else(String::new, |last| format!(" ({last})")))]
    Timeout {
        what: String,
        timeout: Duration,
        /// What was seen last, if anything.
        last: Option<String>,
    },
}

pub type Result<T> = std::result::Result<T, GuiError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    fn spix_code(self) -> i32 {
        match self {
            Self::Left => 1,
            Self::Right => 2,
            Self::Middle => 4,
        }
    }
}

/// Keyboard modifiers, combined with `|`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers(i32);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1);
    pub const CONTROL: Self = Self(2);
    pub const ALT: Self = Self(4);
    pub const META: Self = Self(8);
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A key, by Spix key code (Qt's `Qt::Key` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key(i32);

impl Key {
    pub const ESCAPE: Self = Self(0x0100_0000);
    pub const TAB: Self = Self(0x0100_0001);
    pub const F5: Self = Self(0x0100_0034);
    pub const K: Self = Self(0x4B);
    pub const V: Self = Self(0x56);
    pub const BACKSPACE: Self = Self(0x0100_0003);
    pub const RETURN: Self = Self(0x0100_0004);
    pub const ENTER: Self = Self(0x0100_0005);
    pub const DELETE: Self = Self(0x0100_0007);
    pub const HOME: Self = Self(0x0100_0010);
    pub const END: Self = Self(0x0100_0011);
    pub const LEFT: Self = Self(0x0100_0012);
    pub const UP: Self = Self(0x0100_0013);
    pub const RIGHT: Self = Self(0x0100_0014);
    pub const DOWN: Self = Self(0x0100_0015);
    pub const PAGE_UP: Self = Self(0x0100_0016);
    pub const PAGE_DOWN: Self = Self(0x0100_0017);
}

/// An element's bounds in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Drives the app through its Spix server.
///
/// # Errors
///
/// Every call fails if the server can't be reached or answers with a fault, and if Spix reported
/// an error since the previous call (for example "item not found"), so a typo in a path can't go
/// unnoticed.
pub struct Gui {
    rpc: xmlrpc::Client,
    errors_seen: Cell<usize>,
}

#[expect(
    clippy::missing_errors_doc,
    reason = "every call fails the same way, as documented on the type"
)]
impl Gui {
    /// Connects to a Spix server on localhost. Nothing is sent until the first call.
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self {
            rpc: xmlrpc::Client::new(port, CALL_TIMEOUT),
            errors_seen: Cell::new(0),
        }
    }

    pub fn mouse_click(&self, path: &str) -> Result<()> {
        self.command("mouseClick", &[path.into()]).map(drop)
    }

    /// Double-clicks the left button at the item's centre.
    pub fn mouse_double_click(&self, path: &str) -> Result<()> {
        self.command("mouseDoubleClick", &[path.into()]).map(drop)
    }

    pub fn mouse_click_with(
        &self,
        path: &str,
        button: MouseButton,
        modifiers: Modifiers,
    ) -> Result<()> {
        self.command(
            "mouseClickWithButton",
            &[
                path.into(),
                Value::Int(button.spix_code()),
                Value::Int(modifiers.0),
            ],
        )
        .map(drop)
    }

    /// Turns the mouse wheel over the centre of the item `steps` times, `interval` apart.
    ///
    /// The deltas are Qt's `angleDelta`: 120 is one notch, and a positive `delta_y` scrolls
    /// towards the start (up). The Slint app treats one notch as 60 logical pixels. The call may
    /// return before the app has handled every step (the Qt app only queues them), and the view
    /// glides on after the last one, so callers wait for what they expect to see instead of
    /// reading a position at once.
    pub fn mouse_wheel(
        &self,
        path: &str,
        delta_x: i32,
        delta_y: i32,
        steps: u32,
        interval: Duration,
    ) -> Result<()> {
        self.command(
            "mouseWheel",
            &[
                path.into(),
                Value::Int(delta_x),
                Value::Int(delta_y),
                Value::Int(i32::try_from(steps).unwrap_or(i32::MAX)),
                Value::Int(i32::try_from(interval.as_millis()).unwrap_or(i32::MAX)),
            ],
        )
        .map(drop)
    }

    /// Types `text` into the window of the element at `path`, which goes to the focused element.
    pub fn input_text(&self, path: &str, text: &str) -> Result<()> {
        self.command("inputText", &[path.into(), text.into()])
            .map(drop)
    }

    pub fn enter_key(&self, path: &str, key: Key, modifiers: Modifiers) -> Result<()> {
        self.command(
            "enterKey",
            &[path.into(), Value::Int(key.0), Value::Int(modifiers.0)],
        )
        .map(drop)
    }

    /// False for a missing element too (not an error).
    pub fn exists_and_visible(&self, path: &str) -> Result<bool> {
        match self.command("existsAndVisible", &[path.into()])? {
            Value::Bool(visible) => Ok(visible),
            value => Err(unexpected("existsAndVisible", value)),
        }
    }

    pub fn text(&self, path: &str) -> Result<String> {
        self.property(path, "text")
    }

    pub fn set_text(&self, path: &str, text: &str) -> Result<()> {
        self.set_property(path, "text", text)
    }

    pub fn checked(&self, path: &str) -> Result<bool> {
        self.bool_property(path, "checked")
    }

    /// The `truncated` property of a text element: whether its text is elided.
    pub fn truncated(&self, path: &str) -> Result<bool> {
        self.bool_property(path, "truncated")
    }

    pub fn set_checked(&self, path: &str, checked: bool) -> Result<()> {
        self.set_property(path, "checked", if checked { "true" } else { "false" })
    }

    pub fn enabled(&self, path: &str) -> Result<bool> {
        self.bool_property(path, "enabled")
    }

    /// The `count` of a list or repeater.
    pub fn count(&self, path: &str) -> Result<usize> {
        let count = self.property(path, "count")?;
        count
            .parse()
            .map_err(|_| unexpected("getStringProperty", Value::String(count)))
    }

    pub fn bounding_box(&self, path: &str) -> Result<Rect> {
        match self.command("getBoundingBox", &[path.into()])? {
            Value::Array(values) => match values.as_slice() {
                &[
                    Value::Double(x),
                    Value::Double(y),
                    Value::Double(width),
                    Value::Double(height),
                ] => Ok(Rect {
                    x,
                    y,
                    width,
                    height,
                }),
                _ => Err(unexpected("getBoundingBox", Value::Array(values))),
            },
            value => Err(unexpected("getBoundingBox", value)),
        }
    }

    /// A PNG of the element at `path`, cropped from the window, written to `file` by the app and
    /// returned. Both servers write it before they answer. An old `file` is removed first, so one
    /// the app didn't write can't pass for the screenshot.
    pub fn screenshot(&self, path: &str, file: &Path) -> Result<Vec<u8>> {
        let io = |source| GuiError::Screenshot {
            path: file.to_path_buf(),
            source,
        };
        match std::fs::remove_file(file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io(error)),
            _ => {}
        }
        self.command(
            "takeScreenshot",
            &[path.into(), file.to_string_lossy().as_ref().into()],
        )?;
        std::fs::read(file).map_err(io)
    }

    /// Calls `check` until it says true, for up to `timeout`: the core works asynchronously, so the
    /// UI changes some time after a click. `what` names the awaited state in the timeout error.
    /// A Spix error from `check` (an element that isn't there yet) is retried; the last one is in
    /// the timeout error.
    pub fn wait_until(
        &self,
        what: &str,
        timeout: Duration,
        mut check: impl FnMut() -> Result<bool>,
    ) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut last = None;
        loop {
            match check() {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error @ GuiError::Spix(_)) => last = Some(error.to_string()),
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(GuiError::Timeout {
                    what: what.to_owned(),
                    timeout,
                    last,
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Waits until the element exists and is visible.
    pub fn wait_until_visible(&self, path: &str, timeout: Duration) -> Result<()> {
        self.wait_until(&format!("{path} to be visible"), timeout, || {
            self.exists_and_visible(path)
        })
    }

    /// Waits until the element is missing or hidden.
    pub fn wait_until_hidden(&self, path: &str, timeout: Duration) -> Result<()> {
        self.wait_until(&format!("{path} to be hidden"), timeout, || {
            self.exists_and_visible(path).map(|visible| !visible)
        })
    }

    /// Waits until the element's `text` is exactly `expected`. The timeout error says what it
    /// showed last.
    pub fn wait_until_text(&self, path: &str, expected: &str, timeout: Duration) -> Result<()> {
        let mut shown = None;
        self.wait_until(&format!("{path} to show {expected:?}"), timeout, || {
            let text = self.text(path)?;
            let done = text == expected;
            shown = Some(text);
            Ok(done)
        })
        .map_err(|error| match (error, shown) {
            (GuiError::Timeout { what, timeout, .. }, Some(text)) => GuiError::Timeout {
                what,
                timeout,
                last: Some(format!("it shows {text:?}")),
            },
            (error, _) => error,
        })
    }

    /// Lets the app run for `duration` before it handles the next call.
    pub fn wait(&self, duration: Duration) -> Result<()> {
        let ms = i32::try_from(duration.as_millis()).unwrap_or(i32::MAX);
        self.command("wait", &[Value::Int(ms)]).map(drop)
    }

    /// Asks the app to quit. No call works after this one.
    pub fn quit(&self) -> Result<()> {
        self.call("quit", &[]).map(drop)
    }

    /// Succeeds once the server answers. Leaves earlier Spix errors for the next call to report.
    pub(crate) fn ping(&self) -> Result<()> {
        self.call("getErrors", &[]).map(drop)
    }

    fn property(&self, path: &str, name: &str) -> Result<String> {
        match self.command("getStringProperty", &[path.into(), name.into()])? {
            Value::String(value) => Ok(value),
            value => Err(unexpected("getStringProperty", value)),
        }
    }

    fn bool_property(&self, path: &str, name: &str) -> Result<bool> {
        match self.property(path, name)?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(unexpected(
                "getStringProperty",
                Value::String(other.to_owned()),
            )),
        }
    }

    fn set_property(&self, path: &str, name: &str, value: &str) -> Result<()> {
        self.command(
            "setStringProperty",
            &[path.into(), name.into(), value.into()],
        )
        .map(drop)
    }

    /// Calls `method`, then fails with any error Spix reported since the previous command. Spix
    /// runs calls in order on the GUI thread, so this also waits until `method` has run.
    fn command(&self, method: &'static str, params: &[Value]) -> Result<Value> {
        let result = self.call(method, params)?;
        let errors = match self.call("getErrors", &[])? {
            Value::Array(errors) => errors,
            value => return Err(unexpected("getErrors", value)),
        };
        let new: Vec<String> = errors
            .into_iter()
            .skip(self.errors_seen.get())
            .map(|e| match e {
                Value::String(s) => s,
                other => format!("{other:?}"),
            })
            .collect();
        if new.is_empty() {
            Ok(result)
        } else {
            self.errors_seen.set(self.errors_seen.get() + new.len());
            Err(GuiError::Spix(new))
        }
    }

    fn call(&self, method: &'static str, params: &[Value]) -> Result<Value> {
        self.rpc
            .call(method, params)
            .map_err(|source| GuiError::Rpc { method, source })
    }
}

fn unexpected(method: &'static str, value: Value) -> GuiError {
    GuiError::UnexpectedValue { method, value }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client nothing answers: `wait_until` makes no Spix calls of its own.
    fn gui() -> Gui {
        Gui::new(1)
    }

    // INFRA-002
    #[test]
    fn waiting_returns_as_soon_as_the_check_holds() {
        let mut calls = 0;
        gui()
            .wait_until("three calls", Duration::from_secs(10), || {
                calls += 1;
                Ok(calls == 3)
            })
            .unwrap();
        assert_eq!(calls, 3);
    }

    // INFRA-002
    #[test]
    fn waiting_names_what_it_waited_for_when_it_times_out() {
        let result = gui().wait_until("the sun to rise", Duration::from_millis(100), || Ok(false));
        let message = result.unwrap_err().to_string();
        assert!(message.contains("the sun to rise"), "{message}");
        assert!(message.contains("timed out"), "{message}");
    }

    // INFRA-002
    #[test]
    fn waiting_retries_spix_errors_and_reports_the_last_one() {
        let mut calls = 0;
        gui()
            .wait_until("the element", Duration::from_secs(10), || {
                calls += 1;
                if calls < 3 {
                    Err(GuiError::Spix(vec!["item not found".to_owned()]))
                } else {
                    Ok(true)
                }
            })
            .unwrap();

        let result = gui().wait_until("the element", Duration::from_millis(100), || {
            Err(GuiError::Spix(vec!["item not found".to_owned()]))
        });
        let message = result.unwrap_err().to_string();
        assert!(message.contains("item not found"), "{message}");
    }

    // INFRA-002
    #[test]
    fn waiting_gives_up_on_other_errors() {
        let result = gui().wait_until("anything", Duration::from_secs(10), || {
            Err(unexpected("getErrors", Value::Bool(true)))
        });
        assert!(
            matches!(result, Err(GuiError::UnexpectedValue { .. })),
            "{result:?}"
        );
    }
}
