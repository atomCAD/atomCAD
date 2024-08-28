// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate::{App, Plugin};

/// Application plugin to perform platform-specific initialization.  On the web backend, this sets
/// up the console to catch and log unhandled panics.  On all other backends, this does nothing.  It
/// is a default plugin included in [`App`] instances returned by [`App::new`].
pub struct PanicHandlerPlugin;

/// When added to an [`App`] instance, ensures that any unhandled panics will be logged to the
/// Javascript console on web platforms.  Currently performs no other action on other backends.
///
/// ```
/// # use atomcad_app::prelude::*;
/// # use atomcad_app::PanicHandlerPlugin;
/// fn run(app: &mut App) {
///     panic!("Oh no!");
/// }
///
/// App::empty("Panic Handler Plugin".into())
///     .add_plugin(PanicHandlerPlugin)
///     .run();
///
/// // Backtrace will be printed to debug console.
/// ```
impl Plugin for PanicHandlerPlugin {
    /// Sets up the panic handler to log errors to the console on the web backend if we are running
    /// on WebAssembly, so that panics can be viewed in the browser's debug inspector.  Otherwise
    /// does nothing.
    fn register(&self, _app: &mut App) {
        crate::platform_impl::setup_panic_handler();
    }
}

// End of File
