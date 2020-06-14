// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::math::{Mat3, Mat4, Vec3};
use glsl_layout::AsStd140;
use rand::distributions::{Distribution, Uniform as RandUniform};
use rayon::prelude::*;
use std::{convert::TryInto as _, mem};
use winit::dpi::PhysicalSize;

use super::uniform::Uniform;
use super::{DEFAULT_FORMAT, DEPTH_FORMAT};

// Testing
static POINTS: &[Point] = &[
    Point {
        pos: Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        kind: 0,
    },
    Point {
        pos: Vec3 {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        },
        kind: 1,
    },
    Point {
        pos: Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
        kind: 0,
    },
    Point {
        pos: Vec3 {
            x: 0.0,
            y: -1.0,
            z: 0.0,
        },
        kind: 1,
    },
    Point {
        pos: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        kind: 0,
    },
    Point {
        pos: Vec3 {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        },
        kind: 1,
    },
];

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct Point {
    pub pos: Vec3,
    pub kind: u32,
}

unsafe impl bytemuck::Zeroable for Point {}
unsafe impl bytemuck::Pod for Point {}

#[derive(Debug, Copy, Clone, PartialEq, AsStd140)]
struct VertUniforms {
    world_mx: glsl_layout::mat4,
    inv_view_mx: glsl_layout::mat3,
    sphere_radius: glsl_layout::float,
}

#[derive(Debug, Copy, Clone, PartialEq, AsStd140)]
struct FragUniforms {
    projection_mx: glsl_layout::mat4,
}

pub struct Billboards {
    render_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vert_uniform_buffer: Uniform<VertUniforms>,
    frag_uniform_buffer: Uniform<FragUniforms>,

    depth_texture: wgpu::Texture,

    /// These are temporary.
    /// Eventually, the buffer of points will be generated by a compute shader.
    point_buffer: wgpu::Buffer,
    num_points: usize,
}

impl Billboards {
    pub fn new(device: &wgpu::Device, size: PhysicalSize<u32>) -> Self {
        let num_points = 80_000_000;

        let point_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: (mem::size_of::<Point>() * num_points) as u64,
            usage: wgpu::BufferUsage::STORAGE,
            mapped_at_creation: true,
            label: None,
        });

        {
            let buffer_slice = point_buffer.slice(..);

            let mut writable_view = buffer_slice.get_mapped_range_mut();

            let pos_die = RandUniform::from(-600.0..600.0);
            let kind_die = RandUniform::from(0..=1);

            writable_view[..]
                .par_chunks_mut(mem::size_of::<Point>())
                .for_each_init(
                    || rand::thread_rng(),
                    |rng, chunk| {
                        chunk.copy_from_slice(bytemuck::bytes_of(&Point {
                            pos: Vec3::new(
                                pos_die.sample(rng),
                                pos_die.sample(rng),
                                pos_die.sample(rng),
                            ),
                            kind: kind_die.sample(rng),
                        }))
                    },
                );
        }

        point_buffer.unmap();

        create_billboards(device, size, point_buffer, num_points)
    }

    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        world_mx: Mat4,
        projection_mx: Mat4,
        inv_view_mx: Mat3,
    ) {
        let vert_uniforms = VertUniforms {
            world_mx: Into::<[[f32; 4]; 4]>::into(world_mx).into(),
            inv_view_mx: Into::<[[f32; 3]; 3]>::into(inv_view_mx).into(),
            sphere_radius: 1.0, // ?
        };

        let frag_uniforms = FragUniforms {
            projection_mx: Into::<[[f32; 4]; 4]>::into(projection_mx).into(),
        };

        self.vert_uniform_buffer.set(queue, vert_uniforms);
        self.frag_uniform_buffer.set(queue, frag_uniforms);
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: PhysicalSize<u32>) {
        self.depth_texture = create_depth_texture(device, size);
    }

    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: wgpu::TextureView) {
        let depth_view = self.depth_texture.create_default_view();

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &target,
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::WHITE,
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &depth_view,
                depth_load_op: wgpu::LoadOp::Clear,
                depth_store_op: wgpu::StoreOp::Store,
                clear_depth: 1.0,
                stencil_load_op: wgpu::LoadOp::Clear,
                stencil_store_op: wgpu::StoreOp::Store,
                clear_stencil: 1,
                depth_read_only: false,
                stencil_read_only: true,
            }),
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(
            0..(self.num_points * 6) // See shaders/billboard.vert.
                .try_into()
                .expect("too many points to draw"),
            0..1,
        );
    }
}

fn create_billboards(
    device: &wgpu::Device,
    size: PhysicalSize<u32>,
    point_buffer: wgpu::Buffer,
    num_points: usize,
) -> Billboards {
    let vert_shader = include_shader_binary!("billboard.vert");
    let frag_shader = include_shader_binary!("billboard.frag");

    let vert_module = device.create_shader_module(vert_shader);
    let frag_module = device.create_shader_module(frag_shader);

    let vert_uniform_buffer: Uniform<VertUniforms> = Uniform::new(device);
    let frag_uniform_buffer: Uniform<FragUniforms> = Uniform::new(device);

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        bindings: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                ..Default::default()
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::StorageBuffer {
                    dynamic: false,
                    readonly: true,
                },
                ..Default::default()
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                ..Default::default()
            },
        ],
        label: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        bindings: &[
            wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(vert_uniform_buffer.buffer_view()),
            },
            wgpu::Binding {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(point_buffer.slice(..)),
            },
            wgpu::Binding {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(frag_uniform_buffer.buffer_view()),
            },
        ],
        label: None,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[&bind_group_layout],
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
        color_states: &[wgpu::ColorStateDescriptor {
            format: DEFAULT_FORMAT,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        }],
        depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil_front: Default::default(),
            stencil_back: Default::default(),
            stencil_read_mask: !0,
            stencil_write_mask: !0,
        }),
        // Ignored, since we're not using a vertex buffer.
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    let depth_texture = create_depth_texture(device, size);

    Billboards {
        render_pipeline,
        bind_group,
        vert_uniform_buffer,
        frag_uniform_buffer,

        depth_texture,

        point_buffer,
        num_points,
    }
}

fn create_depth_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        label: None,
    })
}
