use common::InputEvent;
use render::{Camera, CameraRepr};
use ultraviolet::{projection, Mat4, Vec3};
use winit::{
    dpi::LogicalPosition,
    event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
};

const PI: f32 = std::f32::consts::PI;

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

pub struct ArcballCamera {
    camera: CameraRepr,

    mouse_button_pressed: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    speed: f32,
}

impl ArcballCamera {
    pub fn new(distance: f32, speed: f32) -> Self {
        Self {
            camera: CameraRepr::default(),
            mouse_button_pressed: false,
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
                            self.distance = (self.distance - delta * self.speed).max(0.001);
                        }
                        MouseScrollDelta::PixelDelta(LogicalPosition { y, .. }) => {
                            self.distance = (self.distance - y as f32 * self.speed).max(0.001);
                        }
                    }
                    true
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Right {
                        if state == ElementState::Pressed {
                            self.mouse_button_pressed = true;
                        } else {
                            self.mouse_button_pressed = false;
                        }
                    }
                    false
                }
                WindowEvent::CursorLeft { .. } => {
                    self.mouse_button_pressed = false;
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
        }
    }

    fn finalize(&mut self) {
        let eye = self.distance
            * Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.yaw.cos() * self.pitch.cos(),
                self.pitch.sin(),
            );

        // let eye = self.rotor * (self.distance * Vec3::unit_z());
        self.camera.view = Mat4::look_at(eye, Vec3::zero(), Vec3::unit_z());
        self.camera.projection_view = self.camera.projection * self.camera.view;
    }

    fn repr(&self) -> CameraRepr {
        self.camera.clone()
    }
}
