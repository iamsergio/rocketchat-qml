// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The `EmojiCatalog` model: the emoji a user can react with, filtered by `query`. The filter
//! changes the whole list, so it resets the model; so do new custom emoji.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::room_list_model::qobject::QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
    }

    unsafe extern "RustQt" {
        /// Available in QML as the `EmojiCatalog` singleton: the emoji of the one emoji picker.
        #[qobject]
        #[qml_element = "EmojiCatalog"]
        #[qml_singleton]
        #[base = QAbstractListModel]
        // Only the emoji whose shortcodes contain it, ignoring case. Empty lists all of them.
        #[qproperty(QString, query, READ, WRITE = set_query_text, NOTIFY)]
        type EmojiPickerModel = super::EmojiPickerModelRust;

        /// The setter of `query`.
        #[qinvokable]
        #[cxx_name = "setQueryText"]
        fn set_query_text(self: Pin<&mut EmojiPickerModel>, query: &QString);
    }

    // The begin/end pairs must be kept balanced, so they are unsafe to call.
    extern "RustQt" {
        /// # Safety
        ///
        /// Call `end_reset_model` after changing the data.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut EmojiPickerModel>);
        /// # Safety
        ///
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut EmojiPickerModel>);
    }

    extern "RustQt" {
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &EmojiPickerModel, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(self: &EmojiPickerModel, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &EmojiPickerModel) -> QHash_i32_QByteArray;
    }

    impl cxx_qt::Threading for EmojiPickerModel {}
    impl cxx_qt::Initialize for EmojiPickerModel {}
}

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use rc_core::emoji::CustomEmojis;

use crate::hub;
use rc_view::emoji_picker_view::Picker;

use qobject::EmojiPickerModel;

/// `Qt::DisplayRole`, which shows the shortcode.
const DISPLAY_ROLE: i32 = 0;

// The custom roles, from `Qt::UserRole` on (see `RoomListModel`).
const SHORTCODE_ROLE: i32 = 256;
const GLYPH_ROLE: i32 = 257;
const IMAGE_ROLE: i32 = 258;

/// Backing data of [`qobject::EmojiPickerModel`].
#[derive(Default)]
pub struct EmojiPickerModelRust {
    query: QString,
    picker: Picker,
}

impl cxx_qt::Initialize for EmojiPickerModel {
    fn initialize(mut self: Pin<&mut Self>) {
        let emoji = hub::register_emoji_picker(self.qt_thread());
        self.as_mut().rust_mut().picker = Picker::with_emoji(emoji);
    }
}

impl EmojiPickerModel {
    /// Lists what matches the text from now on.
    fn set_query_text(mut self: Pin<&mut Self>, query: &QString) {
        let text = query.to_string();
        if self.picker.query() == text {
            return;
        }
        // SAFETY: balanced by `end_reset_model` below.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().picker.set_query(&text);
        // SAFETY: after `begin_reset_model`.
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().rust_mut().query = query.clone();
        self.query_changed();
    }

    /// Lists the server's custom emoji too, keeping the query.
    pub fn set_emoji(mut self: Pin<&mut Self>, emoji: &CustomEmojis) {
        if self.picker.emoji() == emoji {
            return;
        }
        // SAFETY: balanced by `end_reset_model` below.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().picker.set_emoji(emoji);
        // SAFETY: after `begin_reset_model`.
        unsafe { self.as_mut().end_reset_model() };
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.picker.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(emoji) = usize::try_from(index.row())
            .ok()
            .and_then(|at| self.picker.get(at))
        else {
            return QVariant::default();
        };
        let text = |value: &str| QVariant::from(&QString::from(value));
        match role {
            DISPLAY_ROLE | SHORTCODE_ROLE => text(&emoji.shortcode),
            GLYPH_ROLE => text(&emoji.glyph),
            IMAGE_ROLE => text(&emoji.image),
            _ => QVariant::default(),
        }
    }

    #[allow(clippy::unused_self)] // Signature fixed by `QAbstractItemModel::roleNames`.
    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (DISPLAY_ROLE, "display"),
            (SHORTCODE_ROLE, "shortcode"),
            (GLYPH_ROLE, "glyph"),
            (IMAGE_ROLE, "image"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
}
