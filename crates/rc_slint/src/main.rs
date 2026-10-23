// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rocketchat_slint`: the Slint frontend.

use std::error::Error;

use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn Error>> {
    rc_core::logging::init_app(cfg!(feature = "developer-mode"));
    let args = Args::from_env()?;
    if args.dev {
        enable_dev_login();
    }
    // `--spix-port` uses the testing backend unless SLINT_BACKEND picks another (winit, to watch the app).
    #[cfg(feature = "spix")]
    if args.spix_port.is_some() && std::env::var_os("SLINT_BACKEND").is_none() {
        i_slint_backend_testing::init_integration_test_with_system_time();
    }
    // The window first: the core's events hop to the event loop, which the backend provides.
    let window = rc_slint::new_main_window()?;
    // The server after the window exists, so that its calls have a window to drive.
    #[cfg(feature = "spix")]
    if let Some(port) = args.spix_port {
        spix_slint::start(port, [("mainWindow", window.as_weak())])?;
    }
    #[cfg(not(feature = "spix"))]
    if args.spix_port.is_some() {
        tracing::warn!("--spix-port needs a spix build: ignored");
    }
    rc_slint::app::start(&window);
    let result = window.run();
    // After the event loop: stopping the core waits for a pending logout.
    rc_slint::app::stop();
    Ok(result?)
}

/// What the app was started with.
#[derive(Default)]
struct Args {
    /// `--dev`: the login page starts filled in for the local test server.
    dev: bool,
    /// `--spix-port <port>`: the Spix test server answers there, so the GUI tests can drive it.
    spix_port: Option<u16>,
}

impl Args {
    /// The options of this run. A `--spix-port` without a usable port number is an error: the app
    /// would start without a server, and the harness would wait 30 s for one that never comes.
    /// Anything else on the command line is not ours to read.
    fn from_env() -> Result<Self, String> {
        let mut args = Self::default();
        let mut rest = std::env::args().skip(1);
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "--dev" => args.dev = true,
                "--spix-port" => {
                    let Some(port) = rest.next() else {
                        return Err("--spix-port needs a port number".to_owned());
                    };
                    args.spix_port = Some(
                        port.parse()
                            .map_err(|_| format!("--spix-port: {port} is not a port number"))?,
                    );
                }
                _ => {}
            }
        }
        Ok(args)
    }
}

/// `--dev`: the login page starts filled in for the local test server (`RC_DEV_SERVER`,
/// `RC_DEV_USER` and `RC_DEV_PASS` override it). Only builds with `developer-mode` do it.
fn enable_dev_login() {
    #[cfg(feature = "developer-mode")]
    rc_view::dev_login::enable();
    #[cfg(not(feature = "developer-mode"))]
    tracing::warn!("--dev needs a developer-mode build: ignored");
}
