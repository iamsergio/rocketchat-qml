// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the GitHub pull requests screens show: the state a frontend keeps from the
//! [`GithubEvent`](rc_core::github::GithubEvent)s ([`GithubCache`]) and the plain views both
//! frontends build their rows, page, tile and header from. Plain Rust that knows no GUI toolkit.
//! Nothing here reads the clock: the views that show an age or a duration take `now`, so the
//! frontend decides when to redraw them. See docs/design/github.md.

mod cache;
mod chrome;
mod page;
mod row;
pub mod text;

#[cfg(test)]
mod tests;

pub use cache::{Applied, CommandStatus, GithubCache, PrCommand, Sections, Tab};
pub use chrome::{ConnectState, ConnectView, HeaderView, RunnerLine, TileView};
pub use page::{BaseNote, Button, ChecksView, JobGroupView, JobRowView, PrPageView};
pub use row::{CiKind, Glyph, PrRowView, Segment};
