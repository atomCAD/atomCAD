use crate::{
    InputEvent,
    utils::AsBytes,
    bind_groups::AsBindingResource,
};
use ultraviolet::{
    projection,
    Mat4, Vec3,
};
use winit::{
    dpi::{PhysicalSize, LogicalPosition},
    event::{WindowEvent, DeviceEvent, MouseScrollDelta, ElementState, MouseButton},
};
use wgpu::util::DeviceExt as _;

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

#[derive(Clone)]
#[repr(C)]
pub struct CameraRepr {
    pub projection: Mat4,
    pub view: Mat4,
    pub projection_view: Mat4,
}

unsafe impl AsBytes for CameraRepr {}

pub trait CameraImpl {
    fn resize(&mut self, aspect: f32, fov: f32, near: f32);
    fn update(&mut self, event: InputEvent) -> bool;
    fn finalize(&mut self);
    fn repr(&self) -> CameraRepr;
}

pub struct Camera {
    // Things related to on-gpu representation.
    uniform_buffer: wgpu::Buffer,

    fov: f32,
    near: f32,
    camera_impl: Box<dyn CameraImpl>,
    camera_was_updated: bool,
}

impl Camera {
    pub fn new(
        device: &wgpu::Device,
        size: PhysicalSize<u32>,
        fov: f32,
        near: f32,
    ) -> Self {
        // let camera = StaticCamera::new(fov, size.width as f32 / size.height as f32, near);
        let camera = ArcballCamera::new(size.width as f32 / size.height as f32, 100.0, 1.0, fov, near);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: camera.repr().as_bytes(),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        Self {
            uniform_buffer,

            fov,
            near,
            camera_impl: Box::new(camera),
            camera_was_updated: false,
        }
    }

    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        self.camera_was_updated = true;
    }

    pub fn set_near(&mut self, near: f32) {
        self.near = near;
        self.camera_was_updated = true;
    }

    pub fn update(&mut self, event: InputEvent) {
        if let InputEvent::Window(WindowEvent::Resized(size)) = event {
            self.camera_impl.resize(size.width as f32 / size.height as f32, self.fov, self.near);
            self.camera_was_updated = true;
        } else {
            self.camera_was_updated |= self.camera_impl.update(event);
        }
        
    }

    pub fn finalize(&mut self, queue: &wgpu::Queue) {
        self.camera_impl.finalize();
        if self.camera_was_updated {
            self.camera_was_updated = false;
            queue.write_buffer(&self.uniform_buffer, 0, self.camera_impl.repr().as_bytes());
        }
    }
}

impl AsBindingResource for Camera {
    fn as_binding_resource(&self) -> wgpu::BindingResource {
        wgpu::BindingResource::Buffer {
            buffer: &self.uniform_buffer,
            offset: 0,
            size: None,
        }
    }
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
    pub fn new(aspect: f32, distance: f32, speed: f32, fov: f32, near: f32) -> Self {
        let eye = distance * Vec3::unit_z();

        let projection = projection::perspective_reversed_infinite_z_wgpu_dx_gl(fov, aspect, near);
        let view = Mat4::look_at(eye, Vec3::zero(), Vec3::unit_y());
        let projection_view = projection * view;

        Self {
            camera: CameraRepr {
                projection,
                view,
                projection_view,
            },

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

impl CameraImpl for ArcballCamera {
    fn resize(&mut self, aspect: f32, fov: f32, near: f32) {
        self.camera.projection = projection::perspective_reversed_infinite_z_wgpu_dx_gl(fov, aspect, near);
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
                    if button == MouseButton::Left {
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
                _ => false
            }
            InputEvent::Device(event) => match event {
                DeviceEvent::MouseMotion { delta: (x, y) } => {
                    if self.mouse_button_pressed {
                        self.add_yaw(-x as f32 / 200.0);
                        self.add_pitch(y as f32 / 200.0);
                        true
                    } else {
                        false
                    }
                }
                _ => false
            }
        }
    }

    fn finalize(&mut self) {
        let eye = self.distance * Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );

        // let eye = self.rotor * (self.distance * Vec3::unit_z());
        self.camera.view = Mat4::look_at(eye, Vec3::zero(), Vec3::unit_y());
        self.camera.projection_view = self.camera.projection * self.camera.view;
    }

    fn repr(&self) -> CameraRepr {
        self.camera.clone()
    }
}
