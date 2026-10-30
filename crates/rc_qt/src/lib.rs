// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `QObject`s and models that expose `rc_core` to QML, in the `RocketChat.Bridge` QML module, and
//! the functions the C++ host calls at startup and shutdown.

// Modules holding a cxx-qt bridge allow what its expansion needs: unsafe FFI glue and constructors
// returning `Box<T>`. cxx-qt rejects attributes on the bridge module itself, so they go here. Keep
// these modules to bridge code and the Qt calls around it; the logic lives in the modules below,
// which have no Qt types and therefore unit tests.
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod account_list_model;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod app_info;
#[allow(unsafe_code)]
mod clipboard;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod emoji_picker_model;
mod github_view;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod github_vm;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod job_list_model;
mod job_rows;
#[allow(unsafe_code)]
pub mod lifecycle;
#[allow(unsafe_code)]
pub mod logging;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod mentions_model;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod message_list_model;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod message_search_model;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod pull_request_list;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod room_list_model;
pub mod room_roles;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod room_search_model;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `actionDone` signal.
)]
pub mod room_vm;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod runner_list_model;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod session_vm;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod staged_files_model;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod thread_list_model;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod thread_list_vm;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod thread_message_list_model;
#[allow(unsafe_code, clippy::unnecessary_box_returns)]
pub mod thread_vm;
#[allow(
    unsafe_code,
    clippy::unnecessary_box_returns,
    clippy::must_use_candidate // The generated `dataChanged` signal.
)]
pub mod upload_list_model;

mod hub;
mod message_mirror;
