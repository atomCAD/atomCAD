// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[cfg(not(any(target_os = "android")))]
pub fn main() {
    use winit::event_loop::EventLoopBuilder;
    atomcad::start(&mut EventLoopBuilder::new())
}

#[cfg(target_os = "android")]
pub fn main() {}

// End of File
