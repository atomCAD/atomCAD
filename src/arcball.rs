// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// use ultraviolet::{self, projection::perspective_gl, Bivec3, Isometry3, Mat4, Rotor3, Vec2, Vec3};

use cgmath::{num_traits::clamp, perspective, Deg, InnerSpace as _, One as _};
use winit::dpi::{PhysicalPosition, PhysicalSize};

use crate::math::{Mat4, Point2, Point3, Quaternion, Vec2, Vec3};

/// Create a rotor that specifies the rotation between
/// the old cursor position and the new cursor position.
///
/// `old_cursor` and `new_cursor` are scaled between -1 and 1 in each dimension.
///
/// The content of this function is based on https://www.khronos.org/opengl/wiki/Object_Mouse_Trackball.
fn create_rotor(old_cursor: Vec2, new_cursor: Vec2) -> Quaternion {
    let r: f32 = 1.0; // Recheck this.

    // Cast the mouse position onto a piecewise function that is
    // either a sphere or a hyperbolic sheet depending on distance
    // from the origin.
    let z = |pos: Vec2| {
        if pos.magnitude2() <= r.powi(2) / 2.0 {
            (r.powi(2) - pos.magnitude2()).sqrt()
        } else {
            (r.powi(2) / 2.0) / pos.magnitude()
        }
    };

    let project = |pos: Vec2| Vec3::new(pos.x, pos.y, z(pos)).normalize();

    Quaternion::from_arc(project(old_cursor), project(new_cursor), None)

    // Rotor3::from_rotation_between(project(old_cursor), project(new_cursor))
}

pub struct ArcballCamera {
    camera: Point3,
    rotation: Quaternion,
    size: PhysicalSize<u32>,
}

impl ArcballCamera {
    pub fn new(camera: Point3, size: PhysicalSize<u32>) -> Self {
        Self {
            camera,
            rotation: Quaternion::one(),
            size,
        }
    }

    pub fn view_matrix(&self) -> Mat4 {
        // let rotation = Isometry3::new(Vec3::zero(), self.rotor).into_homogeneous_matrix();
        let rotation: Mat4 = self.rotation.into();
        self.generate_matrix() * rotation
    }

    pub fn rotate(&mut self, old_cursor: PhysicalPosition<u32>, new_cursor: PhysicalPosition<u32>) {
    }

    // pub fn rotate(&mut self, old_cursor: PhysicalPosition<u32>, new_cursor: PhysicalPosition<u32>) {
    //     let m_cur = Vec2::new(
    //         new_cursor.x as f32 * self.inverse_size.width * 2.0 - 1.0,
    //         1.0 - new_cursor.y as f32 * self.inverse_size.height * 2.0,
    //     )
    //     .clamped(Vec2::broadcast(-1.0), Vec2::broadcast(1.0));

    //     let m_prev = Vec2::new(
    //         old_cursor.x as f32 * self.inverse_size.width * 2.0 - 1.0,
    //         1.0 - old_cursor.y as f32 * self.inverse_size.height * 2.0,
    //     )
    //     .clamped(Vec2::broadcast(-1.0), Vec2::broadcast(1.0));

    //     // let scale = |cursor: PhysicalPosition<u32>| {
    //     //     Vec2::new(
    //     //         // scale pixel coordinates to [0, 2]
    //     //         cursor.x as f32 / (self.size.width as f32 / 2.0) - 1.0,
    //     //         1.0 - cursor.y as f32 / (self.size.height as f32 / 2.0),
    //     //     )
    //     // };

    //     // let old_cursor = scale(old_cursor);
    //     // let new_cursor = scale(new_cursor);

    //     // let cursor_ball_old = Self::screen_to_arcball(old_cursor);
    //     // let cursor_ball = Self::screen_to_arcball(new_cursor);

    //     // self.rotor = cursor_ball * cursor_ball_old * self.rotor;

    //     self.rotation = create_rotor(old_cursor, new_cursor) * self.rotation;
    // }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
    }

    // fn screen_to_arcball(p: Vec2) -> Rotor3 {
    //     let dist = p.mag();

    //     if dist <= 1.0 {
    //         Rotor3::new(0.0, Bivec3::new(p.x, p.y, (1.0 - dist).sqrt()))
    //     } else {
    //         let unit_p = p.normalized();
    //         Rotor3::new(0.0, Bivec3::new(unit_p.x, unit_p.y, 0.0))
    //     }
    // }

    fn generate_matrix(&self) -> Mat4 {
        let opengl_to_wgpu_matrix: Mat4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 0.5, 0.0],
            [0.0, 0.0, 0.5, 1.0],
        ]
        .into();

        let aspect_ratio = self.size.width as f32 / self.size.height as f32;

        let mx_projection = perspective(Deg(45.0), aspect_ratio, 1.0, 10.0);
        let mx_view = Mat4::look_at(self.camera, Point3::new(0.0, 0.0, 0.0), Vec3::unit_z());

        opengl_to_wgpu_matrix * mx_projection * mx_view
    }
}
