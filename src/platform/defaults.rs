// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

#[allow(dead_code)]
pub mod menubar {
    // Currently does nothing, and is present merely to ensure we compile on
    // platforms, including those that don't natively support any menubar
    // functionality.
    use crate::menubar::Menu;
    use winit::{event_loop::EventLoopBuilder, window::Window};

    pub fn configure_event_loop<T: 'static>(
        _event_loop_builder: &mut EventLoopBuilder<T>,
        _menu: &Menu,
    ) {
    }

    pub fn attach_menu(_window: &Window, _menu: &Menu) {}
}

// End of File
