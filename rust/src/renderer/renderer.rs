use wgpu::*;
use bytemuck;
use std::time::Instant;
use wgpu::util::DeviceExt;
use crate::kernel::model::Model;
use super::mesh::Mesh;
use super::mesh::Vertex;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UniformData {
    time: f32,
}

const VERTICES: &[Vertex] = &[
  Vertex { position: [-0.0868241, 0.49240386, 0.0], color: [0.5, 0.0, 0.5] }, // A
  Vertex { position: [-0.49513406, 0.06958647, 0.0], color: [0.5, 0.0, 0.5] }, // B
  Vertex { position: [-0.21918549, -0.44939706, 0.0], color: [0.5, 0.0, 0.5] }, // C
  Vertex { position: [0.35966998, -0.3473291, 0.0], color: [0.5, 0.0, 0.5] }, // D
  Vertex { position: [0.44147372, 0.2347359, 0.0], color: [0.5, 0.0, 0.5] }, // E
];

const INDICES: &[u32] = &[
    0, 1, 4,
    1, 2, 4,
    2, 3, 4,
];

pub struct Renderer  {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer, 
    num_indices: u32,
    texture: Texture,
    texture_view: TextureView,
    output_buffer: Buffer,
    texture_size: Extent3d,
    uniform_buffer: Buffer,
    uniform_bind_group: BindGroup,
    start_time: Instant,
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Self {
        let start_time = Instant::now();

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

        let vertex_buffer = device.create_buffer_init(
          &wgpu::util::BufferInitDescriptor {
              label: Some("Vertex Buffer"),
              contents: bytemuck::cast_slice(VERTICES),
              usage: wgpu::BufferUsages::VERTEX,
          }
        );

        let index_buffer = device.create_buffer_init(
          &wgpu::util::BufferInitDescriptor {
              label: Some("Index Buffer"),
              contents: bytemuck::cast_slice(INDICES),
              usage: wgpu::BufferUsages::INDEX,
          }
        );
        let num_indices = INDICES.len() as u32;

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

        // Uniform buffer for time
        let initial_uniform_data = UniformData { time: 0.0 };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
          label: Some("Uniform Buffer"),
          contents: bytemuck::cast_slice(&[initial_uniform_data]),
          usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Shader module
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: ShaderSource::Wgsl(include_str!("triangle_shader.wgsl").into()),
        });

        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
          label: Some("Uniform Bind Group Layout"),
          entries: &[
              wgpu::BindGroupLayoutEntry {
                  binding: 0,
                  visibility: wgpu::ShaderStages::VERTEX,
                  ty: wgpu::BindingType::Buffer {
                      ty: wgpu::BufferBindingType::Uniform,
                      has_dynamic_offset: false,
                      min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<UniformData>() as _),
                  },
                  count: None,
              }
          ],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
          layout: &uniform_bind_group_layout, // Use the layout defined above
          entries: &[
              wgpu::BindGroupEntry {
                  binding: 0, // Must match the bind group layout and shader binding
                  resource: uniform_buffer.as_entire_binding(),
              },
          ],
          label: Some("Uniform Bind Group"),
        });

        // Pipeline setup
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[
                  Vertex::desc(),
                ],
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
            vertex_buffer,
            index_buffer,
            num_indices,
            texture,
            texture_view,
            output_buffer,
            texture_size,
            uniform_buffer,
            uniform_bind_group,
            start_time,
        }
    }

    pub fn refresh(model: &Model) {
      //TODO: tessellate everythin into the vertex buffer for now
    }

    pub fn render(&mut self) -> Vec<u8> {

        let t = (&self).start_time.elapsed().as_secs_f32();
        let uniform_data = UniformData { time: t };

        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform_data]));

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
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]); // Bind the uniforms
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
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
