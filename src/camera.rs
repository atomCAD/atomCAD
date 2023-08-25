// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The 3D view camera in atomCAD is a user interface element that can be
//! interacted with.  Just like a slider widget can be dragged to change a
//! number, the 3D viewport can be clicked, dragged, scrolled, etc. to change
//! the orientation or focus of the viewport.
//!
//! This module implements that user interface processing logic, and exposes
//! an implementation of the [Camera](`render::Camera`) trait that translates
//! the camera's current state into parameters used by the rendering system.

use common::InputEvent;
use render::{Camera, CameraRepr};
use ultraviolet::{projection, Mat4, Vec3};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
};

const PI: f32 = std::f32::consts::PI;

// Make sure that the given value is between min and max, inclusive.  This is
// used to keep the user from rotating beyond the vertical.
//
// TODO: Move this to a common math module.
#[inline]
fn clamp(mut x: f32, min: f32, max: f32) -> f32 {
    assert!(min <= max);
    if x < min {
        x = min;
    }
    if x > max {
        x = max;
    }
    x
}

/// The arcball camera is the simplest camera type, used in the part editing
/// view.  It allows the user to rotate the camera around a focus point,
/// usually the center of the part or assembly being worked on, and zoom
/// in and out.
pub struct ArcballCamera {
    camera: CameraRepr,

    mouse_button_pressed: bool,
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    speed: f32,
}

impl ArcballCamera {
    pub fn new(focus: Vec3, distance: f32, speed: f32) -> Self {
        Self {
            camera: CameraRepr::default(),
            mouse_button_pressed: false,
            focus,
            yaw: 0.0,
            pitch: 0.0,
            distance,
            speed,
        }
    }

    fn add_yaw(&mut self, dyaw: f32) {
        self.yaw = (self.yaw + dyaw) % (PI * 2.0);
    }

    fn add_pitch(&mut self, dpitch: f32) {
        self.pitch = clamp(self.pitch + dpitch, (-PI / 2.0) + 0.001, (PI / 2.0) - 0.001);
    }

    fn position(&self) -> Vec3 {
        self.focus
            + self.distance
                * Vec3::new(
                    self.yaw.sin() * self.pitch.cos(),
                    self.yaw.cos() * self.pitch.cos(),
                    self.pitch.sin(),
                )
    }
}

impl Camera for ArcballCamera {
    fn resize(&mut self, aspect: f32, fov: f32, near: f32) {
        self.camera.projection =
            projection::perspective_reversed_infinite_z_wgpu_dx_gl(fov, aspect, near);
    }

    fn update(&mut self, event: InputEvent) -> bool {
        match event {
            InputEvent::Window(event) => match event {
                WindowEvent::MouseWheel { delta, .. } => {
                    match delta {
                        MouseScrollDelta::LineDelta(_, delta) => {
                            self.distance = (self.distance - delta * self.speed * 10.0).max(0.001);
                        }
                        MouseScrollDelta::PixelDelta(PhysicalPosition { y, .. }) => {
                            self.distance = (self.distance - y as f32 * self.speed).max(0.001);
                        }
                    }
                    true
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Left {
                        self.mouse_button_pressed = state == ElementState::Pressed;
                    }
                    false
                }
                _ => false,
            },
            InputEvent::Device(event) => match event {
                DeviceEvent::MouseMotion { delta: (x, y) } => {
                    if self.mouse_button_pressed {
                        self.add_yaw(x as f32 / 200.0);
                        self.add_pitch(y as f32 / 200.0);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            },
            InputEvent::BeginningFrame => false,
        }
    }

    fn finalize(&mut self) {
        self.camera.view = Mat4::look_at(self.position(), self.focus, Vec3::unit_z());
        self.camera.projection_view = self.camera.projection * self.camera.view;
    }

    fn repr(&self) -> CameraRepr {
        self.camera.clone()
    }

    fn get_ray_from(
        &self,
        pixel: &PhysicalPosition<f64>,
        viewport_size: &PhysicalSize<u32>,
    ) -> (Vec3, Vec3) {
        // 1. Convert the pixel position to normalized device coordinates.
        let x = (2.0 * pixel.x as f32 - viewport_size.width as f32) / viewport_size.width as f32;
        let y = (viewport_size.height as f32 - 2.0 * pixel.y as f32) / viewport_size.height as f32;

        // 2. Create a ray in clip space.
        let ray_clip = Vec3::new(x, y, -1.0); // The -1.0 assumes the near plane is at z=-1 in clip space

        // 3. Inverse project this ray from clip space to camera's view space.
        let proj_inv = self.camera.projection.inversed();
        let ray_eye = proj_inv.transform_vec3(ray_clip);

        // For the perspective projection, we need to flip the direction along the z-axis
        let ray_eye = Vec3::new(ray_eye.x, ray_eye.y, 1.0);

        // 4. Inverse transform this ray from the camera's view space to world space.
        let view_inv = self.camera.view.inversed();
        let ray_world = view_inv.transform_vec3(ray_eye);

        // Normalize the ray's direction
        let ray_dir = ray_world.normalized();

        (self.position(), ray_dir)
    }
}

// End of File
