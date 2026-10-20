// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't start the async runtime")]
    Runtime(#[source] std::io::Error),
}
