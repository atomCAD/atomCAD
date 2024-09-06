// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

//! Currently does nothing, and is present merely to ensure we compile on all platforms, including
//! those that don't natively support any menubar functionality.

use crate::Blueprint;
use bevy::{prelude::*, winit::WinitWindows};

pub fn configure_event_loop(windows: NonSend<WinitWindows>) {
    let _ = windows;
}

pub fn attach_to_window(
    // On some platforms, e.g. Windows and Linux, the menu bar is part of the window itself, and we
    // need to attach a copy of the menu to each individual window.
    windows: &WinitWindows,
    // The layout of the menubar to be used when this window is in focus.
    blueprint: &Blueprint,
) {
    let _ = (windows, blueprint);
}

// End of File
