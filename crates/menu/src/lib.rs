// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

mod platform_impl;

mod blueprint;
pub use blueprint::{Action, Blueprint, Item, Shortcut, SystemAction, SystemShortcut};

mod plugin;
pub use plugin::MenubarPlugin;

/// Re-export the public API of the menu crate.
pub mod prelude {
    pub use crate::blueprint::{Action, Blueprint, Item, Shortcut, SystemAction, SystemShortcut};
    pub use crate::plugin::MenubarPlugin;
}

use winit::{event_loop::EventLoopBuilder, window::Window};

/// The platform-specific setup function is called during application initialization to configure
/// the event loop with the necessary platform-specific menu handling code, and returns a handle to
/// a platform-specific datastructure which contains the necessary information to create and attach
/// menus to windows.
pub fn platform_setup<T: 'static>(event_loop_builder: &mut EventLoopBuilder<T>) {
    platform_impl::configure_event_loop(event_loop_builder)
}

/// Attach the menubar to the window.  This function is called when the window is created and
/// attached to the event loop.  It is responsible for creating the platform-specific menu objects
/// from the blueprint spec, and attaching them to the window.
pub fn attach_menubar_to_window(window: &Window, blueprint: &Blueprint) {
    // Do the platform-dependent work of constructing the menubar and
    // attaching it to the application object or main window.
    platform_impl::attach_to_window(window, blueprint);
}

// End of File
