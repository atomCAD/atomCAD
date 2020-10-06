use crate::{include_spirv, GlobalRenderResources, SWAPCHAIN_FORMAT};

pub struct BlitPass {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    render_bundle: wgpu::RenderBundle,
}

impl BlitPass {
    pub fn new(render_resources: &GlobalRenderResources, input: &wgpu::TextureView) -> Self {
        let bind_group_layout = create_bind_group_layout(&render_resources.device);
        let pipeline = create_blit_pipeline(&render_resources.device, &bind_group_layout);
        let render_bundle = create_blit_render_bundle(
            &render_resources.device,
            &bind_group_layout,
            &render_resources.linear_sampler,
            input,
            &pipeline,
        );

        Self {
            bind_group_layout,
            pipeline,
            render_bundle,
        }
    }

    pub fn run(&self, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::TextureView) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: frame,
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
            }],
            depth_stencil_attachment: None,
        });

        rpass.execute_bundles(Some(&self.render_bundle).into_iter());
    }

    pub fn update(&mut self, render_resources: &GlobalRenderResources, input: &wgpu::TextureView) {
        self.render_bundle = create_blit_render_bundle(
            &render_resources.device,
            &self.bind_group_layout,
            &render_resources.linear_sampler,
            input,
            &self.pipeline,
        );
    }
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::Sampler { comparison: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn create_blit_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    let vert_shader = device.create_shader_module(include_spirv!("blit.vert"));
    let frag_shader = device.create_shader_module(include_spirv!("blit.frag"));

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vert_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &frag_shader,
            entry_point: "main",
        }),
        rasterization_state: None,
        primitive_topology: wgpu::PrimitiveTopology::TriangleList, // doesn't matter
        color_states: &[SWAPCHAIN_FORMAT.into()],
        depth_stencil_state: None,
        vertex_state: wgpu::VertexStateDescriptor {
            // doesn't matter
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    })
}

fn create_blit_render_bundle(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    linear_sampler: &wgpu::Sampler,
    input_texture: &wgpu::TextureView,
    blit_pipeline: &wgpu::RenderPipeline,
) -> wgpu::RenderBundle {
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(linear_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&input_texture),
            },
        ],
    });

    let mut encoder = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
        label: None,
        color_formats: &[SWAPCHAIN_FORMAT],
        depth_stencil_format: None,
        sample_count: 1,
    });

    encoder.set_pipeline(blit_pipeline);
    encoder.set_bind_group(0, &bind_group, &[]);
    encoder.draw(0..3, 0..1);
    encoder.finish(&wgpu::RenderBundleDescriptor { label: None })
}
