// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ultraviolet::{self, Rotor3, Vec2, Vec3};

/// Create a rotor that specifies the rotation between
/// the old cursor position and the new cursor position.
///
/// `old_cursor` and `new_cursor` are scaled between -1 and 1 in each dimension.
///
/// The content of this function is based on https://www.khronos.org/opengl/wiki/Object_Mouse_Trackball.
pub fn create_rotor(old_cursor: Vec2, new_cursor: Vec2) -> Rotor3 {
    let r: f32 = 1.0; // Recheck this.

    // Cast the mouse position onto a piecewise function that is
    // either a sphere or a hyperbolic sheet depending on distance
    // from the origin.
    let z = |pos: Vec2| {
        if pos.mag_sq() <= r.powi(2) / 2.0 {
            (r.powi(2) - pos.mag_sq()).sqrt()
        } else {
            (r.powi(2) / 2.0) / pos.mag()
        }
    };

    let project = |pos: Vec2| Vec3::new(pos.x, pos.y, z(pos)).normalized();

    Rotor3::from_rotation_between(project(old_cursor), project(new_cursor))
}
