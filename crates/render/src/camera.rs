use crate::{bind_groups::AsBindingResource};
use common::{InputEvent, AsBytes};
use std::mem;
use ultraviolet::{projection, Mat4, Vec3};
use wgpu::util::DeviceExt as _;
use winit::{
    dpi::{LogicalPosition, PhysicalSize},
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

#[derive(Clone, Default)]
#[repr(C)]
pub struct CameraRepr {
    pub projection: Mat4,
    pub view: Mat4,
    pub projection_view: Mat4,
}

unsafe impl AsBytes for CameraRepr {}

pub trait Camera {
    fn resize(&mut self, aspect: f32, fov: f32, near: f32);
    fn update(&mut self, event: InputEvent) -> bool;
    fn finalize(&mut self);
    fn repr(&self) -> CameraRepr;
}

pub struct RenderCamera {
    // Things related to on-gpu representation.
    uniform_buffer: wgpu::Buffer,

    fov: f32,
    near: f32,
    camera: Option<Box<dyn Camera>>,
    camera_was_updated: bool,
}

impl RenderCamera {
    pub fn new(device: &wgpu::Device, size: PhysicalSize<u32>, fov: f32, near: f32) -> Self {
        // let camera = StaticCamera::new(fov, size.width as f32 / size.height as f32, near);
        // let camera = ArcballCamera::new(
        //     size.width as f32 / size.height as f32,
        //     100.0,
        //     1.0,
        //     fov,
        //     near,
        // );
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: mem::size_of::<CameraRepr>() as u64,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            uniform_buffer,

            fov,
            near,
            camera: None,
            camera_was_updated: false,
        }
    }

    pub(crate) fn set_camera<C: Camera + 'static>(&mut self, mut camera: C, size: PhysicalSize<u32>) {
        camera.resize(size.width as f32 / size.height as f32, self.fov, self.near);
        self.camera = Some(Box::new(camera));
        self.camera_was_updated = true;
    }

    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        self.camera_was_updated = true;
    }

    pub fn set_near(&mut self, near: f32) {
        self.near = near;
        self.camera_was_updated = true;
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if let Some(camera) = self.camera.as_mut() {
            camera.resize(new_size.width as f32 / new_size.height as f32, self.fov, self.near);
            self.camera_was_updated = true;
        }
        // let mut camera = self.camera;
        // camera.as_mut().map(|camera| {
        //     camera.resize(new_size.width as f32 / new_size.height as f32, self.fov, self.near);
        //     self.camera_was_updated = true;
        // });
        // self.camera_impl
        //     .resize(new_size.width as f32 / new_size.height as f32, self.fov, self.near);
        
    }

    pub fn update(&mut self, event: InputEvent) {
        if let Some(camera) = self.camera.as_mut() {
            self.camera_was_updated |= camera.update(event);
        }
        // self.camera.map(|camera| {
        //     self.camera_was_updated |= camera.update(event);
        // });
    }

    #[must_use = "returns bool indicating whether a camera is currently set or not"]
    pub fn upload(&mut self, queue: &wgpu::Queue) -> bool {
        if let Some(camera) = self.camera.as_mut() {
            camera.finalize();
            if self.camera_was_updated {
                queue.write_buffer(&self.uniform_buffer, 0, camera.repr().as_bytes());
            }
            self.camera_was_updated = false;
        }
        self.camera.is_some()

        // self.camera.map(|camera| {
        //     camera.finalize();
        //     if self.camera_was_updated {
        //         queue.write_buffer(&self.uniform_buffer, 0, camera.repr().as_bytes());
        //     }
        //     self.camera_was_updated = false;
        // }).is_some()
        // self.camera.map(|camera| camera.finalize());
        // if self.camera_was_updated {
        //     self.camera_was_updated = false;
        //     queue.write_buffer(&self.uniform_buffer, 0, self.camera_impl.repr().as_bytes());
        // }
    }
}

impl AsBindingResource for RenderCamera {
    fn as_binding_resource(&self) -> wgpu::BindingResource {
        wgpu::BindingResource::Buffer {
            buffer: &self.uniform_buffer,
            offset: 0,
            size: None,
        }
    }
}
