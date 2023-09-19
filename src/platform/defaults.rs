// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

#[allow(dead_code)]
pub mod bevy {
    use bevy::app::{App, Plugin};

    pub struct PlatformTweaks;

    impl Plugin for PlatformTweaks {
        fn build(&self, _app: &mut App) {}
    }
}

// End of File
