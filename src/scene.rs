// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use anyhow::Result;
use bytemuck;
use std::mem;
use ultraviolet::{projection::perspective_gl, Isometry3, Mat4, Vec2, Vec3};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton},
};

use crate::arcball;

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
/// Normal as in perpendicular, not usual.
const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

mod event;
mod handle;
mod isosphere;
mod scene_impl;

pub use event::{Event, Resize};
pub use handle::SceneHandle;
use isosphere::IsoSphere;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

/// Temporary?
#[derive(Debug)]
struct Entity {
    vertex_buffer: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,

    vertex_num: usize,
}

#[derive(Debug)]
struct Mouse {
    pub old_cursor: Option<PhysicalPosition<u32>>,
    pub cursor: Option<PhysicalPosition<u32>>,
    pub left_button: ElementState,
}

#[derive(Debug)]
struct State {
    pub mouse: Mouse,
}

struct Scene {
    global_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    render_texture: wgpu::Texture,
    normals_fbo: wgpu::Texture,

    size: PhysicalSize<u32>,
    world_mx: Mat4,
    camera: Vec3,

    icosphere: Entity,

    state: State,
}

impl Scene {
    /// This is called for every frame.
    fn render_frame(
        &mut self,
        device: &wgpu::Device,
        events: Vec<Event>,
        resize: Option<Resize>,
    ) -> Result<wgpu::CommandBuffer> {
        let mut cmd_encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        if let Some(Resize { new_texture, size }) = resize {
            self.resize(&device, &mut cmd_encoder, new_texture, size);
        }

        self.process_events(&device, &mut cmd_encoder, events.into_iter());

        self.rotate_with_arcball(device, &mut cmd_encoder);

        self.draw(&mut cmd_encoder);

        Ok(cmd_encoder.finish())
    }

    fn rotate_with_arcball(
        &mut self,
        device: &wgpu::Device,
        cmd_encoder: &mut wgpu::CommandEncoder,
    ) {
        if let Mouse {
            old_cursor: Some(old_cursor),
            cursor: Some(new_cursor),
            left_button: ElementState::Pressed,
            ..
        } = self.state.mouse
        {
            let scale = |cursor: PhysicalPosition<u32>| {
                Vec2::new( // scale pixel coordinates to [0, 2]
                    cursor.x as f32 / (self.size.width as f32 / 2.0),
                    cursor.y as f32 / (self.size.height as f32 / 2.0),
                )
                .map(|i| i - 1.0) // scale from [0, 2] to [-1, 1]
                    * Vec2::new(-1.0, 1.0)
            };

            let old_cursor = scale(old_cursor);
            let new_cursor = scale(new_cursor);

            let rotor = arcball::create_rotor(old_cursor, new_cursor);
            let rotation = Isometry3::new(Vec3::zero(), rotor).into_homogeneous_matrix();
            self.world_mx = self.world_mx * rotation;

            upload_matrix(&device, cmd_encoder, &self.uniform_buffer, self.world_mx);
        }
    }

    fn process_events(
        &mut self,
        _device: &wgpu::Device,
        _encoder: &mut wgpu::CommandEncoder,
        events: impl Iterator<Item = Event>,
    ) {
        let mut cursor_left = false;

        self.state.mouse.old_cursor = self.state.mouse.cursor.take();

        for event in events {
            match event {
                Event::MouseInput { button, state } => match button {
                    MouseButton::Left => {
                        self.state.mouse.left_button = state;
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
        encoder: &mut wgpu::CommandEncoder,
        render_texture: wgpu::Texture,
        size: PhysicalSize<u32>,
    ) {
        self.world_mx = generate_matrix(self.camera, size.width as f32 / size.height as f32);

        upload_matrix(device, encoder, &self.uniform_buffer, self.world_mx);

        self.normals_fbo = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: NORMAL_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            label: if cfg!(build = "debug") {
                Some("scene normal texture")
            } else {
                None
            },
        });

        self.render_texture = render_texture;
        self.size = size;
    }

    fn draw(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let render_view = self.render_texture.create_default_view();
        let normals_view = self.normals_fbo.create_default_view();

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &render_view,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::WHITE,
                    },
                    wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &normals_view,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::TRANSPARENT,
                    },
                ],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.icosphere.render_pipeline);
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);
            // render_pass.set_bind_group(1, &self.icosphere.bind_group, &[]);
            render_pass.set_vertex_buffer(0, &self.icosphere.vertex_buffer, 0, 0);
            // render_pass.set_bind_group(index, bind_group, offsets)
            render_pass.draw(0..self.icosphere.vertex_num as u32, 0..1);
        }

        {
            // let mut compute_pass = encoder.begin_compute_pass();
        }
    }
}

fn generate_matrix(camera: Vec3, aspect_ratio: f32) -> Mat4 {
    let opengl_to_wgpu_matrix = Mat4::new(
        [1.0, 0.0, 0.0, 0.0].into(),
        [0.0, 1.0, 0.0, 0.0].into(),
        [0.0, 0.0, 0.5, 0.0].into(),
        [0.0, 0.0, 0.5, 1.0].into(),
    );

    let mx_projection = perspective_gl(45_f32.to_radians(), aspect_ratio, 1.0, 10.0);
    let mx_view = Mat4::look_at(camera, Vec3::zero(), Vec3::unit_z());

    opengl_to_wgpu_matrix * mx_projection * mx_view
}

/// TODO: Replace with `queue.write_buffer`.
fn upload_matrix(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    uniform: &wgpu::Buffer,
    mx: Mat4,
) {
    let matrix_src =
        device.create_buffer_with_data(mx.as_byte_slice(), wgpu::BufferUsage::COPY_SRC);

    encoder.copy_buffer_to_buffer(&matrix_src, 0, &uniform, 0, mem::size_of_val(&mx) as u64);
}

/// TODO: This is temporary and will be removed when billboard rendering is implemented.
fn create_unit_icosphere_entity(
    device: &wgpu::Device,
    global_bind_group_layout: &wgpu::BindGroupLayout,
) -> Entity {
    let vert_shader = include_shader_binary!("icosphere.vert");
    let frag_shader = include_shader_binary!("icosphere.frag");

    let vert_module = device.create_shader_module(vert_shader);
    let frag_module = device.create_shader_module(frag_shader);

    let icosphere = IsoSphere::new();

    let vertex_buffer = device.create_buffer_with_data(
        bytemuck::cast_slice(icosphere.vertices()),
        wgpu::BufferUsage::VERTEX,
    );

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[global_bind_group_layout],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vert_module,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &frag_module,
            entry_point: "main",
        }),
        rasterization_state: Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: wgpu::CullMode::None,
            depth_bias: 0,
            depth_bias_slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        }),
        primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        color_states: &[
            wgpu::ColorStateDescriptor {
                format: DEFAULT_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            },
            wgpu::ColorStateDescriptor {
                format: NORMAL_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            },
        ],
        depth_stencil_state: None,
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 4 * 3,
                        shader_location: 1,
                    },
                ],
            }],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    Entity {
        vertex_buffer,
        render_pipeline,

        vertex_num: icosphere.vertices().len(),
    }
}
