// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::{Mutex, OnceLock};

use bevy::{
    prelude::*, winit::EventLoopProxy, winit::EventLoopProxyWrapper, winit::WinitUserEvent,
};

pub fn setup_ctrlc_handler() {
    crate::platform_impl::setup_ctrlc_handler()
}

pub static EVENT_LOOP_PROXY: EventLoopWaker = EventLoopWaker(OnceLock::new());
pub struct EventLoopWaker(OnceLock<Mutex<EventLoopProxy<WinitUserEvent>>>);

impl EventLoopWaker {
    pub fn wake_event_loop(&self) {
        let Some(mutex) = self.0.get() else {
            warn!("Event loop waker not initialized.");
            return;
        };
        let Ok(proxy) = mutex.lock() else {
            warn!("Failed to lock event loop waker.");
            return;
        };
        if let Err(e) = proxy.send_event(WinitUserEvent::WakeUp) {
            warn!("Failed to send wake up event to event loop: {e}");
        }
    }

    fn init(proxy: Res<EventLoopProxyWrapper>) {
        let proxy: &EventLoopProxy<WinitUserEvent> = &proxy;
        EVENT_LOOP_PROXY
            .0
            .get_or_init(|| std::sync::Mutex::new(proxy.clone()));
    }
}

pub struct EventLoopWakerPlugin;

impl Plugin for EventLoopWakerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, EventLoopWaker::init.chain());
    }
}

// End of File
