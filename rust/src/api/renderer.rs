use wgpu::*;

pub struct Renderer {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,
    texture: Texture,
    texture_view: TextureView,
    output_buffer: Buffer,
    texture_size: Extent3d,
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Self {
        // Initialize GPU
        let instance = Instance::default();
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .expect("Failed to find a suitable adapter");
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default(), None)
            .await
            .expect("Failed to create device");

        // Texture size
        let texture_size = Extent3d {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        };

        // Create texture
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Render Target Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        // Texture view
        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        // Create output buffer for readback
        let output_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Output Buffer"),
            size: (4 * texture_size.width * texture_size.height) as BufferAddress, // 4 bytes per pixel (RGBA8)
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Shader module
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: ShaderSource::Wgsl(include_str!("triangle_shader.wgsl").into()),
        });

        // Pipeline setup
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device,
            queue,
            pipeline,
            texture,
            texture_view,
            output_buffer,
            texture_size,
        }
    }

    pub fn render(&mut self) -> Vec<u8> {

        // Create a new command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Command Encoder"),
            });

        // Render pass
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.texture_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.draw(0..3, 0..1);
        }

        // Copy texture to output buffer
        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &self.output_buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.texture_size.width),
                    rows_per_image: Some(self.texture_size.height),
                },
            },
            self.texture_size,
        );

        // Submit commands
        self.queue.submit(Some(encoder.finish()));

        // Read data
        let buffer_slice = self.output_buffer.slice(..);
        buffer_slice.map_async(MapMode::Read, |_| {
        });

        self.device.poll(Maintain::Wait);

        let data = buffer_slice.get_mapped_range().to_vec();
        self.output_buffer.unmap();

        data
    }
}
