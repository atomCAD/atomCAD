// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use crate as menu;
use app::prelude::*;
use winit_runner::WinitEventLoopBuilder;

#[derive(Default)]
pub struct MenubarPlugin<T: 'static> {
    _phantom_user_event: core::marker::PhantomData<T>,
}

impl<T> app::Plugin for MenubarPlugin<T> {
    /// Not used.  Our registration code requires the [`WinitEventLoopBuilder`] to be present as a
    /// [`World`](ecs::prelude::World) resource, which is added when the
    /// [`WinitPlugin`](winit_runner::WinitPlugin) is registered.  However we can't be sure that the
    /// [`WinitPlugin`](winit_runner::WinitPlugin) will be registered before this plugin, so we
    /// delay registration of this plugin's components until [`initialize`](Plugin::initialize).
    fn register(&self, _app: &mut App) {}

    fn initialize(&self, app: &mut App) -> anyhow::Result<()> {
        let event_loop_builder = app
            .get_non_send_mut::<WinitEventLoopBuilder<T>>()
            .expect("Cannot find EventLoopBuilder in world")
            .into_inner();
        menu::platform_setup(event_loop_builder);
        Ok(())
    }
}

// End of File
