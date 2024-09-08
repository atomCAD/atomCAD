// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

//! Contains default implementations for the platform specific code, that can be (partially-)reused
//! on platforms that don't need customization.  Only accessed by the platform specific modules
//! below, so it not exposed as public.

// Some platforms re-export the default implementations for features that do not need platform
// customization.  This unfortunately means that when a platform does provide a custom version of
// these APIs, dead code warnings are emitted for the default implementations that are not used,
// even though those implementations are used required for other platforms.  To avoid this, we
// suppress the dead code warnings for the entire default module.
#![allow(dead_code)]

use std::ffi::{OsStr, OsString};

use bevy::app::App;

/// Does nothing on platforms which don't need customization.
pub(crate) fn tweak_bevy_app(app: &mut App) {
    let _ = app;
}

pub(crate) fn get_process_name() -> OsString {
    // Option 1: Try std::env::args()
    if let Some(arg0) = std::env::args().next()
        && let Some(name) = std::path::PathBuf::from(arg0).file_name()
    {
        return name.to_owned();
    }

    // Option 2: Using std::env::current_exe()
    if let Ok(path) = std::env::current_exe()
        && let Some(name) = path.file_name()
    {
        return name.to_owned();
    }

    // Fallback: Use the crate name in Cargo.toml
    OsString::from(env!("CARGO_PKG_NAME"))
}

pub(crate) fn set_process_name(name: &OsStr) {
    let _ = name;
}

// End of File
