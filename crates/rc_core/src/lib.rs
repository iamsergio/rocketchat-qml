// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Domain types, session state and use cases. Knows nothing about Qt or Slint.

#![forbid(unsafe_code)]

/// Application version, from `version.txt` at the repository root.
pub const VERSION: &str = include_str!("../../../version.txt").trim_ascii();
