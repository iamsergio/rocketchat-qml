// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("RocketChat.Bridge"))
        .qt_module("Qml")
        .files([
            "src/app_info.rs",
            "src/lifecycle.rs",
            "src/logging.rs",
            "src/message_list_model.rs",
            "src/room_list_model.rs",
            "src/room_vm.rs",
            "src/session_vm.rs",
        ])
        .build();
}
