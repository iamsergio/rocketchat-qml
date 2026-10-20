// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `--dev`, in developer-mode builds only: the login page starts filled in for the local test
//! server, so trying a change takes one click. Plain Rust, so it can be unit-tested without Qt.

use std::sync::OnceLock;

/// The local test server (`scripts/run_test_server.sh`) and its admin account.
const DEFAULT_SERVER: &str = "http://127.0.0.1:3000";
const DEFAULT_USERNAME: &str = "admin";
const DEFAULT_PASSWORD: &str = "admin";

// Environment variables that override the defaults, e.g. to log in as a test user.
const SERVER_ENV: &str = "RC_DEV_SERVER";
const USERNAME_ENV: &str = "RC_DEV_USER";
const PASSWORD_ENV: &str = "RC_DEV_PASS";

/// What the login fields start with. Empty unless `--dev` was given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DevLogin {
    pub server: String,
    pub username: String,
    /// Only ever a test server's password: it sits in a QML property in clear.
    pub password: String,
}

// Set once, before the QML is loaded; the `Session` view model reads it when QML creates it.
static PREFILL: OnceLock<DevLogin> = OnceLock::new();

/// The defaults, with each one that `var` (the environment) has a non-empty value for replaced.
#[cfg_attr(not(feature = "developer-mode"), allow(dead_code))]
fn from_env(var: impl Fn(&str) -> Option<String>) -> DevLogin {
    let value = |name: &str, default: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.to_owned())
    };
    DevLogin {
        server: value(SERVER_ENV, DEFAULT_SERVER),
        username: value(USERNAME_ENV, DEFAULT_USERNAME),
        password: value(PASSWORD_ENV, DEFAULT_PASSWORD),
    }
}

/// Fills in the login fields from now on. Call before the QML is loaded.
#[cfg(feature = "developer-mode")]
pub fn enable() {
    let login = from_env(|name| std::env::var(name).ok());
    tracing::info!(server = %login.server, username = %login.username, "--dev: login fields filled in");
    if PREFILL.set(login).is_err() {
        tracing::debug!("--dev: already enabled");
    }
}

/// What the login fields start with.
#[must_use]
pub fn prefill() -> DevLogin {
    PREFILL.get().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // INFRA-003
    #[test]
    fn defaults_to_the_local_test_server_admin() {
        assert_eq!(
            from_env(|_| None),
            DevLogin {
                server: "http://127.0.0.1:3000".into(),
                username: "admin".into(),
                password: "admin".into(),
            }
        );
    }

    // INFRA-003
    #[test]
    fn the_environment_overrides_each_default() {
        let login = from_env(|name| match name {
            "RC_DEV_SERVER" => Some("http://localhost:4000".into()),
            "RC_DEV_USER" => Some("bob".into()),
            // Empty counts as unset.
            "RC_DEV_PASS" => Some(String::new()),
            _ => None,
        });
        assert_eq!(login.server, "http://localhost:4000");
        assert_eq!(login.username, "bob");
        assert_eq!(login.password, "admin");
    }

    // INFRA-003
    #[test]
    fn nothing_is_filled_in_without_dev() {
        // Nothing in the unit tests calls `enable`.
        assert_eq!(prefill(), DevLogin::default());
    }
}
