use winit::dpi::PhysicalSize;

pub struct Filter {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    texture: 
}

impl Filter {
    pub fn new(device: &wgpu::Device, size: PhysicalSize<u32>) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            bindings: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::COMPUTE,
                ty: wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView()
                }
            ]
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {});
    }
}
