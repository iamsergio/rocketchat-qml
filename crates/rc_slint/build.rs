// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

fn main() -> Result<(), slint_build::CompileError> {
    // The element search API (`ElementHandle`), which the tests and the coming Spix server are
    // built on, needs the element names that debug info embeds.
    let config = slint_build::CompilerConfiguration::new().with_debug_info(true);
    slint_build::compile_with_config("ui/main.slint", config)
}
