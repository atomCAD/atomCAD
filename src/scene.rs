// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use anyhow::Result;
use bytemuck;
// use ultraviolet::{projection::perspective_gl, Isometry3, Mat4, Vec2, Vec3};
use arcball::ArcballCamera;
use cgmath::{perspective, Deg};
use winit::{
    dpi::{LogicalPosition, PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta},
};

use crate::math::{Mat3, Mat4, Vec2, Vec3, Vec4};

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const VIEWPORT_MATRIX: Mat4 = Mat4::from_cols(
    Vec4::new(1.0, 0.0, 0.0, 0.0),
    Vec4::new(0.0, 1.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 0.5, 0.0),
    Vec4::new(0.0, 0.0, 0.5, 1.0),
);

mod billboards;
mod event;
mod handle;
mod uniform;
// mod filter;

use billboards::Billboards;
pub use event::{Event, Resize};
pub use handle::SceneHandle;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

#[derive(Debug)]
struct Mouse {
    pub old_cursor: Option<PhysicalPosition<u32>>,
    pub cursor: Option<PhysicalPosition<u32>>,
    pub left_button: ElementState,
    pub right_button: ElementState,
}

#[derive(Debug)]
struct State {
    pub mouse: Mouse,
}

struct Scene {
    render_texture: wgpu::Texture,

    size: PhysicalSize<u32>,
    world_mx: Mat4,
    arcball_camera: ArcballCamera<f32>,

    billboards: Billboards,

    state: State,
}

impl Scene {
    fn new(device: &wgpu::Device, size: PhysicalSize<u32>) -> Self {
        create_scene(device, size)
    }

    /// This is called for every frame.
    fn render_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        events: Vec<Event>,
        resize: Option<Resize>,
    ) -> Result<wgpu::CommandBuffer> {
        let mut cmd_encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        if let Some(Resize { new_texture, size }) = resize {
            self.resize(&device, new_texture, size);
        }

        self.process_events(events.into_iter());

        self.rotate_with_arcball();

        // Update the world matrix in case it's changed.
        let projection_matrix =
            generate_projection_matrix(self.size.width as f32 / self.size.height as f32);

        self.world_mx = VIEWPORT_MATRIX * projection_matrix * self.arcball_camera.get_mat4();

        {
            let inv_camera = self.arcball_camera.get_inv_camera();
            self.billboards.update(
                queue,
                self.world_mx,
                projection_matrix,
                Mat3::from_cols(
                    inv_camera.x.truncate(),
                    inv_camera.y.truncate(),
                    inv_camera.z.truncate(),
                ),
            );
        }

        self.draw(&mut cmd_encoder);

        Ok(cmd_encoder.finish())
    }

    fn rotate_with_arcball(&mut self) {
        if let Mouse {
            old_cursor: Some(old_cursor),
            cursor: Some(new_cursor),
            right_button: ElementState::Pressed,
            ..
        } = self.state.mouse
        {
            let convert = |pos: PhysicalPosition<u32>| Vec2::new(pos.x as f32, pos.y as f32);

            self.arcball_camera
                .rotate(convert(old_cursor), convert(new_cursor));
        }
    }

    fn process_events(&mut self, events: impl Iterator<Item = Event>) {
        let mut cursor_left = false;

        self.state.mouse.old_cursor = self.state.mouse.cursor.take();

        for event in events {
            match event {
                Event::MouseInput { button, state } => match button {
                    MouseButton::Left => {
                        self.state.mouse.left_button = state;
                    }
                    MouseButton::Right => {
                        self.state.mouse.right_button = state;
                    }
                    _ => {}
                },
                Event::CursorMoved { new_pos } => {
                    // This event can be fired several times during a single frame.
                    self.state.mouse.cursor = Some(new_pos);
                }
                Event::CursorLeft => {
                    cursor_left = true;
                }
                Event::Zoom { delta, .. } => {
                    if let MouseScrollDelta::PixelDelta(LogicalPosition { y, .. }) = delta {
                        self.arcball_camera.zoom(y as f32 / 100.0, 1.0);
                    }

                    match delta {
                        MouseScrollDelta::PixelDelta(LogicalPosition { y, .. }) => {
                            self.arcball_camera.zoom(y as f32 / 100.0, 1.0);
                        }
                        MouseScrollDelta::LineDelta(_, y) => {
                            self.arcball_camera.zoom(y, 1.0);
                        }
                    }
                }
            }
        }

        if cursor_left {
            self.state.mouse.old_cursor = None;
            self.state.mouse.cursor = None;
        }
    }

    fn resize(
        &mut self,
        device: &wgpu::Device,
        render_texture: wgpu::Texture,
        size: PhysicalSize<u32>,
    ) {
        self.render_texture = render_texture;
        self.size = size;

        self.arcball_camera
            .update_screen(size.width as f32, size.height as f32);

        self.billboards.resize(device, size);
    }

    fn draw(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let render_view = self.render_texture.create_default_view();

        self.billboards.draw(encoder, render_view);
    }
}

impl State {
    pub fn new() -> Self {
        State {
            mouse: Mouse {
                old_cursor: None,
                cursor: None,
                left_button: ElementState::Released,
                right_button: ElementState::Released,
            },
        }
    }
}

fn generate_projection_matrix(aspect_ratio: f32) -> Mat4 {
    perspective(Deg(45.0), aspect_ratio, 1.0, 2000.0)
}

fn create_scene(device: &wgpu::Device, size: PhysicalSize<u32>) -> Scene {
    let mut arcball_camera = ArcballCamera::new(
        Vec3::new(0.0, 0.0, 0.0),
        1.0,
        [size.width as f32, size.height as f32],
    );

    arcball_camera.zoom(-10.0, 1.0);

    let world_mx = VIEWPORT_MATRIX
        * generate_projection_matrix(size.width as f32 / size.height as f32)
        * arcball_camera.get_mat4();

    let billboards = Billboards::new(device, size);

    // The scene renders to this texture.
    // The main (UI) thread has a view of this texture and copies
    // from it at 60fps.
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEFAULT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
        label: if cfg!(build = "debug") {
            Some("scene render texture")
        } else {
            None
        },
    });

    Scene {
        render_texture,

        size,
        world_mx,
        arcball_camera,

        billboards,

        state: State::new(),
    }
}
