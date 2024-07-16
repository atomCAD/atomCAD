// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

//! # atomCAD's Logging Framework
//!
//! This crate initializes the logging framework for atomCAD.  It of course usse the most excellent
//! [`log`] crate for logging, [`env_logger`] to read logging configuration from the environment on
//! desktop platforms, and `console_log` to output logs to the Javascript console in the browser.
//! To initialize these frameworks, this crate exposes a [`LoggingPlugin`] type that can be
//! instantiated and added to the applications [`app::App`].
//!
//! By default, the logging level is set to [`Info`](log::Level::Info) for debug builds, and
//! [`Warn`](log::Level::Warn) for release builds. This can be overridden by setting the `RUST_LOG`
//! environment variable, like so:
//!
//! ```sh
//! $> RUST_LOG=atomcad=debug cargo run
//! ```

mod platform;
mod platform_impl;
pub use platform::LoggingPlugin;

/// A module which is typically glob imported.
pub mod prelude {
    pub use super::LoggingPlugin;
}

// End of File
