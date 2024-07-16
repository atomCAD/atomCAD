// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use app::{self, App};

/// Initializes the logging framework to capture logs from the crates specified with a minimum
/// [`log::Level`] of [`Info`](log::Level::Info) on debug builds and [`Warn`](log::Level::Warn) on
/// release builds.  On desktop platforms, these defaults can be overridden by setting the
/// `RUST_LOG` environment variable.
pub struct LoggingPlugin {
    crates: Vec<&'static str>,
}

/// Method for creating a [`LoggingPlugin`] instance, specifying the crates to capture logs from.
impl LoggingPlugin {
    /// Creates a new [`LoggingPlugin`] instance with the specified list of crates to capture logs
    /// from.
    pub fn new(crates: Vec<&'static str>) -> Self {
        Self { crates }
    }
}

/// Configures the logging framework for an [`App`] instance, enabling logs to be captured from the
/// terminal on desktop platforms or sent to the Javascript console when running in a browser, with
/// a minimum [`log::Level`] set based on the build profile.
impl app::Plugin for LoggingPlugin {
    /// Configure logging output to be filtered by [`Info`](log::Level::Info) in debug builds, and
    /// by [`Warn`](log::Level::Warn) in release.  This can be overridden on desktop platforms by
    /// setting the `RUST_LOG` environment variable, like so:
    ///
    /// ```sh
    /// $> RUST_LOG=atomcad=debug cargo run
    /// ```
    fn register(&self, _app: &mut App) {
        crate::platform_impl::init_with_level(
            &self.crates,
            if cfg!(debug_assertions) {
                log::LevelFilter::Info
            } else {
                log::LevelFilter::Warn
            },
        );
    }
}

// End of File
