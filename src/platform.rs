// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

//! Platform-specific code.

pub mod bevy {
    use bevy::app::{App, Plugin};

    pub struct PlatformTweaks;

    impl Plugin for PlatformTweaks {
        fn build(&self, app: &mut App) {
            crate::platform_impl::tweak_bevy_app(app);
        }
    }
}

// End of File
