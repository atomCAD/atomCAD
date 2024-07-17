// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::{prelude::*, window::PrimaryWindow};

/// Updates the canvas size to match the browser window size.  As the user resizes the browser
/// window, the canvas element also needs to be resized to match.
pub(crate) fn tweak_bevy_app(app: &mut App) {
    // Record the size of the canvas element as a persistent entity.
    app.world_mut().spawn(CanvasSize::default());
    // Every frame, check the size of the browser window.
    // If necessary, update the canvas size to match.
    app.add_systems(Update, update_canvas_size);
}

/// Stores the browser window's inner frame size as an entity which persiste from frame to frame.
#[derive(Component, PartialEq)]
struct CanvasSize {
    width: f32,
    height: f32,
}

impl Default for CanvasSize {
    fn default() -> Self {
        CanvasSize {
            width: -1.0, // -1.0 indicates uninitialized state
            height: -1.0,
        }
    }
}

/// Every frame, check the browser inner-fame size, and compare that against the persisted
/// [`CanvasSize`] entity.
fn update_canvas_size(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut canvas_size: Query<&mut CanvasSize>,
) {
    // Using a lambda to in order to be able to use the `?` operator here is a bit of a hack, but it
    // makes the code a heck of a lot clearer to read.  At some point this should be refactored to
    // produce an error type, which can be piped into a chained error handler, so we at least
    // capture reasons why this might fail in the wild.
    let _ = (|| {
        // The CanvasSize entity stores the size of the canvas element as of the last frame (or -1
        // if this is the first time through).
        let mut canvas_size = canvas_size.single_mut().ok()?;
        // The winit window, which handles the resizing logic.
        let mut window = window.single_mut().ok()?;
        // The browser window, which is not provided by bevy or winit, but which can be accessed
        // from the web_sys crate, provides our ground truth for the current inner-frame size of the
        // browser window.
        let browser_window = web_sys::window()?;
        // The browser window size is reported as Javascript numeric floating point pixels, so we
        // need to convert to f32 while respecting the previously configured window resizing
        // constraints.
        //
        // Window resize constraints aren't as useful in a web context as they are in native, as we
        // really can't control the size of the browser window.  But if the user resizes the browser
        // window to be smaller than the resize constraints, they will see the window start to clip
        // the scene, rather than the UI elements getting jumbled together.
        let width = (browser_window.inner_width().ok()?.as_f64()? as f32).clamp(
            window.resize_constraints.min_width,
            window.resize_constraints.max_width,
        );
        let height = (browser_window.inner_height().ok()?.as_f64()? as f32).clamp(
            window.resize_constraints.min_height,
            window.resize_constraints.max_height,
        );
        // Only if the browser window size has changed do we need to update the canvas size.
        // Otherwise we would be needlessly triggering bevy's resize logic, which kills performance
        // by reallocating framebuffers every single frame.
        if width != canvas_size.width || height != canvas_size.height {
            window.resolution.set(width, height);
            canvas_size.width = width;
            canvas_size.height = height;
        }
        // Result is thrown away.
        Some(())
    })();
}

// End of File
