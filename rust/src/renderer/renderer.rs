use super::camera::Camera;
use super::gpu_mesh::GPUMesh;
use super::mesh::Mesh;
use super::mesh::Vertex;
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
use crate::renderer::label_atlas::decode_font_atlas;
use crate::renderer::label_mesh::LabelMesh;
use crate::renderer::label_mesh::LabelVertex;
use crate::renderer::line_mesh::LineMesh;
use crate::renderer::line_mesh::LineVertex;
use crate::renderer::transparent_impostor_mesh::TransparentImpostorMesh;
use crate::renderer::transparent_impostor_mesh::TransparentImpostorVertex;
use crate::renderer::transparent_sort::sorted_transparent_indices;
use bytemuck;
use glam::f32::Mat4;
use glam::f32::Vec3;
use glam::f64::DMat4;
use glam::f64::DQuat;
use glam::f64::DVec3;
use glam::f64::DVec4;
use std::sync::Mutex;
use wgpu::util::DeviceExt;
use wgpu::*;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    view_matrix: [[f32; 4]; 4], // Separate view matrix
    proj_matrix: [[f32; 4]; 4], // Separate projection matrix

    camera_position: [f32; 3],
    _padding0: u32,

    head_light_dir: [f32; 3],

    // Orthographic rendering flag (1.0 = orthographic, 0.0 = perspective).
    // No padding before this field: in WGSL the f32 following a vec3<f32>
    // packs into the vec3's 4-byte tail slot, so it must do the same here.
    is_orthographic: f32,

    // Half height for orthographic projection (used for zoom level)
    ortho_half_height: f32,

    // Additional padding to maintain 16-byte struct alignment
    _padding1: [u32; 3],
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
            is_orthographic: 0.0,    // Default to perspective mode
            ortho_half_height: 10.0, // Default orthographic half height
            _padding1: [0, 0, 0],
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

pub struct Renderer {
    device: Device,
    queue: Queue,
    triangle_pipeline: RenderPipeline,
    line_pipeline: RenderPipeline,
    background_line_pipeline: RenderPipeline,
    atom_impostor_pipeline: RenderPipeline,
    bond_impostor_pipeline: RenderPipeline,
    transparent_impostor_pipeline: RenderPipeline,
    /// Atom labels. The only pipeline with a texture, so the only one built on
    /// its own three-group layout (camera, model, atlas) — see
    /// `doc/design_atom_labels.md` §Pipeline changes.
    label_pipeline: RenderPipeline,
    main_mesh: GPUMesh,
    wireframe_mesh: GPUMesh,
    lightweight_mesh: GPUMesh,
    gadget_line_mesh: GPUMesh,
    background_mesh: GPUMesh,
    atom_impostor_mesh: GPUMesh,
    bond_impostor_mesh: GPUMesh,
    transparent_impostor_mesh: GPUMesh,
    /// CPU-side copy of the transparent mesh's per-quad sort centers, retained
    /// so the lazy back-to-front re-sort (§Sorting of `doc/design_xray_node.md`)
    /// can rebuild the index buffer between mesh updates without re-tessellating.
    /// Written on every transparent-mesh upload.
    transparent_quad_centers: Vec<Vec3>,
    /// Bumped whenever a new transparent mesh is uploaded, so the lazy re-sort
    /// knows the centers changed even if the camera has not moved.
    transparent_mesh_generation: u64,
    /// The mesh generation the current sorted index buffer was built for
    /// (`None` = never sorted). A mismatch forces a re-sort.
    transparent_sorted_generation: Option<u64>,
    /// The view matrix the current sorted index buffer was built for
    /// (`None` = never sorted). A change forces a re-sort.
    transparent_sorted_view: Option<Mat4>,
    label_mesh: GPUMesh,
    /// The SDF font atlas' bind group (group 2). It lives on the `Renderer`
    /// rather than on the `GPUMesh`: `GPUMesh` knows only its model bind group
    /// (group 1) and has no notion of a texture, and the atlas is one shared
    /// resource, so it is bound once per pass before the label draw — the way
    /// `camera_bind_group` already is.
    label_atlas_bind_group: wgpu::BindGroup,
    gadget_atom_impostor_mesh: GPUMesh,
    gadget_bond_impostor_mesh: GPUMesh,
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
            orthographic: false,     // Default to perspective mode
            ortho_half_height: 10.0, // Default orthographic half height
            pivot_point: DVec3::new(0.0, 0.0, 0.0),
            nav_up: DVec3::Z,
            nav_up_label: "Z".to_string(),
        };

        // Initialize GPU
        let instance = Instance::default();
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .expect("Failed to find a suitable adapter");
        // Configure custom limits to support larger vertex buffers for complex atomic models
        // Increase max buffer size from default 256 MiB to 1 GiB
        // This allows for much larger atomic crystal models
        let limits = wgpu::Limits {
            max_buffer_size: 1024 * 1024 * 1024, // 1 GiB
            ..Default::default()
        };

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
        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Model Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
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
                }],
            });

        // Initialize meshes with the model_bind_group_layout
        let main_mesh = GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);
        let wireframe_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);

        let lightweight_mesh = GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);
        let gadget_line_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);
        let background_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);

        // Initialize impostor meshes
        let atom_impostor_mesh =
            GPUMesh::new_empty_atom_impostor_mesh(&device, &model_bind_group_layout);
        let bond_impostor_mesh =
            GPUMesh::new_empty_bond_impostor_mesh(&device, &model_bind_group_layout);

        // Merged transparent impostor mesh (x-ray ghost atoms + bonds)
        let transparent_impostor_mesh =
            GPUMesh::new_empty_transparent_impostor_mesh(&device, &model_bind_group_layout);

        // Atom-label glyph quads
        let label_mesh = GPUMesh::new_empty_label_mesh(&device, &model_bind_group_layout);

        // Gadget impostor meshes (rendered in gadget pass, always on top)
        let gadget_atom_impostor_mesh =
            GPUMesh::new_empty_atom_impostor_mesh(&device, &model_bind_group_layout);
        let gadget_bond_impostor_mesh =
            GPUMesh::new_empty_bond_impostor_mesh(&device, &model_bind_group_layout);

        let texture_size = Extent3d {
            width,
            height,
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
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

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

        let transparent_impostor_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Transparent Impostor Shader"),
            source: ShaderSource::Wgsl(include_str!("transparent_impostor.wgsl").into()),
        });

        let label_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Label Shader"),
            source: ShaderSource::Wgsl(include_str!("label.wgsl").into()),
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("camera_bind_group_layout"),
            });

        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("model_bind_group_layout"),
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
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

        // ===== Atom label atlas (the only texture in the renderer) =====
        // Additive: the atlas gets its own group-2 layout and its own pipeline
        // layout, so the shared two-group layout above and the six pipelines
        // built on it are untouched.
        let (label_atlas_bind_group_layout, label_atlas_bind_group) =
            Self::create_label_atlas_binding(&device, &queue);

        let label_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Label Pipeline Layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &model_bind_group_layout,
                &label_atlas_bind_group_layout,
            ],
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
            },
        );
        let background_line_pipeline = Self::create_line_pipeline(
            &device,
            &pipeline_layout,
            wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        );

        // Impostor pipelines
        let atom_impostor_pipeline =
            Self::create_atom_impostor_pipeline(&device, &pipeline_layout, &atom_impostor_shader);

        let bond_impostor_pipeline =
            Self::create_bond_impostor_pipeline(&device, &pipeline_layout, &bond_impostor_shader);

        let transparent_impostor_pipeline = Self::create_transparent_impostor_pipeline(
            &device,
            &pipeline_layout,
            &transparent_impostor_shader,
        );

        let label_pipeline =
            Self::create_label_pipeline(&device, &label_pipeline_layout, &label_shader);

        Self {
            device,
            queue,
            triangle_pipeline,
            line_pipeline,
            background_line_pipeline,
            atom_impostor_pipeline,
            bond_impostor_pipeline,
            transparent_impostor_pipeline,
            label_pipeline,
            main_mesh,
            wireframe_mesh,
            lightweight_mesh,
            gadget_line_mesh,
            background_mesh,
            atom_impostor_mesh,
            bond_impostor_mesh,
            transparent_impostor_mesh,
            transparent_quad_centers: Vec::new(),
            transparent_mesh_generation: 0,
            transparent_sorted_generation: None,
            transparent_sorted_view: None,
            label_mesh,
            label_atlas_bind_group,
            gadget_atom_impostor_mesh,
            gadget_bond_impostor_mesh,
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
            layout: Some(pipeline_layout),
            vertex: VertexState {
                module: &line_shader,
                entry_point: Some("vs_main"),
                buffers: &[LineVertex::desc()],
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
            layout: Some(pipeline_layout),
            vertex: VertexState {
                module: triangle_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: triangle_shader,
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
            layout: Some(pipeline_layout),
            vertex: VertexState {
                module: atom_impostor_shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::renderer::atom_impostor_mesh::AtomImpostorVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: atom_impostor_shader,
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
            layout: Some(pipeline_layout),
            vertex: VertexState {
                module: bond_impostor_shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::renderer::bond_impostor_mesh::BondImpostorVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: bond_impostor_shader,
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

    /// Pipeline for the merged transparent impostor mesh (x-ray ghost atoms +
    /// bonds). Standard alpha blending, depth *test* on but depth *write* off,
    /// so ghosts test against opaque geometry (drawn first with depth writes on)
    /// yet do not occlude one another in the buffer. Culling is disabled: each
    /// quad is a camera-facing billboard whose real shape comes from the
    /// per-fragment ray-cast, so back-face culling could only wrongly drop a
    /// quad. Draw order back-to-front is handled by the index buffer (emission
    /// order for now; Phase 5 adds the depth sort).
    fn create_transparent_impostor_pipeline(
        device: &Device,
        pipeline_layout: &wgpu::PipelineLayout,
        transparent_impostor_shader: &wgpu::ShaderModule,
    ) -> RenderPipeline {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Transparent Impostor Render Pipeline"),
            layout: Some(pipeline_layout),
            vertex: VertexState {
                module: transparent_impostor_shader,
                entry_point: Some("vs_main"),
                buffers: &[TransparentImpostorVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: transparent_impostor_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8Unorm,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
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

    /// Decode the committed SDF font atlas, upload it as an R8 texture, and
    /// build its group-2 bind group (texture + sampler).
    ///
    /// Unlike the render-target texture, this one needs `TEXTURE_BINDING` — it
    /// is sampled rather than drawn into. Linear filtering is what lets one
    /// small atlas stay smooth as a label is zoomed into; the SDF's
    /// `smoothstep` then recovers a crisp edge from the interpolated distance.
    fn create_label_atlas_binding(
        device: &Device,
        queue: &Queue,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let atlas = decode_font_atlas();
        let size = Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Label Font Atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &atlas.data,
            ImageDataLayout {
                offset: 0,
                // R8: one byte per texel, so a row is exactly `width` bytes.
                bytes_per_row: Some(atlas.width),
                rows_per_image: Some(atlas.height),
            },
            size,
        );

        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Label Font Atlas Sampler"),
            // Clamp: glyph cells are interior to the atlas, so wrapping could
            // only ever bleed a neighbouring glyph in at a quad edge.
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("label_atlas_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("label_atlas_bind_group"),
        });

        (layout, bind_group)
    }

    /// Blended, depth-**writing** label pipeline.
    ///
    /// The renderer has no MSAA, so an alpha-tested label would throw away the
    /// antialiasing that is the main reason to use an SDF at all. Blending gives
    /// smooth edges; keeping depth writes on preserves correct occlusion of and
    /// by other scene content, and the shader's `discard` on near-zero coverage
    /// stops empty texels from polluting the depth buffer. Two labels
    /// overlapping each other therefore blend in draw order rather than depth
    /// order — an accepted artifact, and no sort is needed.
    fn create_label_pipeline(
        device: &Device,
        label_pipeline_layout: &wgpu::PipelineLayout,
        label_shader: &wgpu::ShaderModule,
    ) -> RenderPipeline {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Label Render Pipeline"),
            layout: Some(label_pipeline_layout),
            vertex: VertexState {
                module: label_shader,
                entry_point: Some("vs_main"),
                buffers: &[LabelVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: label_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8Unorm,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // A billboard's facing is meaningless, so culling could only
                // wrongly drop a quad.
                cull_mode: None,
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

    /// Gets the current viewport size as (width, height)
    pub fn get_viewport_size(&self) -> (u32, u32) {
        (self.texture_size.width, self.texture_size.height)
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

    #[allow(clippy::too_many_arguments)]
    pub fn update_all_gpu_meshes(
        &mut self,
        lightweight_mesh: &Mesh,
        gadget_line_mesh: &LineMesh,
        main_mesh: &Mesh,
        wireframe_mesh: &LineMesh,
        atom_impostor_mesh: &AtomImpostorMesh,
        bond_impostor_mesh: &BondImpostorMesh,
        transparent_impostor_mesh: &TransparentImpostorMesh,
        label_mesh: &LabelMesh,
        gadget_atom_impostor_mesh: &AtomImpostorMesh,
        gadget_bond_impostor_mesh: &BondImpostorMesh,
        update_non_lightweight: bool,
    ) {
        self.lightweight_mesh
            .update_from_mesh(&self.device, lightweight_mesh, "Lightweight");
        self.lightweight_mesh.set_identity_transform(&self.queue);

        self.gadget_line_mesh
            .update_from_line_mesh(&self.device, gadget_line_mesh, "Gadget Lines");
        self.gadget_line_mesh.set_identity_transform(&self.queue);

        if update_non_lightweight {
            self.main_mesh
                .update_from_mesh(&self.device, main_mesh, "Main");
            self.wireframe_mesh
                .update_from_line_mesh(&self.device, wireframe_mesh, "Wireframe");

            self.atom_impostor_mesh.update_from_atom_impostor_mesh(
                &self.device,
                atom_impostor_mesh,
                "Atom Impostors",
            );
            self.bond_impostor_mesh.update_from_bond_impostor_mesh(
                &self.device,
                bond_impostor_mesh,
                "Bond Impostors",
            );

            self.transparent_impostor_mesh
                .update_from_transparent_impostor_mesh(
                    &self.device,
                    transparent_impostor_mesh,
                    "Transparent Impostors",
                );
            // Retain a CPU copy of the sort centers for the lazy re-sort, and
            // bump the mesh generation so `render` re-sorts against the new
            // geometry even if the camera has not moved. The freshly uploaded
            // index buffer is in emission order; the re-sort replaces it.
            self.transparent_quad_centers
                .clone_from(&transparent_impostor_mesh.quad_centers);
            self.transparent_mesh_generation = self.transparent_mesh_generation.wrapping_add(1);

            self.label_mesh
                .update_from_label_mesh(&self.device, label_mesh, "Atom Labels");

            self.gadget_atom_impostor_mesh
                .update_from_atom_impostor_mesh(
                    &self.device,
                    gadget_atom_impostor_mesh,
                    "Gadget Atom Impostors",
                );
            self.gadget_bond_impostor_mesh
                .update_from_bond_impostor_mesh(
                    &self.device,
                    gadget_bond_impostor_mesh,
                    "Gadget Bond Impostors",
                );

            self.main_mesh.set_identity_transform(&self.queue);
            self.wireframe_mesh.set_identity_transform(&self.queue);

            self.atom_impostor_mesh.set_identity_transform(&self.queue);
            self.bond_impostor_mesh.set_identity_transform(&self.queue);
            self.transparent_impostor_mesh
                .set_identity_transform(&self.queue);
            self.label_mesh.set_identity_transform(&self.queue);
            self.gadget_atom_impostor_mesh
                .set_identity_transform(&self.queue);
            self.gadget_bond_impostor_mesh
                .set_identity_transform(&self.queue);
        }
    }

    pub fn update_background_mesh(&mut self, background_line_mesh: &LineMesh) {
        let _lock = self.render_mutex.lock().unwrap();

        self.background_mesh.update_from_line_mesh(
            &self.device,
            background_line_mesh,
            "Background",
        );
        self.background_mesh.set_identity_transform(&self.queue);
    }

    pub fn render(&mut self, background_color_rgb: [u8; 3]) -> Vec<u8> {
        let _lock = self.render_mutex.lock().unwrap();

        // Lazily re-sort the transparent impostors back-to-front for the current
        // camera before the pass draws them. Recomputes and re-uploads the index
        // buffer only when the camera view has changed or a new transparent mesh
        // was uploaded since the last sort — so a resting camera costs nothing,
        // and orbiting re-sorts once per moved frame (§Sorting of
        // `doc/design_xray_node.md`). The sort is a fixed-size permutation of the
        // existing index buffer, so this `write_buffer` never reallocates. This
        // is written out over disjoint fields (not a `&mut self` helper) because
        // `_lock` holds an immutable borrow of `self` for the whole method.
        if self.transparent_impostor_mesh.num_indices > 0 {
            let view = self.camera.build_view_matrix().as_mat4();
            let up_to_date = self.transparent_sorted_generation
                == Some(self.transparent_mesh_generation)
                && self.transparent_sorted_view == Some(view);
            if !up_to_date {
                let indices = sorted_transparent_indices(&self.transparent_quad_centers, &view);
                self.queue.write_buffer(
                    &self.transparent_impostor_mesh.index_buffer,
                    0,
                    bytemuck::cast_slice(&indices),
                );
                self.transparent_sorted_generation = Some(self.transparent_mesh_generation);
                self.transparent_sorted_view = Some(view);
            }
        }

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

            // Atom labels: after everything opaque (they blend over it) and,
            // critically, BEFORE the transparent pass. The order is forced by
            // the depth-write asymmetry — labels write depth, ghosts do not.
            // Labels-then-ghosts is correct both ways round (a ghost behind a
            // label is depth-rejected at the glyph; a ghost in front passes
            // `Less` and tints the label). Ghosts-then-labels would be wrong: a
            // ghost in front wrote no depth, so the label would pass the test
            // and paint over a ghost that is actually nearer.
            //
            // Group 2 (the font atlas) is bound once here, after the pipeline
            // that declares it — `render_mesh` only knows about group 1.
            self.label_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.label_pipeline);
            render_pass.set_bind_group(2, &self.label_atlas_bind_group, &[]);
            self.render_mesh(&mut render_pass, &self.label_mesh);

            // Transparent impostors (x-ray) draw last in the main pass — after
            // ALL opaque content, including the background lines — with alpha
            // blending and depth writes off. The index buffer is kept in
            // back-to-front order for the current camera by
            // `resort_transparent_indices_if_needed` (called above).
            self.transparent_impostor_mesh
                .set_identity_transform(&self.queue);
            render_pass.set_pipeline(&self.transparent_impostor_pipeline);
            self.render_mesh(&mut render_pass, &self.transparent_impostor_mesh);
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

            // Render gadget atom impostors (guide dots, always on top)
            self.gadget_atom_impostor_mesh
                .set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&self.atom_impostor_pipeline);
            self.render_mesh(&mut gadget_render_pass, &self.gadget_atom_impostor_mesh);

            // Render gadget bond impostors (guide dot connectors, always on top)
            self.gadget_bond_impostor_mesh
                .set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&self.bond_impostor_pipeline);
            self.render_mesh(&mut gadget_render_pass, &self.gadget_bond_impostor_mesh);
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
            buffer_slice.get_mapped_range().to_vec()
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
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[camera_uniform]),
        );
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
    pub fn set_camera_canonical_view(
        &mut self,
        view: crate::renderer::camera::CameraCanonicalView,
    ) {
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
            DVec4::new(0.0, 0.0, 0.0, 1.0),
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
