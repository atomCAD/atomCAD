use wgpu::*;
use bytemuck;
use wgpu::util::DeviceExt;
use super::mesh::Vertex;
use super::mesh::Mesh;
use super::gpu_mesh::GPUMesh;
use super::tessellator::poly_mesh_tessellator::{tessellate_poly_mesh, tessellate_poly_mesh_to_line_mesh};
use crate::renderer::line_mesh::LineVertex;
use crate::renderer::line_mesh::LineMesh;
use super::tessellator::atomic_tessellator;
use super::tessellator::surface_point_tessellator;
use super::tessellator::tessellator::tessellate_cuboid;
use super::camera::Camera;
use glam::f32::Vec3;
use glam::f32::Mat4;
use glam::f64::DMat4;
use glam::f64::DVec3;
use glam::f64::DVec4;
use glam::f64::DQuat;
use crate::common::scene::Scene;
use std::sync::Mutex;
use crate::api::common_api_types::APICameraCanonicalView;
use crate::renderer::mesh::Material;
use crate::api::structure_designer::structure_designer_preferences::GeometryVisualizationPreferences;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

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
    self.view_proj = camera.build_view_projection_matrix().as_mat4().to_cols_array_2d();
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
    main_mesh: GPUMesh,
    wireframe_mesh: GPUMesh,
    selected_clusters_mesh: GPUMesh,
    lightweight_mesh: GPUMesh,
    background_mesh: GPUMesh,
    texture: Texture,
    texture_view: TextureView,
    depth_texture: Texture,
    depth_texture_view: TextureView,
    output_buffer: Buffer,
    pub texture_size: Extent3d,
    pub camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    pub selected_clusters_transform: crate::util::transform::Transform,
    render_mutex: Mutex<()>,
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Self {
        //let start_time = Instant::now();

        let camera = Camera {
          // position the camera at new coordinates
          // +z is out of the screen
          eye: DVec3::new(0.0, 10.0, 30.0),
          // have it look at the origin
          target: DVec3::new(0.0, 0.0, 0.0),
          // calculate up vector perpendicular to (target - eye)
          // The view direction is (0,-10,-30), so a perpendicular vector with positive y is (0,0.95,-0.32)
          up: DVec3::new(0.0, 0.95, -0.32),
          aspect: width as f64 / height as f64,
          fovy: std::f64::consts::PI * 0.15,
          znear: 0.5,
          zfar: 600.0,
          orthographic: false, // Default to perspective mode
          ortho_half_height: 10.0, // Default orthographic half height
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
        let selected_clusters_mesh = GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);
        
        let lightweight_mesh = GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);
        let background_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);

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

        // Triangle pipeline
        let triangle_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Triangle Render Pipeline"),
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
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: -8,
                    slope_scale: -1.0,
                    clamp: 8.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

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

        let result = Self {
          device,
          queue,
          triangle_pipeline,
          line_pipeline,
          background_line_pipeline,
          main_mesh,
          wireframe_mesh,
          selected_clusters_mesh,
          lightweight_mesh,
          background_mesh,
          texture,
          texture_view,
          depth_texture,
          depth_texture_view,
          output_buffer,
          texture_size,
          camera,
          camera_buffer,
          camera_bind_group,
          selected_clusters_transform: crate::util::transform::Transform::default(),
          render_mutex: Mutex::new(()),
        };

        result
    }

    fn create_line_pipeline(
        device: &Device,
        pipeline_layout: &wgpu::PipelineLayout,
        depth_bias_state: wgpu::DepthBiasState,
    ) -> RenderPipeline {
        // Line shader module
        let line_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: ShaderSource::Wgsl(include_str!("line_mesh.wgsl").into()),
        });

        return device.create_render_pipeline(&RenderPipelineDescriptor {
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
        });
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

    pub fn refresh<'a, S: Scene<'a>>(
        &mut self,
        scene: &S,
        lightweight: bool,
        geometry_visualization_preferences: &GeometryVisualizationPreferences
    ) {
        //let start_time = Instant::now();

        // Always refresh lightweight buffers with extra tessellatable data
        let mut lightweight_mesh = Mesh::new();
        if let Some(tessellatable) = scene.tessellatable() {
            tessellatable.tessellate(&mut lightweight_mesh);
        }
        
        // Tessellate camera target sphere if enabled
        if geometry_visualization_preferences.display_camera_target {
            let red_material = Material::new(
                &Vec3::new(1.0, 0.0, 0.0), // Red color
                0.5, // roughness
                0.0, // metallic
            );
            tessellate_cuboid(
                &mut lightweight_mesh,
                &self.camera.target,
                &DVec3::new(0.4, 0.4, 0.4),
                &DQuat::IDENTITY,
                &red_material,
                &red_material,
                &red_material,
            );
        }

        //println!("lightweight tessellated {} vertices and {} indices", 
        //         lightweight_mesh.vertices.len(), lightweight_mesh.indices.len());

        // Update lightweight GPU mesh
        self.lightweight_mesh.update_from_mesh(&self.device, &lightweight_mesh, "Lightweight");
        // Set the identity transform for the lightweight mesh
        self.lightweight_mesh.set_identity_transform(&self.queue);

        // Only refresh main buffers when not in lightweight mode
        if !lightweight {
            // Tessellate everything except tessellatable into main buffers
            let mut mesh = Mesh::new();
            let mut wireframe_mesh = LineMesh::new();
            let mut selected_clusters_mesh = Mesh::new();

            let atomic_tessellation_params = atomic_tessellator::AtomicTessellatorParams {
                sphere_horizontal_divisions: 10,
                sphere_vertical_divisions: 20,
                cylinder_divisions: 16,
            };

            for atomic_structure in scene.atomic_structures() {
                atomic_tessellator::tessellate_atomic_structure(&mut mesh, &mut selected_clusters_mesh, atomic_structure, &atomic_tessellation_params, scene);
            }

            for surface_point_cloud in scene.surface_point_cloud_2ds() {
                surface_point_tessellator::tessellate_surface_point_cloud_2d(&mut mesh, surface_point_cloud);
            }

            for surface_point_cloud in scene.surface_point_clouds() {
                surface_point_tessellator::tessellate_surface_point_cloud(&mut mesh, surface_point_cloud);
            }

            for poly_mesh in scene.poly_meshes() {
                if geometry_visualization_preferences.wireframe_geometry {
                    tessellate_poly_mesh_to_line_mesh(
                        &poly_mesh,
                        &mut wireframe_mesh, 
                        geometry_visualization_preferences.mesh_smoothing.clone(), 
                        Vec3::new(0.0, 0.0, 0.0).to_array(),
                        // normally normal_edge_color should be Vec3::new(0.4, 0.4, 0.4), but we do not show the difference here
                        // as csgrs sometimes creates non-manifold edges (false sharp edges) where it should not.
                        // Fortunatelly csgrs only do this on edges on a plane, so it does not matter for the
                        // solid visualization. 
                        Vec3::new(0.0, 0.0, 0.0).to_array()); 
                } else {
                    tessellate_poly_mesh(
                        &poly_mesh,
                        &mut mesh, 
                        geometry_visualization_preferences.mesh_smoothing.clone(), 
                        &Material::new(
                            &Vec3::new(0.0, 1.0, 0.0), 
                            1.0, 
                            0.0
                        ),
                        Some(&Material::new(
                            &Vec3::new(1.0, 0.0, 0.0), 
                            1.0, 
                            0.0
                        )),
                        Some(&Material::new(
                            &Vec3::new(0.0, 0.0, 1.0), 
                            1.0, 
                            0.0
                        )),
                    );
                }
            }

            //println!("main buffers tessellated {} vertices and {} indices", mesh.vertices.len(), mesh.indices.len());

            // Update main GPU mesh
            self.main_mesh.update_from_mesh(&self.device, &mesh, "Main");
            self.wireframe_mesh.update_from_line_mesh(&self.device, &wireframe_mesh, "Wireframe");
            self.selected_clusters_mesh.update_from_mesh(&self.device, &selected_clusters_mesh, "Selected Clusters");
            
            // Set identity transform for main mesh
            self.main_mesh.set_identity_transform(&self.queue);
            self.wireframe_mesh.set_identity_transform(&self.queue);
            
            // Apply the current selected clusters transform
            self.selected_clusters_mesh.update_transform(&self.queue, &self.selected_clusters_transform);
            
            // Refresh the background coordinate system with the scene's unit cell
            self.refresh_background(scene.get_unit_cell());
        }

        //println!("refresh took: {:?}", start_time.elapsed());
    }

    pub fn refresh_background(&mut self, unit_cell: Option<&UnitCellStruct>) {
        let _lock = self.render_mutex.lock().unwrap();
        
        // Create a new LineMesh for the coordinate system
        let mut line_mesh = LineMesh::new();
        
        // Use the coordinate system tessellator to populate it
        let unit_cell_to_use = unit_cell.cloned().unwrap_or_else(|| UnitCellStruct::cubic_diamond());
        crate::renderer::tessellator::coordinate_system_tessellator::tessellate_coordinate_system(&mut line_mesh, &unit_cell_to_use);
        
        // Update the background mesh with the line mesh
        self.background_mesh.update_from_line_mesh(&self.device, &line_mesh, "Background");
        
        // Set identity transform for the background mesh
        self.background_mesh.set_identity_transform(&self.queue);
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
                            r: 0.6,
                            g: 0.6,
                            b: 0.6,
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

            // Update selected clusters mesh with its transform and render it
            self.selected_clusters_mesh.update_transform(&self.queue, &self.selected_clusters_transform);
            render_pass.set_pipeline(&self.triangle_pipeline);
            self.render_mesh(&mut render_pass, &self.selected_clusters_mesh);
            
            // Set identity transform for lightweight mesh and render it
            self.lightweight_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.triangle_pipeline);
            self.render_mesh(&mut render_pass, &self.lightweight_mesh);

            // Set identity transform for background mesh and render it
            self.background_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.background_line_pipeline);
            self.render_mesh(&mut render_pass, &self.background_mesh);
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
    pub fn set_camera_canonical_view(&mut self, view: APICameraCanonicalView) {
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


    /// Update the transform used for the selected clusters mesh
    pub fn set_selected_clusters_transform(&mut self, transform: &crate::util::transform::Transform) {
        // This should be thread-safe as it's just updating a field value
        // The actual GPU update happens in the render method which is protected by the render_mutex
        self.selected_clusters_transform = transform.clone();
        
        // For immediate visual feedback, we could update the mesh's transform buffer here too
        // But we'll keep it in the render method for consistency and to avoid race conditions
    }

    /// Set the camera from a transform representation
    /// 
    /// The transform's:
    /// - translation becomes the camera eye position
    /// - rotation orients from the identity view (looking down -Z with up as +Y)
    ///   to the desired camera orientation
    pub fn set_camera_transform(&mut self, transform: &crate::util::transform::Transform) {
        // Set eye position directly from translation
        self.camera.eye = transform.translation;

        // The identity view looks down -Z with up as +Y
        let identity_forward = DVec3::new(0.0, 0.0, -1.0);
        let identity_up = DVec3::new(0.0, 1.0, 0.0);

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
