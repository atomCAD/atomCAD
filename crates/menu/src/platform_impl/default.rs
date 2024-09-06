// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

//! Currently does nothing, and is present merely to ensure we compile on all platforms, including
//! those that don't natively support any menubar functionality.

use crate::Blueprint;
use winit::{event_loop::EventLoopBuilder, window::Window};

pub fn configure_event_loop<T: 'static>(event_loop_builder: &mut EventLoopBuilder<T>) {
    let _ = event_loop_builder;
}

pub fn attach_to_window(
    // On some platforms, e.g. Windows and Linux, the menu bar is part of the window itself, and we
    // need to attach a copy of the menu to each individual window.
    window: &Window,
    // The layout of the menubar to be used when this window is in focus.
    blueprint: &Blueprint,
) {
    let _ = (window, blueprint);
}

// End of File
