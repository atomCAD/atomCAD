// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Bevy uses some very complex types for specifying system inputs.
// There's just no getting around this, so silence clippy's complaints.
#![allow(clippy::type_complexity)]

use bevy::app::App;
use bevy::prelude::*;

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, _app: &mut App) {}
}

// End of File
