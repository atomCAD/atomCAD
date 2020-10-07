use crate::{
    include_spirv, BufferVec, Fragment, FragmentId, GlobalRenderResources, PartId, Renderer,
    SWAPCHAIN_FORMAT,
};
use std::{collections::HashMap, convert::TryInto as _, mem};
use winit::dpi::PhysicalSize;

// Renders atoms
pub struct MolecularPass {
    pipeline: wgpu::RenderPipeline,
    top_level_bg: wgpu::BindGroup,

    color_texture: wgpu::TextureView,
    depth_texture: wgpu::TextureView,
    // stencil_texture: wgpu::TextureView,
    // for deferred rendering/ambient occlusion approximation
    normals_texture: wgpu::TextureView,

    driven: Driven,
}

#[repr(C)]
struct DrawIndirect {
    vertex_count: u32,   // The number of vertices to draw.
    instance_count: u32, // The number of instances to draw.
    base_vertex: u32,    // The Index of the first vertex to draw.
    base_instance: u32,  // The instance ID of the first instance to draw.
}

enum Driven {
    CpuDriven,
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
        periodic_table_buffer: &wgpu::Buffer,
        size: PhysicalSize<u32>,
        gpu_driven_rendering: bool,
    ) -> (Self, wgpu::TextureView) {
        let top_level_bgl = create_top_level_bgl(&render_resources.device);
        let pipeline = create_render_pipeline(
            &render_resources.device,
            &top_level_bgl,
            &render_resources.atom_bgl,
        );
        let top_level_bg = create_top_level_bg(
            &render_resources.device,
            &top_level_bgl,
            camera_binding_resource,
            periodic_table_buffer,
        );

        let color_texture = create_color_texture(&render_resources.device, size);
        let depth_texture = create_depth_texture(&render_resources.device, size);
        let normals_texture = create_normals_texture(&render_resources.device, size);

        assert!(!gpu_driven_rendering);

        (
            Self {
                pipeline,
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
        fragments: impl IntoIterator<Item = &'a Fragment>,
        fragment_transforms: &wgpu::Buffer,
        per_fragment: &HashMap<FragmentId, (PartId, u64 /* transform index */)>,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &self.color_texture,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.8,
                            g: 0.8,
                            b: 0.8,
                            a: 1.0,
                        }),
                        store: true,
                    },
                },
                // multiple render targets
                // render to normals texture
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &self.normals_texture,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                },
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &self.depth_texture,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.top_level_bg, &[]);

        // TODO: Try instancing?
        for fragment in fragments {
            let transform_offset =
                per_fragment[&fragment.id()].1 * (mem::size_of::<ultraviolet::Mat4>() as u64);

            rpass.set_vertex_buffer(
                0,
                fragment_transforms.slice(
                    transform_offset..transform_offset + mem::size_of::<ultraviolet::Mat4>() as u64,
                ),
            );

            rpass.set_bind_group(1, &fragment.atoms().bind_group(), &[]);
            rpass.draw(0..(fragment.atoms().len() * 3).try_into().unwrap(), 0..1)
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
                visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // configuration
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: true,
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
                resource: wgpu::BindingResource::Buffer {
                    buffer: &periodic_table_buffer,
                    offset: 0,
                    size: None,
                },
            },
        ],
    })
}

fn create_render_pipeline(
    device: &wgpu::Device,
    top_level_bgl: &wgpu::BindGroupLayout,
    atom_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let atom_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&top_level_bgl, atom_bgl],
        push_constant_ranges: &[],
    });

    let atom_vert_shader = device.create_shader_module(include_spirv!("billboard.vert"));
    let atom_frag_shader = device.create_shader_module(include_spirv!("billboard.frag"));

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&atom_pipeline_layout),
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &atom_vert_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &atom_frag_shader,
            entry_point: "main",
        }),
        rasterization_state: None, // this might not be right
        primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        color_states: &[
            SWAPCHAIN_FORMAT.into(),
            wgpu::TextureFormat::Rgba16Float.into(),
        ],
        depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: wgpu::StencilStateDescriptor::default(),
        }),
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: mem::size_of::<ultraviolet::Mat4>() as _,
                step_mode: wgpu::InputStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    // part and fragment transform matrix
                    0 => Float4,
                    1 => Float4,
                    2 => Float4,
                    3 => Float4,
                ],
            }],
        },
        sample_count: 1, // multisampling doesn't work for shader effects (like spherical imposters/billboards)
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    })
}

fn create_color_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
    Renderer::create_texture(
        device,
        size,
        SWAPCHAIN_FORMAT,
        wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
    )
}

fn create_depth_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
    Renderer::create_texture(
        device,
        size,
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsage::OUTPUT_ATTACHMENT,
    )
    .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_normals_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
    Renderer::create_texture(
        device,
        size,
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
    )
    .create_view(&wgpu::TextureViewDescriptor::default())
}
