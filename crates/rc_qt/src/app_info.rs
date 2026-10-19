// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Static information about the application.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Available in QML as the `AppInfo` singleton.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, version, READ, CONSTANT)]
        type AppInfo = super::AppInfoRust;
    }
}

use cxx_qt_lib::QString;

/// Backing data of [`qobject::AppInfo`].
pub struct AppInfoRust {
    version: QString,
}

impl Default for AppInfoRust {
    fn default() -> Self {
        Self {
            version: QString::from(rc_core::VERSION),
        }
    }
}
