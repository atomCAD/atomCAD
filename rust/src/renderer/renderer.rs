use wgpu::*;
use bytemuck;
use wgpu::util::DeviceExt;
use super::mesh::Vertex;
use super::mesh::Mesh;
use super::gpu_mesh::GPUMesh;
use crate::renderer::line_mesh::LineVertex;
use crate::renderer::line_mesh::LineMesh;
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
use super::camera::Camera;
use glam::f32::Vec3;
use glam::f32::Mat4;
use glam::f64::DMat4;
use glam::f64::DVec3;
use glam::f64::DVec4;
use glam::f64::DQuat;
use std::sync::Mutex;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    view_matrix: [[f32; 4]; 4],        // Separate view matrix
    proj_matrix: [[f32; 4]; 4],        // Separate projection matrix

    camera_position: [f32; 3],
    _padding0: u32,

    head_light_dir: [f32; 3],
    _padding1: u32,

    
    // Orthographic rendering flag (1.0 = orthographic, 0.0 = perspective)
    is_orthographic: f32,
    
    // Half height for orthographic projection (used for zoom level)
    ortho_half_height: f32,
    
    // Additional padding to maintain 16-byte alignment
    _padding2: [u32; 2],
}

impl CameraUniform {
  fn new() -> Self {
      Self {
        view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        view_matrix: Mat4::IDENTITY.to_cols_array_2d(),
        proj_matrix: Mat4::IDENTITY.to_cols_array_2d(),
        camera_position: Vec3::new(0.0, 0.0, 0.0).to_array(),
        _padding0: 0,
        head_light_dir: Vec3::new(0.0, -1.0, 0.0).to_array(),
        _padding1: 0,
        is_orthographic: 0.0,  // Default to perspective mode
        ortho_half_height: 10.0, // Default orthographic half height
        _padding2: [0, 0],
      }
  }

  fn refresh(&mut self, camera: &Camera) {
    let view_matrix = camera.build_view_matrix().as_mat4();
    let proj_matrix = camera.build_projection_matrix().as_mat4();
    let view_proj_matrix = proj_matrix * view_matrix;
    
    self.view_proj = view_proj_matrix.to_cols_array_2d();
    self.view_matrix = view_matrix.to_cols_array_2d();
    self.proj_matrix = proj_matrix.to_cols_array_2d();
    self.camera_position = camera.eye.as_vec3().to_array();
    self.head_light_dir = camera.calc_headlight_direction().as_vec3().to_array();
    self.is_orthographic = if camera.orthographic { 1.0 } else { 0.0 };
    self.ortho_half_height = camera.ortho_half_height as f32;
  }
}

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub struct Renderer  {
    device: Device,
    queue: Queue,
    triangle_pipeline: RenderPipeline,
    line_pipeline: RenderPipeline,
    background_line_pipeline: RenderPipeline,
    atom_impostor_pipeline: RenderPipeline,
    bond_impostor_pipeline: RenderPipeline,
    main_mesh: GPUMesh,
    wireframe_mesh: GPUMesh,
    lightweight_mesh: GPUMesh,
    gadget_line_mesh: GPUMesh,
    background_mesh: GPUMesh,
    atom_impostor_mesh: GPUMesh,
    bond_impostor_mesh: GPUMesh,
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
          // position the camera at new coordinates
          // +z is out of the screen
          eye: DVec3::new(0.0, -30.0, 10.0),
          // have it look at the origin
          target: DVec3::new(0.0, 0.0, 0.0),
          // calculate up vector perpendicular to (target - eye)
          // The view direction is (0,30,-30), so a perpendicular vector 
          // with positive z is (0.0, 0.32, 0.95) 
          up: DVec3::new(0.0, 0.32, 0.95),
          aspect: width as f64 / height as f64,
          fovy: std::f64::consts::PI * 0.15,
          znear: 1.5,
          zfar: 2400.0,
          orthographic: false, // Default to perspective mode
          ortho_half_height: 10.0, // Default orthographic half height
          pivot_point: DVec3::new(0.0, 0.0, 0.0),
        };

        // Initialize GPU
        let instance = Instance::default();
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .expect("Failed to find a suitable adapter");
        // Configure custom limits to support larger vertex buffers for complex atomic models
        let mut limits = wgpu::Limits::default();
        
        // Increase max buffer size from default 256 MiB to 1 GiB
        // This allows for much larger atomic crystal models
        limits.max_buffer_size = 1024 * 1024 * 1024; // 1 GiB
        
        let device_descriptor = DeviceDescriptor {
            label: Some("AtomCAD Renderer Device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::default(),
        };

        let (device, queue) = adapter
            .request_device(&device_descriptor, None)
            .await
            .expect("Failed to create device");

        // Create model bind group layout first, as it's needed for GPUMesh initialization
        let model_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Model Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    // Make sure visibility matches the shader expectations (both vertex and fragment)
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        // Remove explicit min_binding_size to match shader expectation
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        
        // Initialize meshes with the model_bind_group_layout
        let main_mesh = GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);
        let wireframe_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);
        
        let lightweight_mesh = GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);
        let gadget_line_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);
        let background_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);
        
        // Initialize impostor meshes
        let atom_impostor_mesh = GPUMesh::new_empty_atom_impostor_mesh(&device, &model_bind_group_layout);
        let bond_impostor_mesh = GPUMesh::new_empty_bond_impostor_mesh(&device, &model_bind_group_layout);

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

        // Triangle shader module
        let triangle_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
        });

        // Impostor shader modules
        let atom_impostor_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Atom Impostor Shader"),
            source: ShaderSource::Wgsl(include_str!("atom_impostor.wgsl").into()),
        });

        let bond_impostor_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Bond Impostor Shader"),
            source: ShaderSource::Wgsl(include_str!("bond_impostor.wgsl").into()),
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
        
        let model_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
          label: Some("model_bind_group_layout"),
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
        
        // Model bind group layout is already created above
        // Each mesh now has its own model buffer and bind group

        // Pipeline layout - shared between triangle and line pipelines
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &model_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Triangle pipeline (normal depth testing)
        let triangle_pipeline = Self::create_triangle_pipeline(
            &device,
            &pipeline_layout,
            &triangle_shader,
            false, // Normal depth testing
        );

        let line_pipeline = Self::create_line_pipeline(
            &device,
            &pipeline_layout,
            wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            });
        let background_line_pipeline = Self::create_line_pipeline(
            &device,
            &pipeline_layout,
            wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            }
        );

        // Impostor pipelines
        let atom_impostor_pipeline = Self::create_atom_impostor_pipeline(
            &device,
            &pipeline_layout,
            &atom_impostor_shader,
        );

        let bond_impostor_pipeline = Self::create_bond_impostor_pipeline(
            &device,
            &pipeline_layout,
            &bond_impostor_shader,
        );

        let result = Self {
          device,
          queue,
          triangle_pipeline,
          line_pipeline,
          background_line_pipeline,
          atom_impostor_pipeline,
          bond_impostor_pipeline,
          main_mesh,
          wireframe_mesh,
          lightweight_mesh,
          gadget_line_mesh,
          background_mesh,
          atom_impostor_mesh,
          bond_impostor_mesh,
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
        };

        result
    }

    fn create_line_pipeline(
        device: &Device,
        pipeline_layout: &wgpu::PipelineLayout,
        depth_bias_state: wgpu::DepthBiasState,
    ) -> RenderPipeline {
        let line_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: ShaderSource::Wgsl(include_str!("line_mesh.wgsl").into()),
        });

        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Line Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &line_shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    LineVertex::desc(),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &line_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8Unorm,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Don't cull lines
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: depth_bias_state,
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    fn create_triangle_pipeline(
        device: &Device,
        pipeline_layout: &wgpu::PipelineLayout,
        triangle_shader: &wgpu::ShaderModule,
        always_on_top: bool,
    ) -> RenderPipeline {
        let depth_stencil = if always_on_top {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false, // Don't write to depth buffer
                depth_compare: wgpu::CompareFunction::Always, // Always pass depth test
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            })
        } else {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: -8,
                    slope_scale: -1.0,
                    clamp: 8.0,
                },
            })
        };

        let label = if always_on_top {
            "Gadget Triangle Render Pipeline"
        } else {
            "Triangle Render Pipeline"
        };

        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &triangle_shader,
                entry_point: Some("vs_main"),
                buffers: &[
                  Vertex::desc(),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &triangle_shader,
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
              polygon_mode: wgpu::PolygonMode::Fill,
              unclipped_depth: false,
              conservative: false,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    fn create_atom_impostor_pipeline(
        device: &Device,
        pipeline_layout: &wgpu::PipelineLayout,
        atom_impostor_shader: &wgpu::ShaderModule,
    ) -> RenderPipeline {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Atom Impostor Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &atom_impostor_shader,
                entry_point: Some("vs_main"),
                buffers: &[
                  crate::renderer::atom_impostor_mesh::AtomImpostorVertex::desc(),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &atom_impostor_shader,
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
              polygon_mode: wgpu::PolygonMode::Fill,
              unclipped_depth: false,
              conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    fn create_bond_impostor_pipeline(
        device: &Device,
        pipeline_layout: &wgpu::PipelineLayout,
        bond_impostor_shader: &wgpu::ShaderModule,
    ) -> RenderPipeline {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Bond Impostor Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &bond_impostor_shader,
                entry_point: Some("vs_main"),
                buffers: &[
                  crate::renderer::bond_impostor_mesh::BondImpostorVertex::desc(),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &bond_impostor_shader,
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
              polygon_mode: wgpu::PolygonMode::Fill,
              unclipped_depth: false,
              conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    pub fn move_camera(&mut self, eye: &DVec3, target: &DVec3, up: &DVec3) {
      self.camera.eye = *eye;
      self.camera.target = *target;
      self.camera.up = *up;

      self.update_camera_buffer();
    }

    pub fn set_viewport_size(&mut self, width: u32, height: u32) {
        if self.texture_size.width == width && self.texture_size.height == height {
            return;
        }

        self.camera.aspect = width as f64 / height as f64;
        self.update_camera_buffer();

        let _lock = self.render_mutex.lock().unwrap();

        self.device.poll(Maintain::Wait);

        self.texture_size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

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

        self.device.poll(Maintain::Wait);
    }

    pub fn update_all_gpu_meshes(
        &mut self,
        lightweight_mesh: &Mesh,
        gadget_line_mesh: &LineMesh,
        main_mesh: &Mesh,
        wireframe_mesh: &LineMesh,
        atom_impostor_mesh: &AtomImpostorMesh,
        bond_impostor_mesh: &BondImpostorMesh,
        update_non_lightweight: bool
    ) {
        self.lightweight_mesh.update_from_mesh(&self.device, lightweight_mesh, "Lightweight");
        self.lightweight_mesh.set_identity_transform(&self.queue);

        self.gadget_line_mesh.update_from_line_mesh(&self.device, gadget_line_mesh, "Gadget Lines");
        self.gadget_line_mesh.set_identity_transform(&self.queue);

        if update_non_lightweight {
            self.main_mesh.update_from_mesh(&self.device, main_mesh, "Main");
            self.wireframe_mesh.update_from_line_mesh(&self.device, wireframe_mesh, "Wireframe");
            
            self.atom_impostor_mesh.update_from_atom_impostor_mesh(&self.device, atom_impostor_mesh, "Atom Impostors");
            self.bond_impostor_mesh.update_from_bond_impostor_mesh(&self.device, bond_impostor_mesh, "Bond Impostors");
            
            self.main_mesh.set_identity_transform(&self.queue);
            self.wireframe_mesh.set_identity_transform(&self.queue);
            
            self.atom_impostor_mesh.set_identity_transform(&self.queue);
            self.bond_impostor_mesh.set_identity_transform(&self.queue);
        }
    }

    pub fn update_background_mesh(&mut self, background_line_mesh: &LineMesh) {
        let _lock = self.render_mutex.lock().unwrap();

        self.background_mesh.update_from_line_mesh(&self.device, background_line_mesh, "Background");
        self.background_mesh.set_identity_transform(&self.queue);
    }

    pub fn render(&mut self, background_color_rgb: [u8; 3]) -> Vec<u8> {
        let _lock = self.render_mutex.lock().unwrap();

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Command Encoder"),
            });

        // Convert background color from 0-255 RGB to 0.0-1.0 range
        let bg_color = Color {
            r: background_color_rgb[0] as f64 / 255.0,
            g: background_color_rgb[1] as f64 / 255.0,
            b: background_color_rgb[2] as f64 / 255.0,
            a: 1.0,
        };

        // Render pass
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(bg_color),
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

            // Set camera bind group (shared for both pipelines)
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);

            // Each mesh now has its own model bind group

            // Set identity transform for wireframe mesh and render it
            self.wireframe_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.line_pipeline);
            self.render_mesh(&mut render_pass, &self.wireframe_mesh);

            // Set identity transform for main mesh and render it
            self.main_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.triangle_pipeline);
            self.render_mesh(&mut render_pass, &self.main_mesh);

            // Render impostor meshes (if they have data)
            // Note: Impostor meshes are only populated when AtomicRenderingMethod::Impostors is selected
            self.atom_impostor_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.atom_impostor_pipeline);
            self.render_mesh(&mut render_pass, &self.atom_impostor_mesh);

            self.bond_impostor_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.bond_impostor_pipeline);
            self.render_mesh(&mut render_pass, &self.bond_impostor_mesh);

            // Set identity transform for background mesh and render it
            self.background_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.background_line_pipeline);
            self.render_mesh(&mut render_pass, &self.background_mesh);
        }

        // Second render pass for gadgets - clear depth buffer but preserve color
        {
            let mut gadget_render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Gadget Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &self.texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing color buffer
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                  view: &self.depth_texture_view,
                  depth_ops: Some(wgpu::Operations {
                      load: wgpu::LoadOp::Clear(1.0), // Clear depth buffer for gadgets
                      store: wgpu::StoreOp::Store,
                  }),
                  stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set camera bind group for gadget render pass
            gadget_render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            
            // Render gadget lines first
            self.gadget_line_mesh.set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&self.line_pipeline);
            self.render_mesh(&mut gadget_render_pass, &self.gadget_line_mesh);

            // Render gadget triangles
            self.lightweight_mesh.set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&self.triangle_pipeline);
            self.render_mesh(&mut gadget_render_pass, &self.lightweight_mesh);
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
            let aligned_data = buffer_slice.get_mapped_range();
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
            // Set the mesh's model bind group (index 1)
            render_pass.set_bind_group(1, &mesh.model_bind_group, &[]);
            
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
        }
    }

    // Helper method to update camera buffer
    pub fn update_camera_buffer(&mut self) {
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.refresh(&self.camera);
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));
    }
    
    /// Sets the camera to orthographic or perspective mode
    pub fn set_orthographic_mode(&mut self, orthographic: bool) {
        self.camera.orthographic = orthographic;
        self.update_camera_buffer();
    }
    
    /// Gets the current projection mode
    pub fn is_orthographic(&self) -> bool {
        self.camera.orthographic
    }
    
    /// Sets the orthographic half height (controls zoom level in orthographic mode)
    pub fn set_ortho_half_height(&mut self, half_height: f64) {
        self.camera.ortho_half_height = half_height;
        self.update_camera_buffer();
    }
    
    /// Gets the current orthographic half height
    pub fn get_ortho_half_height(&self) -> f64 {
        self.camera.ortho_half_height
    }
    
    /// Sets the camera to a canonical view
    pub fn set_camera_canonical_view(&mut self, view: crate::renderer::camera::CameraCanonicalView) {
        self.camera.set_canonical_view(view);
        self.update_camera_buffer();
    }
    
    // These methods are no longer needed as each mesh now manages its own model buffer

    /// Get the camera transform representation
    /// 
    /// Returns a Transform where:
    /// - translation corresponds to the camera eye position
    /// - rotation orients from the identity orientation (looking down -Z with up as +Y)
    ///   to the current camera orientation
    pub fn get_camera_transform(&self) -> crate::util::transform::Transform {
        use crate::util::transform::Transform;

        // For the camera transform, translation is just the eye position
        let translation = self.camera.eye;

        // Calculate the rotation quaternion that transforms from the identity orientation
        // (where forward is -Z and up is +Y) to the current camera orientation

        // First, calculate the camera's basis vectors
        let forward = (self.camera.target - self.camera.eye).normalize();
        let right = forward.cross(self.camera.up).normalize();
        let up = right.cross(forward).normalize(); // Recalculate up to ensure orthogonality

        // The identity orientation has:
        // forward = (0, 0, -1)
        // up = (0, 1, 0)
        // right = (1, 0, 0)

        // Create a rotation that transforms from identity to current orientation
        // We need to construct a quaternion from these basis vectors
        let mat = DMat4::from_cols(
            DVec4::new(right.x, right.y, right.z, 0.0),
            DVec4::new(up.x, up.y, up.z, 0.0),
            DVec4::new(-forward.x, -forward.y, -forward.z, 0.0), // Negate forward since -Z is forward
            DVec4::new(0.0, 0.0, 0.0, 1.0)
        );

        // Extract quaternion from the rotation matrix
        let rotation = DQuat::from_mat4(&mat);

        Transform::new(translation, rotation)
    }



    /// Set the camera from a transform representation
    /// 
    /// The transform's:
    /// - translation becomes the camera eye position
    /// - rotation orients from the identity view (looking down Y with up as +Z)
    ///   to the desired camera orientation
    pub fn set_camera_transform(&mut self, transform: &crate::util::transform::Transform) {
        // Set eye position directly from translation
        self.camera.eye = transform.translation;

        // The identity view looks down Y with up as +Z (Z-up coordinate system)
        let identity_forward = DVec3::new(0.0, 1.0, 0.0);
        let identity_up = DVec3::new(0.0, 0.0, 1.0);

        // Apply rotation to get current orientation vectors
        let forward = transform.rotation.mul_vec3(identity_forward);
        let up = transform.rotation.mul_vec3(identity_up);

        // Calculate target from eye and forward
        self.camera.target = self.camera.eye + forward;
        self.camera.up = up;

        // Update the GPU buffers
        self.update_camera_buffer();
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
