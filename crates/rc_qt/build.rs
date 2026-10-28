// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("RocketChat.Bridge"))
        .qt_module("Qml")
        .qt_module("Gui")
        .files([
            "src/account_list_model.rs",
            "src/app_info.rs",
            "src/clipboard.rs",
            "src/emoji_picker_model.rs",
            "src/lifecycle.rs",
            "src/logging.rs",
            "src/message_list_model.rs",
            "src/room_list_model.rs",
            "src/room_search_model.rs",
            "src/room_vm.rs",
            "src/session_vm.rs",
            "src/staged_files_model.rs",
            "src/thread_list_model.rs",
            "src/thread_list_vm.rs",
            "src/thread_message_list_model.rs",
            "src/thread_vm.rs",
            "src/upload_list_model.rs",
        ])
        .cpp_file("src/clipboard_probe.cpp")
        .build();
}
