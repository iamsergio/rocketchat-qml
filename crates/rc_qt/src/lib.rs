// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `QObject`s and models that expose `rc_core` to QML, in the `RocketChat.Bridge` QML module, and
//! the functions the C++ host calls at startup.

// Modules holding a cxx-qt bridge allow what its expansion needs: unsafe FFI glue and constructors
// returning `Box<T>`. cxx-qt rejects attributes on the bridge module itself, so they go here. Keep
// these modules to bridge code only.
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod app_info;
#[allow(unsafe_code)]
pub mod logging;
