// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[cfg(not(any(target_os = "android", target_os = "macos")))]
pub fn main() {
    use winit::event_loop::EventLoopBuilder;
    atomcad::start(EventLoopBuilder::new().build())
}

#[cfg(target_os = "android")]
pub fn main() {}

#[cfg(target_os = "macos")]
pub fn main() {
    use winit::event_loop::EventLoopBuilder;
    use winit::platform::macos::EventLoopBuilderExtMacOS;
    atomcad::start(EventLoopBuilder::new().with_default_menu(false).build())
}

// End of File
