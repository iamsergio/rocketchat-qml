// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("RocketChat.Bridge"))
        .qt_module("Qml")
        .files(["src/app_info.rs", "src/logging.rs"])
        .build();
}
