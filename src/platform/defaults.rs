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

#[allow(dead_code)]
pub mod menubar {
    use crate::menubar::Menu;
    use bevy::winit::WinitWindows;

    // Currently does nothing, and is present merely to ensure we compile on
    // platforms, including those that don't natively support any menubar
    // functionality.
    pub fn attach_menu(_windows: &WinitWindows, _menu: &Menu) {}
}

// End of File
