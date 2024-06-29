// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::app::App;
use bevy::prelude::*;

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, _app: &mut App) {
        // This is where we would load our assets.
        // For now we'll just print a message.
        info!("Loading assets...");
    }
}

// End of File
