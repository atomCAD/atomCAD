// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{camera::Camera, command_encoder::CommandEncoder};
use anyhow::Result;
use bytemuck;
use futures::{executor::LocalSpawner, future::FutureExt, task::SpawnExt};
use na::{Matrix3, Matrix4, Vector3};
use winit::{
    dpi::{LogicalPosition, PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta},
};

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

mod billboards;
mod event;
mod handle;
mod uniform;
// mod filter;

use billboards::Billboards;
pub use event::{Event, Resize};
pub use handle::SceneHandle;

const VIEWPORT_MATRIX: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.5, 0.0],
    [0.0, 0.0, 0.5, 1.0],
];

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
    world_mx: Matrix4<f32>,
    arcball_camera: Camera,

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
        cmd_encoder: &mut CommandEncoder,
        events: Vec<Event>,
        resize: Option<Resize>,
        spawner: &LocalSpawner,
    ) -> Result<()> {
        let viewport_matrix: Matrix4<f32> = VIEWPORT_MATRIX.into();

        if let Some(Resize { new_texture, size }) = resize {
            self.resize(&device, new_texture, size);
        }

        self.process_events(events.into_iter());

        self.rotate_with_arcball();

        // Update the world matrix in case it's changed.
        let projection_matrix =
            generate_projection_matrix(self.size.width as f32 / self.size.height as f32);

        self.world_mx = viewport_matrix * projection_matrix * self.arcball_camera.get_camera();

        {
            let inv_camera = self.arcball_camera.get_inv_camera();
            self.billboards.update(
                queue,
                self.world_mx,
                projection_matrix,
                Matrix3::from_columns(&[
                    inv_camera.column(0).xyz(),
                    inv_camera.column(1).xyz(),
                    inv_camera.column(2).xyz(),
                ]),
                self.state
                    .mouse
                    .cursor
                    .unwrap_or((u32::max_value(), u32::max_value()).into()),
            );
        }

        self.draw(cmd_encoder);

        if let Some(cursor_pos) = self.state.mouse.cursor {
            let mouseover_id =
                self.billboards
                    .get_mouseover_id(&device, cmd_encoder, cursor_pos);
            spawner
                .spawn(mouseover_id.map(|id| {
                    println!("async mouseover id: {:?}", id);
                }))
                .expect("unable to spawn mouseover future");
        }

        Ok(())
    }

    fn rotate_with_arcball(&mut self) {
        if let Mouse {
            old_cursor: Some(old_cursor),
            cursor: Some(new_cursor),
            right_button: ElementState::Pressed,
            ..
        } = self.state.mouse
        {
            self.arcball_camera.rotate(old_cursor, new_cursor);
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
                        self.arcball_camera.zoom(y as f32 / 100.0);
                    }

                    match delta {
                        MouseScrollDelta::PixelDelta(LogicalPosition { y, .. }) => {
                            self.arcball_camera.zoom(y as f32 / 100.0);
                        }
                        MouseScrollDelta::LineDelta(_, y) => {
                            self.arcball_camera.zoom(y * 20.0);
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

        self.arcball_camera.resize(size);

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

fn generate_projection_matrix(aspect_ratio: f32) -> Matrix4<f32> {
    Matrix4::new_perspective(aspect_ratio, 45.0_f32.to_radians(), 1.0, 3000.0)
}

fn create_scene(device: &wgpu::Device, size: PhysicalSize<u32>) -> Scene {
    let viewport_matrix: Matrix4<f32> = VIEWPORT_MATRIX.into();

    let mut arcball_camera = Camera::new(Vector3::zeros(), 1.0, size);

    arcball_camera.zoom(-1000.0);

    let world_mx = viewport_matrix
        * generate_projection_matrix(size.width as f32 / size.height as f32)
        * arcball_camera.get_camera();

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
