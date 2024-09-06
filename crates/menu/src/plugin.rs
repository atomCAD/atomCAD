// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{Blueprint, attach_menubar_to_window};
use bevy::{ecs::system::NonSendMarker, prelude::*};
use std::sync::{Arc, Mutex};

pub struct MenubarPlugin {
    blueprint: Arc<Mutex<Option<Blueprint>>>,
}

impl MenubarPlugin {
    pub fn new(blueprint: Blueprint) -> Self {
        Self {
            blueprint: Arc::new(Mutex::new(Some(blueprint))),
        }
    }
}

impl Plugin for MenubarPlugin {
    fn build(&self, app: &mut App) {
        let blueprint = self.blueprint.lock().unwrap().take().unwrap_or_default();
        // Add blueprint as a resource
        app.insert_resource(blueprint);
        // Setup the menu bar, and attach it to the primary window (on Windows or X11), or to the
        // application itself (macOS).
        app.add_systems(Startup, setup_menu_bar);
    }
}

fn setup_menu_bar(
    // We have to use `NonSendMarker` here. This forces this function to be called from the main
    // thread (which is required on macOS). We don't actually use this marker, but we do need to
    // be in the main (event loop) thread in order to access the macOS APIs we need.
    _non_send_marker: NonSendMarker,
    blueprint: Res<Blueprint>,
) {
    // Do the platform-dependent work of constructing the menubar and attaching it to the
    // application object or main window.
    attach_menubar_to_window(&blueprint);
}

// End of File
