// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#pragma once

#include "rust/cxx.h"

namespace rc_qt {

/// The URLs the clipboard lists, as strings.
rust::Vec<rust::String> clipboard_urls();

/// The clipboard's plain text, empty if it has none.
rust::String clipboard_text();

/// Whether the clipboard holds a picture.
bool clipboard_has_image();

/// Saves the clipboard's picture to `path` as a PNG. False if there is none or it can't be written.
bool clipboard_save_image(rust::Str path);

/// Replaces the clipboard with the local files `paths`, the text and the picture in the file
/// `image_path` (each left out if empty). For the GUI tests, which can't copy anything.
void clipboard_seed(const rust::Vec<rust::String> &paths, rust::Str text, rust::Str image_path);

} // namespace rc_qt
