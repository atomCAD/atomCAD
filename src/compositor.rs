// Copyright (c) 2020 by Lachlan Sneff <lachlan@charted.space>
// Copyright (c) 2020 by Mark Friedenbach <mark@friedenbach.org>

use winit::dpi::PhysicalSize;

/// Used to layer UI and scene on top of each other.
pub struct Compositor {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    render_pipeline: wgpu::RenderPipeline,
    ui_texture: wgpu::Texture,
    scene_target: wgpu::TextureView,
}

impl Compositor {
    pub fn new(
        device: &wgpu::Device,
        scene_target: wgpu::TextureView,
        size: PhysicalSize<u32>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: if cfg!(build = "debug") {
                Some("compositor bind group layout")
            } else {
                None
            },
            bindings: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        dimension: wgpu::TextureViewDimension::D2,
                        component_type: wgpu::TextureComponentType::Float,
                        multisampled: false,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        dimension: wgpu::TextureViewDimension::D2,
                        component_type: wgpu::TextureComponentType::Float,
                        multisampled: false,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler { comparison: false },
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            compare: wgpu::CompareFunction::Undefined,
        });

        let ui_texture = Self::generate_ui_texture(device, size);
        let bind_group = Self::generate_bind_group(
            device,
            &ui_texture.create_default_view(),
            &scene_target,
            &bind_group_layout,
            &sampler,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout],
        });

        let vert_shader_spirv = include_shader_binary!("compositor.vert");
        let frag_shader_spirv = include_shader_binary!("compositor.frag");

        let vert_shader = device.create_shader_module(vert_shader_spirv);
        let frag_shader = device.create_shader_module(frag_shader_spirv);

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vert_shader,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &frag_shader,
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
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[],
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        Self {
            bind_group_layout,
            bind_group,
            sampler,
            render_pipeline,
            ui_texture,
            scene_target,
        }
    }

    fn generate_ui_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: if cfg!(build = "debug") {
                Some("ui texture")
            } else {
                None
            },
        })
    }

    fn generate_bind_group(
        device: &wgpu::Device,
        ui_texture_view: &wgpu::TextureView,
        scene_target: &wgpu::TextureView,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(ui_texture_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(scene_target),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
            label: if cfg!(build = "debug") {
                Some("compositor bind group")
            } else {
                None
            },
        })
    }

    /// Resize the compositor.
    // TODO: Generate new textures for the scene and everything in the main thread so we can resize this
    // when we're supposed to.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        scene_target: wgpu::TextureView,
        size: PhysicalSize<u32>,
    ) {
        self.ui_texture = Self::generate_ui_texture(device, size);
        self.bind_group = Self::generate_bind_group(
            device,
            &self.ui_texture.create_default_view(),
            &scene_target,
            &self.bind_group_layout,
            &self.sampler,
        );
        self.scene_target = scene_target;
    }

    pub fn get_ui_texture(&self) -> wgpu::TextureView {
        self.ui_texture.create_default_view()
    }

    pub fn blit(
        &mut self,
        swapchain_output: &wgpu::TextureView,
        command_encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: swapchain_output,
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::TRANSPARENT,
            }],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

// End of File
