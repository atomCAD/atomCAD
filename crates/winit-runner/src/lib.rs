// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use anyhow::Context;
use app::prelude::*;

mod event_loop;
pub use event_loop::{WinitEventLoop, WinitEventLoopBuilder};

#[derive(Default)]
pub struct WinitPlugin<T: 'static> {
    _phantom_user_event: core::marker::PhantomData<T>,
}

impl<T> app::Plugin for WinitPlugin<T> {
    fn register(&self, app: &mut App) {
        app.insert_non_send(WinitEventLoopBuilder::<T>::default());
    }

    fn finalize(&self, app: &mut App) -> anyhow::Result<()> {
        let mut event_loop_builder = app
            .remove_non_send::<WinitEventLoopBuilder<T>>()
            .context("Cannot find WinitEventLoopBuilder in world")?;
        let event_loop = event_loop_builder
            .build()
            .context("Failed to create event loop manager.")?;
        app.insert_non_send(WinitEventLoop::<T>(event_loop));
        Ok(())
    }
}

// End of File
