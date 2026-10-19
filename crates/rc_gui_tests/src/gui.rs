// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Client for the running app's Spix server, limited to the calls both frontends' servers support
//! (ARCHITECTURE.md, GUI testing contract).

use std::cell::Cell;
use std::ops::BitOr;
use std::time::Duration;

use crate::xmlrpc::{self, Value};

/// How long one call may take, including any queued [`Gui::wait`].
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

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
    pub const BACKSPACE: Self = Self(0x0100_0003);
    pub const RETURN: Self = Self(0x0100_0004);
    pub const ENTER: Self = Self(0x0100_0005);
    pub const DELETE: Self = Self(0x0100_0007);
    pub const LEFT: Self = Self(0x0100_0012);
    pub const UP: Self = Self(0x0100_0013);
    pub const RIGHT: Self = Self(0x0100_0014);
    pub const DOWN: Self = Self(0x0100_0015);
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
