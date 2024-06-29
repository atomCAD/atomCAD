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

use bevy::app::App;

/// Does nothing on platforms which don't need customization.
pub(crate) fn tweak_bevy_app(app: &mut App) {
    let _ = app;
}

// End of File
