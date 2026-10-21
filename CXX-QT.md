# cxx-qt notes

The limitations we hit in cxx-qt (0.10.0) and its CMake integration (cxx-qt-cmake, Corrosion), how we work around them, and what could be fixed upstream. Remove an entry when a release we use fixes it.

## QEnum variants can't have explicit values

`#[qenum]` rejects `Variant = 256` ("QEnum variants with explicit values are not supported (yet)", `cxx-qt-gen` `parser/qenum.rs`). Variants are numbered from 0.

- **Workaround:** the `RoomListModel` roles are plain `i32` constants starting at `Qt::UserRole` (`crates/rc_qt/src/room_list_model.rs`). A qenum would number them from 0 and clash with `Qt::DisplayRole`.
- **Possible patch:** support discriminants in `cxx-qt-gen`. The parser already has a `TODO: Allow discriminants`. The generated C++ enum and the Rust side would both need to carry the value.

## Model notifications are `unsafe` and unpaired

There are no `QAbstractItemModel` bindings in cxx-qt-lib. Each model declares `beginInsertRows`, `endInsertRows` and the others itself with `#[inherit]`, as `unsafe fn`, because a `begin*` without its `end*` corrupts the model. Nothing enforces the pairing.

- **Workaround:** every call sits next to its partner, with a `SAFETY:` comment (`RoomListModel::apply`). The decision about which calls to make is a pure, tested function (`crates/rc_qt/src/row_ops.rs`).
- **Possible patch:** a `QAbstractListModel` base in cxx-qt-lib with safe, scoped helpers, for example `insert_rows(first, last, |model| ...)`, which calls `begin*`, runs the closure that changes the data, then calls `end*`.

## Sharing one backend between several QObjects

QObjects created by the QML engine (singletons, elements) are default-constructed. Rust gets no say in what they're built with. Linking them to each other goes through `*mut T` properties, which QML has to assign and Rust has to dereference with `unsafe`, with no lifetime guarantee.

- **Workaround:** a process-wide hub (`crates/rc_qt/src/hub.rs`) owns the `rc_core::Client`. Each view model registers its `CxxQtThread` from `Initialize` and replays the hub's cached state, so neither QML wiring nor pointers are needed.
- **Possible patch:** a way to register a QML singleton instance built in Rust, with data passed in (like `qmlRegisterSingletonInstance`, or a factory closure), from the C++ host or from Rust.

## Native libraries of Rust dependencies don't reach the C++ link

The Rust staticlib is linked into a C++ executable. Corrosion adds the platform's standard native libraries, which it gets from `--print=native-static-libs` on a dummy crate in `FindRust.cmake`. It doesn't add the libraries that a dependency's build script asks for (`cargo:rustc-link-lib`), so the link fails with undefined symbols. libdbus, needed by the keyring store, was the first case.

- **Workaround:** `src/gui/CMakeLists.txt` finds `dbus-1` with pkg-config and links it to the executable. Every Rust dependency that wraps a C library will need the same.
- **Possible patch (Corrosion):** collect the build scripts' `linked_libs` (`cargo build --message-format=json`, `build-script-executed` messages) and add them to the imported target's `INTERFACE_LINK_LIBRARIES`.

## Two models can't both declare `QAbstractListModel`

Each list model's bridge declares its Qt base class as `extern "C++Qt"` with `#[qobject] type QAbstractListModel;`. cxx-qt then generates `cxx_qt_ffi_QAbstractListModel_upcastPtr` and `downcastPtr` in every bridge that does, with the same symbol names, so a second model fails to link ("duplicate symbol").

- **Workaround:** `MessageListModel` reuses the first declaration with `type QAbstractListModel = crate::room_list_model::qobject::QAbstractListModel;` and no `#[qobject]` (cxx rejects the attribute on an alias). Keep the original in `RoomListModel`'s bridge.
- **Possible patch:** make the generated symbols unique per bridge, or provide `QAbstractListModel` once in cxx-qt-lib (which would also be the place for the safe `begin*`/`end*` helpers mentioned above).

## An instrumented staticlib has no profiler runtime

With `-Cinstrument-coverage`, rustc links the LLVM profiler runtime (`profiler_builtins`) into executables and dylibs, but not into a staticlib, and nothing in the staticlib refers to it. The C++ executable links, but never writes a profile.

- **Workaround:** with `ROCKETCHATQML_COVERAGE` (the `coverage` preset), `src/gui/CMakeLists.txt` links the toolchain's `libprofiler_builtins-*.rlib` (a plain archive) after the staticlib, with `-Wl,-u,__llvm_profile_runtime` to pull in the member that registers the write at exit. The flag itself reaches cargo through Corrosion's `corrosion_add_target_rustflags`.
- **Possible patch (Corrosion or rustc):** link `profiler_builtins` into staticlibs built with coverage instrumentation, or let Corrosion add it to the imported target.

## A list model that QML creates as an element is an unresolved type for qmllint

An `EmojiPickerModel` declared with `#[qml_element]` and `#[base = QAbstractListModel]`, used as `EmojiPickerModel { }` in QML, makes qmllint warn "QAbstractListModel was not found" and "Type ... is used but it is not resolved". The generated QML module does not carry a type for the base class (Qt doesn't register `QAbstractListModel` for QML either), so the element can't be linted. A singleton has no such problem, because QML only reads its properties.

- **Workaround:** the emoji picker's model is a `#[qml_singleton]` (`EmojiCatalog`), which fits, as there is one picker. A second picker would have to share it. Its `query` property is set from the search box's `onTextChanged`: a `Binding { target: EmojiCatalog }` is rejected by qmllint too ("Cannot assign binding of type EmojiCatalog to QObject").
- **Possible patch:** let a `#[base = ...]` that cxx-qt-lib knows (`QAbstractListModel`) come with a `.qmltypes` entry, or generate `QML_ANONYMOUS`/`QML_FOREIGN` registrations for the base.

## A property with a custom setter

`#[qproperty(QString, query, READ, WRITE = set_query_text, NOTIFY)]` works when `set_query_text` is declared in the bridge as a `#[qinvokable]` with `#[cxx_name = "setQueryText"]`; Qt's `Q_PROPERTY(... WRITE setQueryText)` accepts an invokable. The function has to emit the `NOTIFY` signal itself. The default setter stores the value and emits the signal, with no hook to reset a model first.
