// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rocketchat_tui`: the Slint frontend inside a terminal, drawn by ratatui-slint. A spike: it is
//! `rc_slint`'s `main.rs` with ratatui's loop in place of `window.run()`.
//!
//! Slint renders into a framebuffer that the terminal shows as an image (kitty, sixel, iTerm2) or
//! as half-blocks. Click the window to give it the keyboard; Ctrl+G leaves it, and `q` quits then.
//!
//! `--quadrant` puts the window in the top-left corner of the terminal ([`WINDOW_SHARE`] of its
//! width and height), in a bordered block, with
//! plain ratatui widgets around it: the keys, and the tail of the log. It shows that the app is
//! one widget among others, not the whole screen.

use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui_slint::terminal::{self, TerminalOptions, TerminalSession};
use ratatui_slint::{InputRouter, LoopEvent, Routed, Runtime, SlintWidget};
use tracing_subscriber::EnvFilter;

/// How much of the terminal's width and height the window takes in the quadrant layout, in percent.
const WINDOW_SHARE: u16 = 65;
/// How many lines of the log the quadrant layout keeps.
const LOG_LINES: usize = 12;
/// How much of the end of the log is read to find those lines: a line of the log is far shorter
/// than a fraction of this, and reading a fixed tail keeps the cost independent of its size.
const LOG_TAIL_BYTES: u64 = 8 * 1024;
/// The window's border while it has the keyboard, and while it hasn't.
const FOCUSED: Color = Color::Green;
const UNFOCUSED: Color = Color::DarkGray;

fn main() -> Result<(), Box<dyn Error>> {
    let log_path = log_path();
    init_logging(&log_path);
    let quadrant = std::env::args().any(|arg| arg == "--quadrant");
    if std::env::args().any(|arg| arg == "--dev") {
        enable_dev_login();
    }
    // Sets Slint's platform: before any window.
    let mut rt = Runtime::new()?;
    let mut session = TerminalSession::init(TerminalOptions::default())?;
    // After the startup queries, which read stdin too.
    rt.spawn_terminal_reader()?;
    let mut pane = rt.create_pane(&session.picker, rc_slint::new_main_window)?;
    // The core's events hop to this loop with `invoke_from_event_loop`, which the runtime serves.
    rc_slint::app::start(pane.component());
    let mut router = InputRouter::new();
    router.set_keyboard_enhanced(session.keyboard_enhanced);
    let mut log = LogTail::new(log_path);
    // What was drawn last: the screen has to be drawn again when it changes, not only when the
    // window does, since the text around the window is ours.
    let mut drawn_layout = None;
    let mut drawn_focus = false;
    let result: Result<(), Box<dyn Error>> = (|| {
        loop {
            match rt.next_event() {
                LoopEvent::Quit => break,
                LoopEvent::InputError(error) => return Err(error.into()),
                LoopEvent::Terminal(event) => {
                    if let Event::Resize(..) = event
                        && let Some(font) = terminal::cell_size()
                    {
                        pane.set_font_size(font);
                    }
                    // The router compares the mouse position with the pane's area, which
                    // `set_area` below keeps at the quadrant: a click is translated to the
                    // window's coordinates from there, so the offset needs nothing here.
                    if router.route(&event, &[&pane]) == Routed::Unhandled
                        && matches!(event, Event::Key(key)
                            if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q'))
                    {
                        break;
                    }
                }
                _ => {}
            }
            if rt.begin_frame() {
                let layout = Layout::new(session.terminal.size()?.into(), quadrant);
                pane.set_area(layout.window);
                let changed = pane.update();
                if let Some(error) = pane.last_error() {
                    return Err(std::io::Error::other(error).into());
                }
                let focused = router.focused() == Some(pane.id());
                let log_grew = quadrant && log.refresh();
                if changed || log_grew || drawn_layout != Some(layout) || drawn_focus != focused {
                    drawn_layout = Some(layout);
                    drawn_focus = focused;
                    session.terminal.draw(|frame| {
                        if let Some(side) = &layout.side {
                            draw_frame(frame, side, focused, &log);
                        }
                        frame.render_stateful_widget(SlintWidget, layout.window, &mut pane);
                    })?;
                }
            }
            rt.write_pending_output(session.terminal.backend_mut())?;
        }
        Ok(())
    })();
    drop(pane);
    // After the loop: stopping the core waits for a pending logout.
    rc_slint::app::stop();
    let cleanup = session.finish(&mut rt).map_err(Into::into);
    result.and(cleanup)
}

/// Where the screen goes: the Slint window, and in the quadrant layout the rest.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Layout {
    /// Where the window is drawn: the whole terminal, or the inside of the quadrant's border.
    window: Rect,
    /// Only in the quadrant layout.
    side: Option<Side>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Side {
    /// The window's quadrant, border included.
    frame: Rect,
    keys: Rect,
    log: Rect,
}

impl Layout {
    fn new(screen: Rect, quadrant: bool) -> Self {
        if !quadrant {
            return Self {
                window: screen,
                side: None,
            };
        }
        let (width, height) = (share(screen.width), share(screen.height));
        let frame = Rect::new(screen.x, screen.y, width, height);
        let keys = Rect::new(screen.x + width, screen.y, screen.width - width, height);
        let log = Rect::new(
            screen.x,
            screen.y + height,
            screen.width,
            screen.height - height,
        );
        Self {
            // The block's own idea of its inside, so the border can't overlap the window.
            window: Block::bordered().inner(frame),
            side: Some(Side { frame, keys, log }),
        }
    }
}

/// [`WINDOW_SHARE`] percent of `cells`.
fn share(cells: u16) -> u16 {
    // At most `cells`, so it fits back in a u16.
    u16::try_from(u32::from(cells) * u32::from(WINDOW_SHARE) / 100).unwrap_or(cells)
}

/// Everything but the window itself, which is drawn on top of the block's empty inside.
fn draw_frame(frame: &mut ratatui::Frame, side: &Side, focused: bool, log: &LogTail) {
    let border = Style::new().fg(if focused { FOCUSED } else { UNFOCUSED });
    frame.render_widget(
        Block::bordered()
            .title(" RocketChat: Slint in a ratatui pane ")
            .border_style(border),
        side.frame,
    );
    let key = Style::new().add_modifier(Modifier::BOLD);
    let keys = vec![
        Line::from(if focused {
            "The window has the keyboard."
        } else {
            "The window does not have the keyboard."
        }),
        Line::default(),
        Line::from(vec![
            Span::styled("Click", key),
            " the window to focus it".into(),
        ]),
        Line::from(vec![Span::styled("Ctrl+G", key), " leaves it".into()]),
        Line::from(vec![
            Span::styled("q", key),
            " quits (when it is not focused)".into(),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(keys).block(Block::bordered().title(" Keys ")),
        side.keys,
    );
    // The last lines that fit; a long line is cut, not wrapped.
    let room = usize::from(side.log.height.saturating_sub(2));
    let skip = log.lines.len().saturating_sub(room);
    let lines: Vec<Line> = log.lines[skip..]
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Log ")),
        side.log,
    );
}

/// The last lines of the log file. Tailed instead of followed: the file is read again only when
/// its size changed, and then only its end.
struct LogTail {
    path: PathBuf,
    len: u64,
    lines: Vec<String>,
}

impl LogTail {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            len: 0,
            lines: Vec::new(),
        }
    }

    /// True if the file grew (or was replaced by a shorter one) and `lines` has changed.
    fn refresh(&mut self) -> bool {
        let Ok(len) = std::fs::metadata(&self.path).map(|meta| meta.len()) else {
            return false;
        };
        if len == self.len {
            return false;
        }
        self.len = len;
        self.lines = read_tail(&self.path, len).unwrap_or_default();
        true
    }
}

fn read_tail(path: &Path, len: u64) -> std::io::Result<Vec<String>> {
    let mut file = File::open(path)?;
    let start = len.saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    // The first line is cut where the read started.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(LOG_LINES);
    Ok(lines[skip..].iter().map(|l| (*l).to_owned()).collect())
}

/// `$RC_TUI_LOG`, else `<executable>.log`, as the other frontends' developer-mode builds do.
fn log_path() -> PathBuf {
    std::env::var_os("RC_TUI_LOG").map_or_else(
        || {
            std::env::current_exe()
                .unwrap_or_default()
                .with_extension("log")
        },
        Into::into,
    )
}

/// Logs to the file only: the terminal belongs to the UI, and `rc_core::logging` would write to
/// stderr too.
fn init_logging(path: &Path) {
    let Ok(file) = File::options().create(true).append(true).open(path) else {
        return;
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Only fails if logging is initialized already, which nothing did.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init();
}

/// `--dev`: the login page starts filled in for the local test server.
fn enable_dev_login() {
    #[cfg(feature = "developer-mode")]
    rc_view::dev_login::enable();
    #[cfg(not(feature = "developer-mode"))]
    tracing::warn!("--dev needs a developer-mode build: ignored");
}
