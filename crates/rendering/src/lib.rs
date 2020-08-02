// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

extern crate nalgebra as na;

#[macro_use]
mod macros;
mod camera;
mod encoder_wrapper;
mod compositor;
mod most_recent;
mod scene;

pub use self::compositor::Compositor;
pub use self::scene::{Resize, SceneEvent, SceneHandle};

use self::encoder_wrapper::{EncoderWrapper, Tripper};

pub trait Scene {
    type Event;
    type Error;

    fn update<I>(&mut self, events: I) -> Result<(), Self::Error>
        where I: Iterator<Item = Self::Event>;
    fn render(&self, encoder: &mut EncoderWrapper) -> Result<(), Self::Error>;
}
