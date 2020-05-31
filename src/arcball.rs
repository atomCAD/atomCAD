// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ultraviolet::{self, Mat4, Rotor3, Vec2, Vec3, Vec4};

#[derive(Debug)]
pub struct Arcball {
    center_pos: Vec3,
    camera_pos: Vec3,
    last_mouse: Option<Vec2>,
}

impl Arcball {
    pub fn new(center_pos: Vec3, camera_pos: Vec3) -> Self {
        Self {
            center_pos,
            camera_pos,
            last_mouse: None,
        }
    }

    pub fn rotate(&mut self, view_matrix: Mat4, new_pos: Vec2) -> Option<Rotor3> {
        self.last_mouse.replace(new_pos).map(|last_pos| {
            let sphere_radius = self.camera_pos.z - self.center_pos.z;

            let pos_on_sphere = |pos: Vec2| {
                let v = pos * sphere_radius;
                let vec = view_matrix * Vec4::new(v.x, v.y, 0.0, 0.0);

                let on_sphere = Vec3::new(
                    -vec.x,
                    -vec.y,
                    ((sphere_radius * sphere_radius) - (pos.x * pos.x) - (pos.y * pos.y)).sqrt(),
                );

                on_sphere.normalized()
            };

            let start_on_sphere = pos_on_sphere(last_pos);
            let end_on_sphere = pos_on_sphere(new_pos);

            Rotor3::from_rotation_between(start_on_sphere, end_on_sphere)
        })

        // self.last_mouse.replace(new_pos).map(|last_pos| {
        //     let sphere_radius = self.camera_pos.z - self.center_pos.z;

        //     // do raytrace to find point
        //     let pos_on_sphere = |pos: Point2<f32>| {
        //         let vec: Vector4<f32> =
        //             view_matrix * (pos.to_vec() * sphere_radius).extend(0.0).extend(0.0);

        //         let on_sphere = Vector2::new(-vec.x, -vec.y).extend(
        //             ((sphere_radius * sphere_radius) - (pos.x * pos.x) - (pos.y * pos.y)).sqrt(),
        //         );

        //         on_sphere.normalize()
        //     };

        //     let start_on_sphere = pos_on_sphere(last_pos);
        //     let end_on_sphere = pos_on_sphere(new_pos);

        //     (Quaternion::from_arc(start_on_sphere, end_on_sphere, None) * 10.0).normalize()
        // })
    }

    pub fn release(&mut self) {
        self.last_mouse = None;
    }
}

// End of File
