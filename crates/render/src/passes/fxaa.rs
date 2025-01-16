// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::shaders;
use crate::{GlobalRenderResources, Renderer, SWAPCHAIN_FORMAT};
use common::AsBytes;
use winit::dpi::PhysicalSize;

#[allow(dead_code)]
struct FxaaParams {
    edge_threshold_min: f32,
    edge_threshold_max: f32,
    max_iterations: i32,
    subpixel_quality: f32,
}
unsafe impl AsBytes for FxaaParams {}

impl Default for FxaaParams {
    fn default() -> Self {
        Self {
            edge_threshold_min: 0.0312, // HIGH
            edge_threshold_max: 0.125,  // HIGH
            max_iterations: 12,
            subpixel_quality: 0.75,
        }
    }
}

pub struct FxaaPass {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    texture: wgpu::TextureView,
    size: (u32, u32),
}

impl FxaaPass {
    pub fn new(
        render_resources: &GlobalRenderResources,
        size: PhysicalSize<u32>,
        input_texture: &wgpu::TextureView,
    ) -> (Self, wgpu::TextureView) {
        let params = render_resources
            .device
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("fxaa_params"),
                size: std::mem::size_of::<FxaaParams>() as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: false,
            });
        // Initialize default values for FxaaParams
        render_resources
            .queue
            .write_buffer(&params, 0, FxaaParams::default().as_bytes());

        let sampler = render_resources
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: None,
                mipmap_filter: wgpu::FilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
        let og_texture = create_fxaa_texture(&render_resources.device, size);
        let bind_group_layout = create_bind_group_layout(&render_resources.device);

        let texture = og_texture.create_view(&wgpu::TextureViewDescriptor::default());

        (
            Self {
                pipeline: create_fxaa_pipeline(&render_resources.device, &bind_group_layout),
                bind_group: create_fxaa_bind_group(
                    &render_resources.device,
                    &bind_group_layout,
                    &params,
                    &sampler,
                    input_texture,
                ),
                bind_group_layout,
                params,
                sampler,
                texture,
                size: ((size.width + 7) / 8, (size.height + 7) / 8),
            },
            og_texture.create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    pub fn run(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fxaa_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.texture,
                resolve_target: None,
                ops: wgpu::Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }

    pub fn update(
        &mut self,
        render_resources: &GlobalRenderResources,
        input_texture: &wgpu::TextureView,
        size: PhysicalSize<u32>,
    ) -> &wgpu::TextureView {
        self.texture = create_fxaa_texture(&render_resources.device, size)
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = create_fxaa_bind_group(
            &render_resources.device,
            &self.bind_group_layout,
            &self.params,
            &self.sampler,
            input_texture,
        );
        self.size = ((size.width + 7) / 8, (size.height + 7) / 8);

        &self.texture
    }
}

fn create_fxaa_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
    Renderer::create_texture(
        device,
        size,
        SWAPCHAIN_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    )
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fxaa_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(
                        std::mem::size_of::<FxaaParams>() as u64
                    ),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_fxaa_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    let vert = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::from(shaders::fullscreen::SOURCE)),
    });
    let frag = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::from(shaders::fxaa::SOURCE)),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &vert,
            entry_point: Some("fullscreen"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &frag,
            entry_point: Some("fxaa"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: SWAPCHAIN_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn create_fxaa_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    input_texture: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fxaa_bind_group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: params,
                    offset: 0,
                    size: std::num::NonZeroU64::new(std::mem::size_of::<FxaaParams>() as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(input_texture),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

// End of File
