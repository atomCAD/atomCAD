// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

// Disable console on windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::OsString;

use bevy::app::AppExit;

fn main() -> AppExit {
    // This fixes the application name in the "About" dialog box.
    atomcad::platform::set_process_name(&OsString::from(atomcad::APP_NAME));

    atomcad::start()
}

// End of File
