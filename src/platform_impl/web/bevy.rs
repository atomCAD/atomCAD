// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use web_sys;

pub struct PlatformTweaks;

// Updates the canvas size to match the browser window size.  As the user
// resizes the browser window, the canvas element also needs to be resized to
// match.
impl Plugin for PlatformTweaks {
    fn build(&self, app: &mut App) {
        app.world.spawn(CanvasSize::default());
        app.add_systems(Update, update_canvas_size);
    }
}

#[derive(Component, PartialEq)]
struct CanvasSize {
    width: f32,
    height: f32,
}

impl Default for CanvasSize {
    fn default() -> Self {
        CanvasSize {
            width: -1.0,
            height: -1.0,
        }
    }
}

fn update_canvas_size(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut canvas_size: Query<&mut CanvasSize>,
) {
    (|| {
        let mut canvas_size = canvas_size.get_single_mut().ok()?;
        let mut window = window.get_single_mut().ok()?;
        let browser_window = web_sys::window()?;
        let width = browser_window.inner_width().ok()?.as_f64()? as f32;
        let height = browser_window.inner_height().ok()?.as_f64()? as f32;
        if width != canvas_size.width || height != canvas_size.height {
            window.resolution.set(width, height);
            canvas_size.width = width;
            canvas_size.height = height;
        }
        Some(())
    })();
}

// End of File
