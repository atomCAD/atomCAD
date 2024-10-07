// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use ecs::prelude::*;
use std::ops::{Deref, DerefMut};
use winit::event_loop::{EventLoop, EventLoopBuilder};

#[derive(Resource)]
pub struct WinitEventLoopBuilder<T: 'static>(pub EventLoopBuilder<T>);

impl<T> Default for WinitEventLoopBuilder<T> {
    fn default() -> Self {
        Self(EventLoop::with_user_event())
    }
}

impl<T> Deref for WinitEventLoopBuilder<T> {
    type Target = EventLoopBuilder<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for WinitEventLoopBuilder<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct WinitEventLoop<T: 'static>(pub EventLoop<T>);

impl<T> Deref for WinitEventLoop<T> {
    type Target = EventLoop<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for WinitEventLoop<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// End of File
