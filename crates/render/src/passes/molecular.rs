// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{AtomBuffer, BondBuffer, GlobalRenderResources, Renderer, SWAPCHAIN_FORMAT};
use std::{convert::TryInto as _, mem};
use winit::dpi::PhysicalSize;

// Renders atoms
pub struct MolecularPass {
    atom_pipeline: wgpu::RenderPipeline,
    bond_pipeline: wgpu::RenderPipeline,
    top_level_bg: wgpu::BindGroup,

    color_texture: wgpu::TextureView,
    depth_texture: wgpu::TextureView,
    // stencil_texture: wgpu::TextureView,
    // for deferred rendering/ambient occlusion approximation
    normals_texture: wgpu::TextureView,

    #[allow(dead_code)]
    driven: Driven,
}

#[repr(C)]
#[allow(dead_code)]
struct DrawIndirect {
    vertex_count: u32,   // The number of vertices to draw.
    instance_count: u32, // The number of instances to draw.
    base_vertex: u32,    // The Index of the first vertex to draw.
    base_instance: u32,  // The instance ID of the first instance to draw.
}

enum Driven {
    CpuDriven,
    #[allow(dead_code)]
    GpuDriven {
        // fragment_buffer_refs: BufferVec,
        // draw_commands: BufferVec,
        // do we embed additional passes here?
    },
}

impl MolecularPass {
    pub fn new(
        render_resources: &GlobalRenderResources,
        camera_binding_resource: wgpu::BindingResource,
        vertex_constants_buffer: &wgpu::Buffer,
        periodic_table_buffer: &wgpu::Buffer,
        size: PhysicalSize<u32>,
    ) -> (Self, wgpu::TextureView) {
        let top_level_bgl = create_top_level_bgl(&render_resources.device);
        let atom_pipeline = create_atom_render_pipeline(
            &render_resources.device,
            &top_level_bgl,
            &render_resources.atom_bgl,
        );
        let bond_pipeline = create_bond_render_pipeline(
            &render_resources.device,
            &top_level_bgl,
            &render_resources.atom_bgl,
            &render_resources.bond_bgl,
        );
        let top_level_bg = create_top_level_bg(
            &render_resources.device,
            &top_level_bgl,
            camera_binding_resource,
            vertex_constants_buffer,
            periodic_table_buffer,
        );

        let color_texture = create_color_texture(&render_resources.device, size);
        let depth_texture = create_depth_texture(&render_resources.device, size);
        let normals_texture = create_normals_texture(&render_resources.device, size);

        (
            Self {
                atom_pipeline,
                bond_pipeline,
                top_level_bg,

                color_texture: color_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                depth_texture,
                normals_texture,
                driven: Driven::CpuDriven,
            },
            color_texture.create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    // Returns `(color texture, normals texture)`
    pub fn update(
        &mut self,
        render_resources: &GlobalRenderResources,
        size: PhysicalSize<u32>,
    ) -> (&wgpu::TextureView, &wgpu::TextureView) {
        self.color_texture = create_color_texture(&render_resources.device, size)
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.depth_texture = create_depth_texture(&render_resources.device, size);
        self.normals_texture = create_normals_texture(&render_resources.device, size);

        (&self.color_texture, &self.normals_texture)
    }

    // TODO: figure out how to multithread this
    pub fn run<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        atoms: &[&'a AtomBuffer],
        bonds: &[Option<&'a BondBuffer>],
        fragment_transforms: &wgpu::Buffer,
        // fragments: impl IntoIterator<Item = &'a Fragment>,
        // fragment_transforms: &wgpu::Buffer,
        // per_fragment: &HashMap<FragmentId, (PartId, u64 /* transform index */)>,
    ) {
        assert!(
            atoms.len() == bonds.len(),
            "Must have equal number of AtomBuffers and Option<BondBuffers>"
        );
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &self.color_texture,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.703125,
                            g: 0.703125,
                            b: 0.703125,
                            a: 1.000000,
                        }),
                        store: true,
                    },
                }),
                // multiple render targets
                // render to normals texture
                Some(wgpu::RenderPassColorAttachment {
                    view: &self.normals_texture,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        rpass.set_pipeline(&self.atom_pipeline);
        rpass.set_bind_group(0, &self.top_level_bg, &[]);

        for (idx, atoms_inst) in atoms.iter().enumerate() {
            let transform_offset = (idx * mem::size_of::<ultraviolet::Mat4>()) as u64;

            rpass.set_vertex_buffer(
                0,
                fragment_transforms.slice(
                    transform_offset..transform_offset + mem::size_of::<ultraviolet::Mat4>() as u64,
                ),
            );

            rpass.set_bind_group(1, atoms_inst.bind_group(), &[]);
            rpass.draw(0..(atoms_inst.len() * 3).try_into().unwrap(), 0..1);
        }

        // render bonds
        rpass.set_pipeline(&self.bond_pipeline);
        rpass.set_bind_group(0, &self.top_level_bg, &[]);

        for (idx, bonds_inst) in bonds.iter().enumerate() {
            if let Some(bonds_inst) = bonds_inst {
                let transform_offset = (idx * mem::size_of::<ultraviolet::Mat4>()) as u64;

                rpass.set_vertex_buffer(
                    0,
                    fragment_transforms.slice(
                        transform_offset
                            ..transform_offset + mem::size_of::<ultraviolet::Mat4>() as u64,
                    ),
                );

                rpass.set_bind_group(1, atoms[idx].bind_group(), &[]);
                rpass.set_bind_group(2, bonds_inst.bind_group(), &[]);
                // the factor of 6 is used because each atom becomes a quad (two tris)
                rpass.draw(0..(bonds_inst.len() * 6).try_into().unwrap(), 0..1);
            }
        }
    }
}

fn create_top_level_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            // camera
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // periodic table
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // vertex constants
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_top_level_bg(
    device: &wgpu::Device,
    top_level_bgl: &wgpu::BindGroupLayout,
    camera_binding_resource: wgpu::BindingResource,
    vertex_constants_buffer: &wgpu::Buffer,
    periodic_table_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: top_level_bgl,
        entries: &[
            // camera
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_binding_resource,
            },
            // periodic table
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: periodic_table_buffer,
                    offset: 0,
                    size: None,
                }),
            },
            // vertex constants
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: vertex_constants_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    })
}

fn create_atom_render_pipeline(
    device: &wgpu::Device,
    top_level_bgl: &wgpu::BindGroupLayout,
    atom_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let atom_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[top_level_bgl, atom_bgl],
        push_constant_ranges: &[],
    });

    let atom_shader = device.create_shader_module(wgpu::include_wgsl!("atom.wgsl"));

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&atom_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &atom_shader,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<ultraviolet::Mat4>() as _,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    // part and fragment transform matrix
                    0 => Float32x4,
                    1 => Float32x4,
                    2 => Float32x4,
                    3 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &atom_shader,
            entry_point: "fs_main",
            targets: &[
                Some(SWAPCHAIN_FORMAT.into()),
                Some(wgpu::TextureFormat::Rgba16Float.into()),
            ],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Front),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    })
}

fn create_bond_render_pipeline(
    device: &wgpu::Device,
    top_level_bgl: &wgpu::BindGroupLayout,
    atom_bgl: &wgpu::BindGroupLayout,
    bond_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let bond_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[top_level_bgl, atom_bgl, bond_bgl],
        push_constant_ranges: &[],
    });

    let bond_shader = device.create_shader_module(wgpu::include_wgsl!("bond.wgsl"));

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&bond_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &bond_shader,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<ultraviolet::Mat4>() as _,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    // part and fragment transform matrix
                    0 => Float32x4,
                    1 => Float32x4,
                    2 => Float32x4,
                    3 => Float32x4,
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &bond_shader,
            entry_point: "fs_main",
            targets: &[
                Some(SWAPCHAIN_FORMAT.into()),
                Some(wgpu::TextureFormat::Rgba16Float.into()),
            ],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    })
}

fn create_color_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
    Renderer::create_texture(
        device,
        size,
        SWAPCHAIN_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    )
}

fn create_depth_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
    Renderer::create_texture(
        device,
        size,
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    )
    .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_normals_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
    Renderer::create_texture(
        device,
        size,
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    )
    .create_view(&wgpu::TextureViewDescriptor::default())
}

// End of File
