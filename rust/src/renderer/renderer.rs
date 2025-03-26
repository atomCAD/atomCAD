use wgpu::*;
use bytemuck;
use wgpu::util::DeviceExt;
use super::mesh::Vertex;
use super::mesh::Mesh;
use super::gpu_mesh::GPUMesh;
use super::tessellator::atomic_tessellator;
use super::tessellator::surface_point_tessellator;
use super::camera::Camera;
use glam::f32::Vec3;
use glam::f32::Mat4;
use glam::f64::DVec3;
use glam::f64::DMat4;
use crate::common::scene::Scene;
use std::time::Instant;
use std::sync::Mutex;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],

    camera_position: [f32; 3],

    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use padding fields
    _padding0: u32,

    // There is a directional light 'attached' to the camera.
    // It behaves as a 'head light', so it always shines slightly 'from above'.
    head_light_dir: [f32; 3],

    _padding1: u32,
}

impl CameraUniform {
  fn new() -> Self {
      Self {
        view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        camera_position: Vec3::new(0.0, 0.0, 0.0).to_array(),
        _padding0: 0,
        head_light_dir: Vec3::new(0.0, -1.0, 0.0).to_array(),
        _padding1: 0,
      }
  }

  fn refresh(&mut self, camera: &Camera) {
    self.view_proj = camera.build_view_projection_matrix().as_mat4().to_cols_array_2d();
    self.camera_position = camera.eye.as_vec3().to_array();
    self.head_light_dir = camera.calc_headlight_direction().as_vec3().to_array();
  }
}

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub struct Renderer  {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,  
    main_mesh: GPUMesh,
    lightweight_mesh: GPUMesh,
    texture: Texture,
    texture_view: TextureView,
    depth_texture: Texture,
    depth_texture_view: TextureView,
    output_buffer: Buffer,
    pub texture_size: Extent3d,
    pub camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    render_mutex: Mutex<()>,
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Self {
        //let start_time = Instant::now();

        let camera = Camera {
          // position the camera 20 units back
          // +z is out of the screen
          eye: DVec3::new(0.0, 0.0, 20.0),
          // have it look at the origin
          target: DVec3::new(0.0, 0.0, 0.0),
          // which way is "up"
          up: DVec3::new(0.0, 1.0, 0.0),
          aspect: width as f64 / height as f64,
          fovy: std::f64::consts::PI * 0.15,
          znear: 0.5,
          zfar: 600.0,
        };

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

        let main_mesh = GPUMesh::new_empty(&device);
        let lightweight_mesh = GPUMesh::new_empty(&device);

        let texture_size = Extent3d {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        };

        // Create texture
        let texture = Self::create_texture(&device, &texture_size);
        
        // Texture view
        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        // Create depth texture
        let depth_texture = Self::create_depth_texture(&device, &texture_size);
      
        // Create depth texture view
        let depth_texture_view = depth_texture.create_view(&TextureViewDescriptor::default());

        // Create output buffer for readback
        let output_buffer = Self::create_output_buffer(&device, &texture_size);

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.refresh(&camera);
        let camera_buffer = device.create_buffer_init(
          &wgpu::util::BufferInitDescriptor {
              label: Some("Camera Buffer"),
              contents: bytemuck::cast_slice(&[camera_uniform]),
              usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
          }
        );

        // Shader module
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
          entries: &[
              wgpu::BindGroupLayoutEntry {
                  binding: 0,
                  visibility: wgpu::ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                  ty: wgpu::BindingType::Buffer {
                      ty: wgpu::BufferBindingType::Uniform,
                      has_dynamic_offset: false,
                      min_binding_size: None,
                  },
                  count: None,
              }
          ],
          label: Some("camera_bind_group_layout"),
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
          layout: &camera_bind_group_layout,
          entries: &[
              wgpu::BindGroupEntry {
                  binding: 0,
                  resource: camera_buffer.as_entire_binding(),
              }
          ],
          label: Some("camera_bind_group"),
        });

        // Pipeline setup
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                  Vertex::desc(),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8Unorm,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
              topology: wgpu::PrimitiveTopology::TriangleList,
              strip_index_format: None,
              front_face: wgpu::FrontFace::Ccw,
              cull_mode: Some(wgpu::Face::Back),
              // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
              polygon_mode: wgpu::PolygonMode::Fill,
              // Requires Features::DEPTH_CLIP_CONTROL
              unclipped_depth: false,
              // Requires Features::CONSERVATIVE_RASTERIZATION
              conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
              format: DEPTH_FORMAT,
              depth_write_enabled: true,
              depth_compare: wgpu::CompareFunction::Less, // Typical for 3D rendering
              stencil: wgpu::StencilState::default(),
              bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
              count: 1, // 2.
              mask: !0, // 3.
              alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None,
            cache: None,
        });

        Self {
            device,
            queue,
            pipeline,
            main_mesh,
            lightweight_mesh,
            texture,
            texture_view,
            depth_texture,
            depth_texture_view,
            output_buffer,
            texture_size,
            camera,
            camera_buffer,
            camera_bind_group,
            render_mutex: Mutex::new(()),
        }
    }

    pub fn move_camera(&mut self, eye: &DVec3, target: &DVec3, up: &DVec3) {
      self.camera.eye = *eye;
      self.camera.target = *target;
      self.camera.up = *up;

      self.update_camera_buffer();
    }

    // Method to resize the viewport
    pub fn set_viewport_size(&mut self, width: u32, height: u32) {
        // Skip if dimensions haven't changed
        if self.texture_size.width == width && self.texture_size.height == height {
            return;
        }

        // Update camera aspect ratio
        self.camera.aspect = width as f64 / height as f64;
        self.update_camera_buffer();

        // Acquire lock for texture recreation only
        let _lock = self.render_mutex.lock().unwrap();

        // Ensure all previous GPU work is complete before changing resources
        self.device.poll(Maintain::Wait);

        // Update texture size
        self.texture_size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // Recreate all GPU resources
        let texture = Self::create_texture(&self.device, &self.texture_size);
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let depth_texture = Self::create_depth_texture(&self.device, &self.texture_size);
        let depth_texture_view = depth_texture.create_view(&TextureViewDescriptor::default());
        let output_buffer = Self::create_output_buffer(&self.device, &self.texture_size);

        self.texture = texture;
        self.texture_view = texture_view;
        self.depth_texture = depth_texture;
        self.depth_texture_view = depth_texture_view;
        self.output_buffer = output_buffer;

        // Ensure all resource creation is complete
        self.device.poll(Maintain::Wait);
    }

    pub fn refresh<'a, S: Scene<'a>>(&mut self, scene: &S, lightweight: bool) {
        let start_time = Instant::now();

        // Always refresh lightweight buffers with extra tessellatable data
        let mut lightweight_mesh = Mesh::new();
        if let Some(tessellatable) = scene.tessellatable() {
            tessellatable.tessellate(&mut lightweight_mesh);
        }
        
        //println!("lightweight tessellated {} vertices and {} indices", 
        //         lightweight_mesh.vertices.len(), lightweight_mesh.indices.len());

        // Update lightweight GPU mesh
        self.lightweight_mesh.update_from_mesh(&self.device, &lightweight_mesh, "Lightweight");

        // Only refresh main buffers when not in lightweight mode
        if !lightweight {
            // Tessellate everything except tessellatable into main buffers
            let mut mesh = Mesh::new();

            let atomic_tessellation_params = atomic_tessellator::AtomicTessellatorParams {
                sphere_horizontal_divisions: 10,
                sphere_vertical_divisions: 20,
                cylinder_divisions: 16,
            };

            for atomic_structure in scene.atomic_structures() {
                atomic_tessellator::tessellate_atomic_structure(&mut mesh, atomic_structure, &atomic_tessellation_params);
            }

            for surface_point_cloud in scene.surface_point_clouds() {
                surface_point_tessellator::tessellate_surface_point_cloud(&mut mesh, surface_point_cloud);
            }

            //println!("main buffers tessellated {} vertices and {} indices", mesh.vertices.len(), mesh.indices.len());

            // Update main GPU mesh
            self.main_mesh.update_from_mesh(&self.device, &mesh, "Main");
        }

        println!("refresh took: {:?}", start_time.elapsed());
    }

    pub fn render(&mut self) -> Vec<u8> {
        // Acquire lock before rendering
        let _lock = self.render_mutex.lock().unwrap();

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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                  view: &self.depth_texture_view,
                  depth_ops: Some(wgpu::Operations {
                      load: wgpu::LoadOp::Clear(1.0), // Clear depth to the farthest value
                      store: wgpu::StoreOp::Store,
                  }),
                  stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            
            // Draw main mesh
            self.render_mesh(&mut render_pass, &self.main_mesh);
            
            // Draw lightweight mesh
            self.render_mesh(&mut render_pass, &self.lightweight_mesh);
        }

        // Calculate bytes per row with proper alignment (256-byte boundary for WebGPU)
        let bytes_per_row = 4 * self.texture_size.width;
        let aligned_bytes_per_row = (bytes_per_row + 255) & !255;

        // Copy texture to output buffer with aligned bytes per row
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
                    bytes_per_row: Some(aligned_bytes_per_row),
                    rows_per_image: Some(self.texture_size.height),
                },
            },
            self.texture_size,
        );

        // Submit commands
        self.queue.submit(Some(encoder.finish()));

        // Read data
        let buffer_slice = self.output_buffer.slice(..);
        buffer_slice.map_async(MapMode::Read, |_| {});
        self.device.poll(Maintain::Wait);

        // Get the data and handle alignment if needed
        let data = if aligned_bytes_per_row != bytes_per_row {
            let mut aligned_data = buffer_slice.get_mapped_range();
            let mut data = Vec::with_capacity((bytes_per_row * self.texture_size.height) as usize);
            
            // Extract each row, skipping the padding
            for row in 0..self.texture_size.height {
                let start = row as usize * aligned_bytes_per_row as usize;
                let end = start + bytes_per_row as usize;
                data.extend_from_slice(&aligned_data[start..end]);
            }
            
            // Drop the mapped data before unmapping
            drop(aligned_data);
            data
        } else {
            let data = buffer_slice.get_mapped_range().to_vec();
            data
        };
        
        self.output_buffer.unmap();
        data
    }

    // Private helper method to render a GPU mesh
    fn render_mesh<'a>(&self, render_pass: &mut RenderPass<'a>, mesh: &GPUMesh) {
        if mesh.num_indices > 0 {
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
        }
    }

    // Helper method to update camera buffer
    fn update_camera_buffer(&mut self) {
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.refresh(&self.camera);
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));
    }

    // Helper method to create texture
    fn create_texture(device: &Device, texture_size: &Extent3d) -> Texture {
        device.create_texture(&TextureDescriptor {
            label: Some("Render Target Texture"),
            size: *texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    // Helper method to create depth texture
    fn create_depth_texture(device: &Device, texture_size: &Extent3d) -> Texture {
        device.create_texture(&TextureDescriptor {
            label: Some("Depth Buffer"),
            size: *texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    // Helper method to create output buffer
    fn create_output_buffer(device: &Device, texture_size: &Extent3d) -> Buffer {
        // Calculate aligned bytes per row (must be a multiple of 256 in WebGPU)
        let bytes_per_row = 4 * texture_size.width; // 4 bytes per pixel (RGBA8)
        let aligned_bytes_per_row = (bytes_per_row + 255) & !255;
        let buffer_size = aligned_bytes_per_row * texture_size.height;

        device.create_buffer(&BufferDescriptor {
            label: Some("Output Buffer"),
            size: buffer_size as BufferAddress,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }
}
