use crate::{include_spirv, GlobalRenderResources, Renderer, STORAGE_TEXTURE_FORMAT};
use winit::dpi::PhysicalSize;

pub struct FxaaPass {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    texture: wgpu::TextureView,
    size: (u32, u32),
}

impl FxaaPass {
    pub fn new(
        render_resources: &GlobalRenderResources,
        size: PhysicalSize<u32>,
        input: &wgpu::TextureView,
    ) -> (Self, wgpu::TextureView) {
        let og_texture = create_fxaa_texture(&render_resources.device, size);
        let bind_group_layout = create_bind_group_layout(&render_resources.device);

        let texture = og_texture.create_view(&wgpu::TextureViewDescriptor::default());

        (
            Self {
                pipeline: create_fxaa_pipeline(&render_resources.device, &bind_group_layout),
                bind_group: create_fxaa_bind_group(
                    &render_resources.device,
                    &bind_group_layout,
                    &render_resources.linear_sampler,
                    input,
                    &texture,
                ),
                bind_group_layout,
                texture,
                size: ((size.width + 7) / 8, (size.height + 7) / 8),
            },
            og_texture.create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    pub fn run<'a>(&'a self, cpass: &mut wgpu::ComputePass<'a>) {
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &self.bind_group, &[]);
        cpass.dispatch(self.size.0, self.size.1, 1);
    }

    pub fn update(
        &mut self,
        render_resources: &GlobalRenderResources,
        input: &wgpu::TextureView,
        size: PhysicalSize<u32>,
    ) -> &wgpu::TextureView {
        self.texture = create_fxaa_texture(&render_resources.device, size)
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = create_fxaa_bind_group(
            &render_resources.device,
            &self.bind_group_layout,
            &render_resources.linear_sampler,
            input,
            &self.texture,
        );
        self.size = ((size.width + 7) / 8, (size.height + 7) / 8);

        &self.texture
    }
}

fn create_fxaa_texture(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::Texture {
    Renderer::create_texture(
        device,
        size,
        STORAGE_TEXTURE_FORMAT,
        wgpu::TextureUsage::OUTPUT_ATTACHMENT
            | wgpu::TextureUsage::SAMPLED
            | wgpu::TextureUsage::STORAGE,
    )
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::Sampler { comparison: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    format: crate::STORAGE_TEXTURE_FORMAT,
                    readonly: false,
                },
                count: None,
            },
        ],
    })
}

fn create_fxaa_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    let shader = device.create_shader_module(include_spirv!("fxaa.comp"));

    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        compute_stage: wgpu::ProgrammableStageDescriptor {
            module: &shader,
            entry_point: "main",
        },
    })
}

fn create_fxaa_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    linear_sampler: &wgpu::Sampler,
    input_texture: &wgpu::TextureView,
    fxaa_texture: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(linear_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(input_texture),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(fxaa_texture),
            },
        ],
    })
}
