use super::camera::Camera;
use super::gpu_mesh::GPUMesh;
use super::mesh::Mesh;
use crate::atom_impostor_mesh::AtomImpostorMesh;
use crate::bond_impostor_mesh::BondImpostorMesh;
use crate::label_atlas::decode_font_atlas;
use crate::label_mesh::LabelMesh;
use crate::line_lod::{LINE_LOD_SLOTS, LodSlot, SlotLod, lod_uniform_entries};
use crate::line_mesh::LineMesh;
use crate::pipelines::{PipelineLayouts, PipelineSet, Shaders};
use crate::render_target::{
    COLOR_FORMAT, DEPTH_FORMAT, RenderQuality, RenderTarget, TargetSpec, live_target_spec,
};
use crate::surface_sort::sorted_component_order;
use crate::thumbnail::ThumbnailBackground;
use crate::transparent_impostor_mesh::TransparentImpostorMesh;
use crate::transparent_sort::sorted_transparent_indices;
use crate::transparent_surface_mesh::{
    SurfaceComponentRange, SurfaceTransparencyMode, TransparentSurfaceMesh,
};
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

/// The per-view uniform every shader binds at group 0.
///
/// Six `.wgsl` files declare the same struct and must agree with this one
/// byte for byte: issue #269 was a mismatch of exactly this kind. The padding
/// is therefore explicit, `tests/renderer/shader_test.rs` checks the offsets
/// naga computes for each shader against `offset_of!` here, and
/// `tests/renderer/camera_uniform_test.rs` pins the size and the new fields.
///
/// | Field | Offset |
/// |---|---|
/// | `view_proj`, `view_matrix`, `proj_matrix` | 0, 64, 128 |
/// | `camera_position` | 192 |
/// | `head_light_dir`, `is_orthographic` | 208, 220 |
/// | `ortho_half_height` | 224 |
/// | `viewport_size_px` | 232 |
/// | `pixel_scale` | 240 |
/// | *size* | 256 |
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view_matrix: [[f32; 4]; 4], // Separate view matrix
    pub proj_matrix: [[f32; 4]; 4], // Separate projection matrix

    pub camera_position: [f32; 3],
    pub _padding0: u32,

    pub head_light_dir: [f32; 3],

    // Orthographic rendering flag (1.0 = orthographic, 0.0 = perspective).
    // No padding before this field: in WGSL the f32 following a vec3<f32>
    // packs into the vec3's 4-byte tail slot, so it must do the same here.
    pub is_orthographic: f32,

    // Half height for orthographic projection (used for zoom level)
    pub ortho_half_height: f32,

    // WGSL aligns a vec2<f32> to 8 bytes, so it starts at 232, not 228.
    pub _padding1: u32,

    /// Size of the render target being drawn into, in render pixels
    /// (`doc/design_screen_space_lines.md` D3).
    pub viewport_size_px: [f32; 2],

    /// Render-target pixels per output pixel: the pixel scale of the
    /// [`RenderTarget`] drawn into (D2).
    pub pixel_scale: f32,

    // The struct's size rounds up to its 16-byte alignment (the matrices').
    pub _padding2: [u32; 3],
}

impl CameraUniform {
    /// The uniform for drawing `camera` into a target of `target_size`
    /// render pixels at `pixel_scale` (D3). Pure, so it can be tested without
    /// a device; `Renderer::write_view_uniforms` is the only place that
    /// writes one to the GPU.
    pub fn new(camera: &Camera, target_size: (u32, u32), pixel_scale: f32) -> Self {
        let view_matrix = camera.build_view_matrix().as_mat4();
        let proj_matrix = camera.build_projection_matrix().as_mat4();
        let view_proj_matrix = proj_matrix * view_matrix;

        Self {
            view_proj: view_proj_matrix.to_cols_array_2d(),
            view_matrix: view_matrix.to_cols_array_2d(),
            proj_matrix: proj_matrix.to_cols_array_2d(),
            camera_position: camera.eye.as_vec3().to_array(),
            _padding0: 0,
            head_light_dir: camera.calc_headlight_direction().as_vec3().to_array(),
            is_orthographic: if camera.orthographic { 1.0 } else { 0.0 },
            ortho_half_height: camera.ortho_half_height as f32,
            _padding1: 0,
            viewport_size_px: [target_size.0 as f32, target_size.1 as f32],
            pixel_scale,
            _padding2: [0, 0, 0],
        }
    }
}

/// Straight (not premultiplied) alpha blending, the blend state of every
/// blended pipeline: lines, labels, transparent impostors and transparent
/// surfaces (`doc/design_screen_space_lines.md` D6).
///
/// Colour is `SrcAlpha, OneMinusSrcAlpha`. Alpha is `One, OneMinusSrcAlpha`,
/// so a translucent fragment over the opaque clear leaves the destination
/// opaque: readback keeps alpha, and a `SrcAlpha` alpha factor would make
/// every anti-aliased pixel of an exported PNG or a thumbnail translucent.
/// wgpu's `BlendState::ALPHA_BLENDING` is exactly this (its alpha component
/// is `BlendComponent::OVER`); `tests/renderer/line_mesh_test.rs` pins it, so
/// a wgpu upgrade that changed it would fail there.
pub const STRAIGHT_ALPHA_BLENDING: BlendState = BlendState::ALPHA_BLENDING;

/// Which set of per-view uniforms a draw reads: the live viewport's, or the
/// one set shared by the thumbnail and the image export, which is rewritten
/// before every use. Each camera uniform and each line mesh's
/// `LineLodUniform` exists once per view, so an offscreen render never leaves
/// its values behind for the next live frame.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Live,
    Offscreen,
}

/// The `LineLodUniform`s of one line mesh that has grid level-of-detail slots
/// (`doc/design_screen_space_lines.md` D9): one per [`View`], filled by
/// `Renderer::write_view_uniforms`.
pub(crate) struct LineLodBinding {
    slots: Vec<LodSlot>,
    /// Each slot's visibility in the last live frame, for the hysteresis of
    /// `slot_lod`. Only the live view reads or writes it. A mutex because it
    /// changes under `&self`, while the render lock is held.
    was_visible: Mutex<Vec<Option<bool>>>,
    live: (Buffer, wgpu::BindGroup),
    offscreen: (Buffer, wgpu::BindGroup),
}

impl LineLodBinding {
    fn new(device: &Device, layout: &wgpu::BindGroupLayout, slots: Vec<LodSlot>) -> Self {
        Self {
            slots,
            was_visible: Mutex::new(Vec::new()),
            live: Renderer::create_line_lod_uniform(device, layout, "Live Line LOD"),
            offscreen: Renderer::create_line_lod_uniform(device, layout, "Offscreen Line LOD"),
        }
    }

    fn uniform(&self, view: View) -> &(Buffer, wgpu::BindGroup) {
        match view {
            View::Live => &self.live,
            View::Offscreen => &self.offscreen,
        }
    }
}

/// Which parts of the scene a draw includes.
#[derive(Clone, Copy)]
struct PassFlags {
    /// The editing aids: background grid lines, drawing-plane grids and atom
    /// labels. Off for a thumbnail, which shows only the content (D5).
    editing_aids: bool,
    /// The whole gadget pass: gadgets (the translate/rotate gizmo, guide dots)
    /// and the lightweight pivot cube. On only for the live viewport — neither
    /// a thumbnail nor an exported viewport image shows them.
    gadgets: bool,
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    /// The pipelines at [`LIVE_SAMPLE_COUNT`](crate::render_target::LIVE_SAMPLE_COUNT)
    /// samples, for the multisampled live target
    /// (`doc/design_viewport_antialiasing.md` D2). `None` when the adapter
    /// cannot multisample the colour or depth format; the live target then
    /// always has one sample.
    live_pipelines: Option<PipelineSet>,
    /// The single-sample pipelines: the thumbnail, the export, and the live
    /// target with anti-aliasing off.
    offscreen_pipelines: PipelineSet,
    /// The live viewport's size as last requested by `set_viewport_size`, in
    /// logical pixels, and the quality settings pushed in from the
    /// preferences (D8). The live target is derived from both.
    viewport_request: (f64, f64),
    quality: RenderQuality,
    main_mesh: GPUMesh,
    wireframe_mesh: GPUMesh,
    /// Drawing-plane grids and axes: drawn with the content in the live view,
    /// but an editing aid — not in a thumbnail and not in `content_bounds`.
    reference_line_mesh: GPUMesh,
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
    /// Merged transparent isosurface mesh: every displayed surface with
    /// `alpha < 1.0`. Surfaces at `alpha >= 1.0` go to `main_mesh` instead and
    /// are drawn by the opaque `triangle_pipeline`, which is what makes the
    /// opaque fast path a routing decision rather than a shader branch.
    isosurface_transparent_mesh: GPUMesh,
    /// CPU-side pool of that mesh's component ranges, retained so the
    /// back-to-front component order can be recomputed per moved frame without
    /// re-tessellating. Written on every transparent-surface upload.
    isosurface_components: Vec<SurfaceComponentRange>,
    /// Draw strategy for the mesh above. A preference, pushed in from the api
    /// layer on every refresh rather than read from a preferences type the
    /// renderer crate cannot see.
    surface_transparency_mode: SurfaceTransparencyMode,
    label_mesh: GPUMesh,
    /// The SDF font atlas' bind group (group 2). It lives on the `Renderer`
    /// rather than on the `GPUMesh`: `GPUMesh` knows only its model bind group
    /// (group 1) and has no notion of a texture, and the atlas is one shared
    /// resource, so it is bound once per pass before the label draw — the way
    /// `camera_bind_group` already is.
    label_atlas_bind_group: wgpu::BindGroup,
    /// The line pipeline's group-2 layout, kept for the `LineLodUniform`s of
    /// meshes that have LOD slots (D9).
    line_lod_bind_group_layout: wgpu::BindGroupLayout,
    /// The line pipeline's group 2 for a mesh without LOD slots: a
    /// `LineLodUniform` holding "visible, no fade" in every entry (D9).
    default_line_lod_bind_group: wgpu::BindGroup,
    gadget_atom_impostor_mesh: GPUMesh,
    gadget_bond_impostor_mesh: GPUMesh,
    /// The live viewport's render target.
    target: RenderTarget,
    pub camera: Camera,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// The thumbnail's own render target and camera uniform, created once and
    /// reused, so a capture never touches the live target or the live camera
    /// buffer (`doc/design_network_thumbnails.md` D5).
    thumbnail_target: RenderTarget,
    thumbnail_camera_buffer: wgpu::Buffer,
    thumbnail_camera_bind_group: wgpu::BindGroup,
    /// Axis-aligned bounds of the content meshes on the GPU, recorded at
    /// upload (D4). `None` when the content is empty.
    content_bounds: Option<(DVec3, DVec3)>,
    /// Which network the content meshes on the GPU belong to, as
    /// `(document id, network name)` (D2). The document id is the structure
    /// designer's `DocumentId`, which this crate cannot name. Set by every
    /// content upload; `None` only when no network was active.
    content_owner: Option<(u64, String)>,
    render_mutex: Mutex<()>,
}

impl Renderer {
    pub async fn new(width: u32, height: u32) -> Self {
        //let start_time = Instant::now();

        let camera = Camera::default_pose(width as f64 / height as f64);

        // Initialize GPU. Ask for the high-performance adapter: on a laptop
        // with both, the default is the integrated GPU, which renders a large
        // crystal about 5x slower than the discrete one. A machine with only
        // one adapter gets that one either way.
        let instance = Instance::default();
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                ..Default::default()
            })
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
        let reference_line_mesh = GPUMesh::new_empty_line_mesh(&device, &model_bind_group_layout);

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

        // Merged transparent isosurface mesh. An ordinary triangle mesh — same
        // `Vertex`, same shader as the opaque one; only the pipelines it is
        // drawn with differ.
        let isosurface_transparent_mesh =
            GPUMesh::new_empty_triangle_mesh(&device, &model_bind_group_layout);

        // Atom-label glyph quads
        let label_mesh = GPUMesh::new_empty_label_mesh(&device, &model_bind_group_layout);

        // Gadget impostor meshes (rendered in gadget pass, always on top)
        let gadget_atom_impostor_mesh =
            GPUMesh::new_empty_atom_impostor_mesh(&device, &model_bind_group_layout);
        let gadget_bond_impostor_mesh =
            GPUMesh::new_empty_bond_impostor_mesh(&device, &model_bind_group_layout);

        // D2: both attachment formats must support 4 samples, or there is no
        // multisampled live target at all.
        let msaa_supported = [COLOR_FORMAT, DEPTH_FORMAT].iter().all(|&format| {
            adapter
                .get_texture_format_features(format)
                .flags
                .sample_count_supported(crate::render_target::LIVE_SAMPLE_COUNT)
        });
        if !msaa_supported {
            println!(
                "[renderer] 4x MSAA is not supported by this adapter; anti-aliasing is unavailable"
            );
        }

        let viewport_request = (width as f64, height as f64);
        let quality = RenderQuality::default();
        let target = RenderTarget::new(
            &device,
            live_target_spec(
                viewport_request.0,
                viewport_request.1,
                quality,
                msaa_supported,
            ),
        );
        let thumbnail_target = RenderTarget::new(
            &device,
            TargetSpec::image(
                crate::thumbnail::THUMBNAIL_SIZE,
                crate::thumbnail::THUMBNAIL_SIZE,
                crate::thumbnail::THUMBNAIL_SUPERSAMPLING,
            ),
        );

        // The initial contents of both camera buffers. The thumbnail one is
        // rewritten for its own target before every use.
        let camera_uniform = CameraUniform::new(
            &camera,
            (target.size.width, target.size.height),
            target.pixel_scale,
        );
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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

        let thumbnail_camera_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Thumbnail Camera Buffer"),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let thumbnail_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: thumbnail_camera_buffer.as_entire_binding(),
            }],
            label: Some("thumbnail_camera_bind_group"),
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

        // ===== Lines (doc/design_screen_space_lines.md D6, D9) =====
        let (line_lod_bind_group_layout, default_line_lod_bind_group) =
            Self::create_line_lod_binding(&device);
        let line_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &model_bind_group_layout,
                &line_lod_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let shaders = Shaders::new(&device);
        let layouts = PipelineLayouts {
            main: pipeline_layout,
            line: line_pipeline_layout,
            label: label_pipeline_layout,
        };
        let live_pipelines = msaa_supported.then(|| {
            PipelineSet::new(
                &device,
                &layouts,
                &shaders,
                crate::render_target::LIVE_SAMPLE_COUNT,
            )
        });
        let offscreen_pipelines = PipelineSet::new(&device, &layouts, &shaders, 1);

        Self {
            device,
            queue,
            live_pipelines,
            offscreen_pipelines,
            viewport_request,
            quality,
            main_mesh,
            wireframe_mesh,
            reference_line_mesh,
            lightweight_mesh,
            gadget_line_mesh,
            background_mesh,
            atom_impostor_mesh,
            bond_impostor_mesh,
            transparent_impostor_mesh,
            isosurface_transparent_mesh,
            isosurface_components: Vec::new(),
            surface_transparency_mode: SurfaceTransparencyMode::default(),
            transparent_quad_centers: Vec::new(),
            transparent_mesh_generation: 0,
            transparent_sorted_generation: None,
            transparent_sorted_view: None,
            label_mesh,
            label_atlas_bind_group,
            line_lod_bind_group_layout,
            default_line_lod_bind_group,
            gadget_atom_impostor_mesh,
            gadget_bond_impostor_mesh,
            target,
            camera,
            camera_buffer,
            camera_bind_group,
            thumbnail_target,
            thumbnail_camera_buffer,
            thumbnail_camera_bind_group,
            content_bounds: None,
            content_owner: None,
            render_mutex: Mutex::new(()),
        }
    }

    /// The line pipeline's group-2 layout and the one shared `LineLodUniform`
    /// bound there: `(1, 0, 0, 0)` — visible, no fade — in every entry (D9).
    fn create_line_lod_binding(device: &Device) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line_lod_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let entries = [SlotLod::ALWAYS.to_gpu(); LINE_LOD_SLOTS];
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Default Line LOD Buffer"),
            contents: bytemuck::cast_slice(&entries),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("default_line_lod_bind_group"),
        });
        (layout, bind_group)
    }

    /// One writable `LineLodUniform` and its group-2 bind group (D9).
    fn create_line_lod_uniform(
        device: &Device,
        layout: &wgpu::BindGroupLayout,
        label: &str,
    ) -> (Buffer, wgpu::BindGroup) {
        let entries = [SlotLod::ALWAYS.to_gpu(); LINE_LOD_SLOTS];
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(&entries),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some(label),
        });
        (buffer, bind_group)
    }

    /// Gives `mesh` the `LineLodUniform`s its freshly uploaded `line_mesh`
    /// needs (D9): none without LOD slots; otherwise the existing ones are
    /// kept and only the slot table is replaced. The hysteresis state
    /// survives a re-upload of the same slots (the grids are re-tessellated
    /// on every refresh) and is reset when they change.
    fn attach_line_lod(
        device: &Device,
        layout: &wgpu::BindGroupLayout,
        mesh: &mut GPUMesh,
        line_mesh: &LineMesh,
    ) {
        if line_mesh.lod_slots.is_empty() {
            mesh.line_lod = None;
            return;
        }
        match &mut mesh.line_lod {
            Some(binding) => {
                if binding.slots != line_mesh.lod_slots {
                    binding.slots.clone_from(&line_mesh.lod_slots);
                    binding.was_visible.lock().unwrap().clear();
                }
            }
            None => {
                mesh.line_lod = Some(LineLodBinding::new(
                    device,
                    layout,
                    line_mesh.lod_slots.clone(),
                ));
            }
        }
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

    /// Writes a camera pose. `up` is taken as the caller's *intent* for the
    /// roll and is orthonormalized against the new view direction, so callers
    /// that pass a world axis (the CLI's `camera --up`, a loaded project) leave
    /// the same canonical pose behind as the turntable does — see
    /// [`Camera::orthonormalize_up`] for why a stored world axis breaks
    /// viewport picking.
    pub fn move_camera(&mut self, eye: &DVec3, target: &DVec3, up: &DVec3) {
        self.camera.eye = *eye;
        self.camera.target = *target;
        self.camera.up = *up;
        self.camera.orthonormalize_up();

        self.update_camera_buffer();
    }

    /// Gets the current viewport size as (width, height)
    pub fn get_viewport_size(&self) -> (u32, u32) {
        (self.target.size.width, self.target.size.height)
    }

    pub fn set_viewport_size(&mut self, width: u32, height: u32) {
        self.viewport_request = (width as f64, height as f64);
        self.update_live_target();
    }

    /// Sets the live viewport's quality settings, the *Rendering* preferences
    /// (`doc/design_viewport_antialiasing.md` D8). Pushed in from the api on
    /// every refresh; recreates the live target only when it changes.
    pub fn set_render_quality(&mut self, quality: RenderQuality) {
        self.quality = quality;
        self.update_live_target();
    }

    /// The live viewport's quality settings.
    pub fn render_quality(&self) -> RenderQuality {
        self.quality
    }

    /// Whether the adapter can multisample the live target (D2). Without it
    /// the live target has one sample whatever the quality settings say.
    pub fn msaa_supported(&self) -> bool {
        self.live_pipelines.is_some()
    }

    /// Samples per pixel of the live target: 4 with anti-aliasing on (and
    /// supported), otherwise 1.
    pub fn live_sample_count(&self) -> u32 {
        self.target.sample_count
    }

    /// Recreates the live target when the spec derived from the viewport
    /// request and the quality settings differs from the current one, so the
    /// push on every refresh costs one comparison.
    fn update_live_target(&mut self) {
        let spec = live_target_spec(
            self.viewport_request.0,
            self.viewport_request.1,
            self.quality,
            self.live_pipelines.is_some(),
        );
        if spec == self.target.spec() {
            return;
        }

        self.camera.aspect = spec.size.width as f64 / spec.size.height as f64;

        let _lock = self.render_mutex.lock().unwrap();

        self.device.poll(Maintain::Wait);

        self.target = RenderTarget::new(&self.device, spec);

        self.device.poll(Maintain::Wait);

        // After the target, not before: the uniform carries its size (D3).
        self.write_view_uniforms(View::Live, &self.camera, &self.target);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_all_gpu_meshes(
        &mut self,
        lightweight_mesh: &Mesh,
        gadget_line_mesh: &LineMesh,
        main_mesh: &Mesh,
        wireframe_mesh: &LineMesh,
        reference_line_mesh: &LineMesh,
        atom_impostor_mesh: &AtomImpostorMesh,
        bond_impostor_mesh: &BondImpostorMesh,
        transparent_impostor_mesh: &TransparentImpostorMesh,
        isosurface_transparent_mesh: &TransparentSurfaceMesh,
        label_mesh: &LabelMesh,
        gadget_atom_impostor_mesh: &AtomImpostorMesh,
        gadget_bond_impostor_mesh: &BondImpostorMesh,
        update_non_lightweight: bool,
        content_owner: Option<(u64, String)>,
    ) {
        self.lightweight_mesh
            .update_from_mesh(&self.device, lightweight_mesh, "Lightweight");
        self.lightweight_mesh.set_identity_transform(&self.queue);

        self.gadget_line_mesh
            .update_from_line_mesh(&self.device, gadget_line_mesh, "Gadget Lines");
        Self::attach_line_lod(
            &self.device,
            &self.line_lod_bind_group_layout,
            &mut self.gadget_line_mesh,
            gadget_line_mesh,
        );
        self.gadget_line_mesh.set_identity_transform(&self.queue);

        if update_non_lightweight {
            // The content changes hands here, and only here (D2, D4).
            self.content_owner = content_owner;
            self.content_bounds = crate::thumbnail::content_bounds(
                main_mesh,
                wireframe_mesh,
                atom_impostor_mesh,
                bond_impostor_mesh,
                transparent_impostor_mesh,
                &isosurface_transparent_mesh.mesh,
            );

            self.main_mesh
                .update_from_mesh(&self.device, main_mesh, "Main");
            self.wireframe_mesh
                .update_from_line_mesh(&self.device, wireframe_mesh, "Wireframe");
            self.reference_line_mesh.update_from_line_mesh(
                &self.device,
                reference_line_mesh,
                "Reference Lines",
            );
            Self::attach_line_lod(
                &self.device,
                &self.line_lod_bind_group_layout,
                &mut self.wireframe_mesh,
                wireframe_mesh,
            );
            Self::attach_line_lod(
                &self.device,
                &self.line_lod_bind_group_layout,
                &mut self.reference_line_mesh,
                reference_line_mesh,
            );

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

            self.isosurface_transparent_mesh.update_from_mesh(
                &self.device,
                &isosurface_transparent_mesh.mesh,
                "Transparent Isosurfaces",
            );
            // The component ranges index the mesh just uploaded, so they must
            // be replaced together with it. Unlike the impostor sort this one
            // touches no GPU buffer — it reorders draw calls — so there is no
            // generation counter to keep in step.
            self.isosurface_components
                .clone_from(&isosurface_transparent_mesh.components);

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
            self.reference_line_mesh.set_identity_transform(&self.queue);

            self.atom_impostor_mesh.set_identity_transform(&self.queue);
            self.bond_impostor_mesh.set_identity_transform(&self.queue);
            self.transparent_impostor_mesh
                .set_identity_transform(&self.queue);
            self.isosurface_transparent_mesh
                .set_identity_transform(&self.queue);
            self.label_mesh.set_identity_transform(&self.queue);
            self.gadget_atom_impostor_mesh
                .set_identity_transform(&self.queue);
            self.gadget_bond_impostor_mesh
                .set_identity_transform(&self.queue);
        }

        // New slots need their live LOD now, not at the next camera move.
        self.write_line_lods(View::Live, &self.camera, &self.target);
    }

    pub fn update_background_mesh(&mut self, background_line_mesh: &LineMesh) {
        let _lock = self.render_mutex.lock().unwrap();

        self.background_mesh.update_from_line_mesh(
            &self.device,
            background_line_mesh,
            "Background",
        );
        Self::attach_line_lod(
            &self.device,
            &self.line_lod_bind_group_layout,
            &mut self.background_mesh,
            background_line_mesh,
        );
        self.background_mesh.set_identity_transform(&self.queue);
        self.write_line_lods(View::Live, &self.camera, &self.target);
    }

    pub fn render(&mut self, background_color_rgb: [u8; 3]) -> Vec<u8> {
        let _lock = self.render_mutex.lock().unwrap();

        let view = self.camera.build_view_matrix().as_mat4();
        Self::resort_transparent_impostors(
            &self.queue,
            &self.transparent_impostor_mesh,
            &self.transparent_quad_centers,
            self.transparent_mesh_generation,
            &mut self.transparent_sorted_generation,
            &mut self.transparent_sorted_view,
            view,
        );

        self.draw_to_target(
            &self.target,
            View::Live,
            view,
            background_color_rgb,
            PassFlags {
                editing_aids: true,
                gadgets: true,
            },
        )
    }

    /// Renders the content meshes on the GPU from `camera` in thumbnail mode
    /// (`doc/design_network_thumbnails.md` D5): content only, no gadgets,
    /// pivot cube, grid or labels, on a light or a dark background chosen
    /// for contrast with the content, at
    /// `THUMBNAIL_SUPERSAMPLING`× and box-downsampled to `THUMBNAIL_SIZE`.
    /// Returns tightly packed **RGBA**. `camera.aspect` is ignored: the
    /// thumbnail is square.
    ///
    /// Uses its own render target and camera buffer, so the live viewport's
    /// textures and camera uniform are untouched. The one shared resource is
    /// the transparent impostors' sorted index buffer: it is re-sorted for
    /// this camera, and the next live frame re-sorts it back because the sort
    /// cache is keyed on the view matrix.
    pub fn render_thumbnail(&mut self, camera: &Camera) -> Vec<u8> {
        let _lock = self.render_mutex.lock().unwrap();

        let mut camera = camera.clone();
        camera.aspect = 1.0;
        self.write_view_uniforms(View::Offscreen, &camera, &self.thumbnail_target);

        let view = camera.build_view_matrix().as_mat4();
        Self::resort_transparent_impostors(
            &self.queue,
            &self.transparent_impostor_mesh,
            &self.transparent_quad_centers,
            self.transparent_mesh_generation,
            &mut self.transparent_sorted_generation,
            &mut self.transparent_sorted_view,
            view,
        );

        // The view is drawn on both backgrounds and the one the content
        // stands out on is kept (D5). Drawing the final image on its own
        // background, rather than compositing, keeps antialiased edges right.
        let draw_on = |background: ThumbnailBackground| {
            let mut pixels = self.draw_to_target(
                &self.thumbnail_target,
                View::Offscreen,
                view,
                background.rgb(),
                PassFlags {
                    editing_aids: false,
                    gadgets: false,
                },
            );
            crate::thumbnail::bgra_to_rgba_in_place(&mut pixels);
            pixels
        };
        let on_light = draw_on(ThumbnailBackground::Light);
        let on_dark = draw_on(ThumbnailBackground::Dark);
        let pixels = match crate::thumbnail::choose_background(&on_light, &on_dark) {
            ThumbnailBackground::Light => on_light,
            ThumbnailBackground::Dark => on_dark,
        };
        crate::thumbnail::downsample_2x2(
            &pixels,
            self.thumbnail_target.size.width,
            self.thumbnail_target.size.height,
        )
    }

    /// Renders the viewport scene for *File > Export viewport image* at
    /// `width × height` and returns it as tightly packed **RGBA**.
    ///
    /// The live camera's pose and vertical field of view are kept and only the
    /// aspect follows the requested size, so a wider image shows more at the
    /// sides rather than cropping. Everything the viewport shows is drawn —
    /// grid, labels, selection rims — except the gadget pass (gizmo, guide
    /// dots, pivot cube). The frame is drawn at
    /// [`IMAGE_EXPORT_SUPERSAMPLING`](crate::image_export::IMAGE_EXPORT_SUPERSAMPLING)×
    /// into a target created for this call and box-downsampled, which is
    /// what anti-aliases it.
    ///
    /// The caller sizes the image with
    /// [`fit_export_size`](crate::image_export::fit_export_size); a side over
    /// the limit is clamped here as a last guard. The live viewport's target
    /// and camera uniform are untouched. It borrows the thumbnail camera
    /// buffer (written before every use, under the render lock) and, like a
    /// thumbnail, re-sorts the transparent impostors for its camera — the
    /// export camera has the live view matrix, so that sort is the live one.
    pub fn render_image(
        &mut self,
        width: u32,
        height: u32,
        background_color_rgb: [u8; 3],
    ) -> Vec<u8> {
        use crate::image_export::{IMAGE_EXPORT_MAX_SIZE, IMAGE_EXPORT_SUPERSAMPLING};
        let width = width.clamp(1, IMAGE_EXPORT_MAX_SIZE);
        let height = height.clamp(1, IMAGE_EXPORT_MAX_SIZE);

        let _lock = self.render_mutex.lock().unwrap();

        let mut camera = self.camera.clone();
        camera.aspect = width as f64 / height as f64;

        // The target first: the uniform's size and pixel scale come from it.
        let target = RenderTarget::new(
            &self.device,
            TargetSpec::image(width, height, IMAGE_EXPORT_SUPERSAMPLING),
        );
        self.write_view_uniforms(View::Offscreen, &camera, &target);

        let view = camera.build_view_matrix().as_mat4();
        Self::resort_transparent_impostors(
            &self.queue,
            &self.transparent_impostor_mesh,
            &self.transparent_quad_centers,
            self.transparent_mesh_generation,
            &mut self.transparent_sorted_generation,
            &mut self.transparent_sorted_view,
            view,
        );

        let mut pixels = self.draw_to_target(
            &target,
            View::Offscreen,
            view,
            background_color_rgb,
            PassFlags {
                editing_aids: true,
                gadgets: false,
            },
        );
        crate::thumbnail::bgra_to_rgba_in_place(&mut pixels);
        crate::thumbnail::downsample_2x2(&pixels, target.size.width, target.size.height)
    }

    /// Bounds of the content meshes on the GPU (D4); `None` when the content
    /// is empty, in which case no thumbnail is captured (D3).
    pub fn content_bounds(&self) -> Option<(DVec3, DVec3)> {
        self.content_bounds
    }

    /// Which network the content meshes on the GPU belong to (D2), as
    /// `(document id, network name)`.
    pub fn content_owner(&self) -> Option<&(u64, String)> {
        self.content_owner.as_ref()
    }

    /// Lazily re-sorts the transparent impostors back-to-front for `view`
    /// before a pass draws them. Recomputes and re-uploads the index buffer
    /// only when the view has changed or a new transparent mesh was uploaded
    /// since the last sort — so a resting camera costs nothing, and orbiting
    /// re-sorts once per moved frame (§Sorting of `doc/design_xray_node.md`).
    /// The sort is a fixed-size permutation of the existing index buffer, so
    /// this `write_buffer` never reallocates. Takes the fields it needs one
    /// by one (not `&mut self`) because callers hold the render lock, an
    /// immutable borrow of `self`.
    #[allow(clippy::too_many_arguments)]
    fn resort_transparent_impostors(
        queue: &Queue,
        mesh: &GPUMesh,
        quad_centers: &[Vec3],
        mesh_generation: u64,
        sorted_generation: &mut Option<u64>,
        sorted_view: &mut Option<Mat4>,
        view: Mat4,
    ) {
        if mesh.num_indices == 0 {
            return;
        }
        let up_to_date = *sorted_generation == Some(mesh_generation) && *sorted_view == Some(view);
        if !up_to_date {
            let indices = sorted_transparent_indices(quad_centers, &view);
            queue.write_buffer(&mesh.index_buffer, 0, bytemuck::cast_slice(&indices));
            *sorted_generation = Some(mesh_generation);
            *sorted_view = Some(view);
        }
    }

    /// The pipeline set whose sample count is `target`'s (D2). A target is
    /// only ever created at a count that has a set, so a miss is a bug.
    fn pipelines_for(&self, target: &RenderTarget) -> &PipelineSet {
        [
            Some(&self.offscreen_pipelines),
            self.live_pipelines.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find(|set| set.sample_count == target.sample_count)
        .unwrap_or_else(|| {
            panic!(
                "no pipelines for a {}-sample render target",
                target.sample_count
            )
        })
    }

    /// One frame into `target`, drawn with the per-view uniforms of
    /// `uniforms` (whose view matrix is `view`), read back as BGRA.
    fn draw_to_target(
        &self,
        target: &RenderTarget,
        uniforms: View,
        view: Mat4,
        background_color_rgb: [u8; 3],
        flags: PassFlags,
    ) -> Vec<u8> {
        let camera_bind_group = match uniforms {
            View::Live => &self.camera_bind_group,
            View::Offscreen => &self.thumbnail_camera_bind_group,
        };
        let pipelines = self.pipelines_for(target);
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
                // The last pass resolves the multisampled colour (D3); with
                // the gadget pass to follow, that is not this one.
                color_attachments: &[Some(
                    target.color_attachment(wgpu::LoadOp::Clear(bg_color), !flags.gadgets),
                )],
                depth_stencil_attachment: Some(target.depth_attachment()),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set camera bind group (shared for both pipelines)
            render_pass.set_bind_group(0, camera_bind_group, &[]);

            // Each mesh has its own model bind group (group 1).

            // Set identity transform for main mesh and render it
            self.main_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&pipelines.triangle);
            self.render_mesh(&mut render_pass, &self.main_mesh);

            // Render impostor meshes (if they have data)
            // Note: Impostor meshes are only populated when AtomicRenderingMethod::Impostors is selected
            self.atom_impostor_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&pipelines.atom_impostor);
            self.render_mesh(&mut render_pass, &self.atom_impostor_mesh);

            self.bond_impostor_mesh.set_identity_transform(&self.queue);
            render_pass.set_pipeline(&pipelines.bond_impostor);
            self.render_mesh(&mut render_pass, &self.bond_impostor_mesh);

            if flags.editing_aids {
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
                render_pass.set_pipeline(&pipelines.label);
                render_pass.set_bind_group(2, &self.label_atlas_bind_group, &[]);
                self.render_mesh(&mut render_pass, &self.label_mesh);
            }

            // All lines, after the opaque content and the labels and before the
            // ghosts (`doc/design_screen_space_lines.md` D6). Lines blend and
            // write no depth, so they must test against finished opaque depth:
            // anything opaque drawn after a line would paint over it even where
            // the line is in front. A label in front of a line hides it (labels
            // write depth); a ghost composites over a line and tints it, the
            // milder error of the two orders.
            self.wireframe_mesh.set_identity_transform(&self.queue);
            let mut lines = vec![&self.wireframe_mesh];
            if flags.editing_aids {
                // Drawing-plane grids: content lines, but an editing aid.
                self.reference_line_mesh.set_identity_transform(&self.queue);
                self.background_mesh.set_identity_transform(&self.queue);
                lines.push(&self.reference_line_mesh);
                lines.push(&self.background_mesh);
            }
            self.draw_lines(&mut render_pass, pipelines, &lines, uniforms);

            // Transparent impostors (x-ray) draw last in the main pass — after
            // ALL opaque content and the lines — with alpha
            // blending and depth writes off. The index buffer is kept in
            // back-to-front order for this camera by
            // `resort_transparent_impostors` (called before this draw).
            self.transparent_impostor_mesh
                .set_identity_transform(&self.queue);
            render_pass.set_pipeline(&pipelines.transparent_impostor);
            self.render_mesh(&mut render_pass, &self.transparent_impostor_mesh);

            // Transparent isosurfaces last of all. Nothing orders fragments
            // *between* two transparent pipelines — neither writes depth, so
            // they composite in draw-call order — and drawing surfaces after
            // the ghosts is the choice that matches what a surface usually is:
            // a membrane enclosing the structure, with the ghost atoms seen
            // through it. A ghost in front of the surface is composited behind
            // it anyway; that is failure mode 3 of
            // `doc/design_isosurface_node.md` §Why two-pass alone is not
            // enough, and the input to any future OIT decision.
            self.isosurface_transparent_mesh
                .set_identity_transform(&self.queue);
            self.draw_transparent_surfaces(&mut render_pass, pipelines, &view);
        }

        // Second render pass for gadgets - clear depth buffer but preserve color
        if flags.gadgets {
            let mut gadget_render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Gadget Render Pass"),
                // Preserve the main pass's colour, clear depth for the gadgets,
                // and resolve: this is the last pass (D3).
                color_attachments: &[Some(target.color_attachment(wgpu::LoadOp::Load, true))],
                depth_stencil_attachment: Some(target.depth_attachment()),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set camera bind group for gadget render pass
            gadget_render_pass.set_bind_group(0, camera_bind_group, &[]);

            // Render gadget triangles
            self.lightweight_mesh.set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&pipelines.triangle);
            self.render_mesh(&mut gadget_render_pass, &self.lightweight_mesh);

            // Render gadget atom impostors (guide dots, always on top)
            self.gadget_atom_impostor_mesh
                .set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&pipelines.atom_impostor);
            self.render_mesh(&mut gadget_render_pass, &self.gadget_atom_impostor_mesh);

            // Render gadget bond impostors (guide dot connectors, always on top)
            self.gadget_bond_impostor_mesh
                .set_identity_transform(&self.queue);
            gadget_render_pass.set_pipeline(&pipelines.bond_impostor);
            self.render_mesh(&mut gadget_render_pass, &self.gadget_bond_impostor_mesh);

            // Gadget lines last, for the same reason as in the main pass (D6):
            // they write no depth, so a gadget triangle drawn after them would
            // paint over them where they cross its arrow heads and rings.
            self.gadget_line_mesh.set_identity_transform(&self.queue);
            self.draw_lines(
                &mut gadget_render_pass,
                pipelines,
                &[&self.gadget_line_mesh],
                uniforms,
            );
        }

        // Calculate bytes per row with proper alignment (256-byte boundary for WebGPU)
        let bytes_per_row = 4 * target.size.width;
        let aligned_bytes_per_row = (bytes_per_row + 255) & !255;

        // Copy texture to output buffer with aligned bytes per row
        encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture: &target.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &target.output_buffer,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned_bytes_per_row),
                    rows_per_image: Some(target.size.height),
                },
            },
            target.size,
        );

        // Submit commands
        self.queue.submit(Some(encoder.finish()));

        // Read data
        let buffer_slice = target.output_buffer.slice(..);
        buffer_slice.map_async(MapMode::Read, |_| {});
        self.device.poll(Maintain::Wait);

        // Get the data and handle alignment if needed
        let data = if aligned_bytes_per_row != bytes_per_row {
            let aligned_data = buffer_slice.get_mapped_range();
            let mut data = Vec::with_capacity((bytes_per_row * target.size.height) as usize);

            // Extract each row, skipping the padding
            for row in 0..target.size.height {
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

        target.output_buffer.unmap();
        data
    }

    // Private helper method to render a GPU mesh
    /// Draws the merged transparent surface mesh under the current
    /// [`SurfaceTransparencyMode`].
    ///
    /// Not a `render_mesh` call: every mode but `SinglePass` needs more than
    /// one draw over the same buffers, and `ComponentSorted` needs sub-ranges
    /// of the index buffer. The sort happens **here**, per frame, rather than
    /// at tessellation time — a component order computed once looks right from
    /// the viewpoint it was computed for and wrong from every other. It is a
    /// permutation of a single-digit-length list, so recomputing it per frame
    /// is free and needs no laziness (contrast the impostor sort, which
    /// rewrites and re-uploads an index buffer).
    fn draw_transparent_surfaces<'a>(
        &'a self,
        render_pass: &mut RenderPass<'a>,
        pipelines: &'a PipelineSet,
        view: &Mat4,
    ) {
        let mesh = &self.isosurface_transparent_mesh;
        if mesh.num_indices == 0 {
            return;
        }

        render_pass.set_bind_group(1, &mesh.model_bind_group, &[]);
        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        // An empty component pool with a non-empty mesh would draw nothing at
        // all in `ComponentSorted`, so it falls back to the whole-mesh two-pass
        // draw. The extractor always emits components, so this is a guard
        // against a future producer, not a case that happens today.
        let sorted = self.surface_transparency_mode == SurfaceTransparencyMode::ComponentSorted
            && !self.isosurface_components.is_empty();

        if self.surface_transparency_mode == SurfaceTransparencyMode::SinglePass {
            render_pass.set_pipeline(&pipelines.surface_no_cull);
            render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
        } else if sorted {
            for index in sorted_component_order(&self.isosurface_components, view) {
                let component = self.isosurface_components[index];
                let range = component.first_index..component.first_index + component.index_count;
                // Back faces, then front faces, within this component.
                render_pass.set_pipeline(&pipelines.surface_front_cull);
                render_pass.draw_indexed(range.clone(), 0, 0..1);
                render_pass.set_pipeline(&pipelines.surface_back_cull);
                render_pass.draw_indexed(range, 0, 0..1);
            }
        } else {
            render_pass.set_pipeline(&pipelines.surface_front_cull);
            render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            render_pass.set_pipeline(&pipelines.surface_back_cull);
            render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
        }
    }

    /// Draws `meshes` with the line pipeline. Group 2 (the mesh's line LOD
    /// uniform for `uniforms`, or the shared "visible, no fade" one) is bound
    /// here, after the pipeline that declares it: `render_mesh` only knows
    /// about group 1, and the label pipeline binds a different group 2.
    fn draw_lines<'a>(
        &self,
        render_pass: &mut RenderPass<'a>,
        pipelines: &PipelineSet,
        meshes: &[&GPUMesh],
        uniforms: View,
    ) {
        if meshes.iter().all(|mesh| mesh.num_instances == 0) {
            return;
        }
        render_pass.set_pipeline(&pipelines.line);
        for mesh in meshes {
            if mesh.num_instances == 0 {
                continue;
            }
            let lod_bind_group = match &mesh.line_lod {
                Some(binding) => &binding.uniform(uniforms).1,
                None => &self.default_line_lod_bind_group,
            };
            render_pass.set_bind_group(2, lod_bind_group, &[]);
            self.render_mesh(render_pass, mesh);
        }
    }

    fn render_mesh<'a>(&self, render_pass: &mut RenderPass<'a>, mesh: &GPUMesh) {
        if mesh.num_instances > 0 {
            // A line mesh: one six-vertex quad per instance, no index buffer.
            render_pass.set_bind_group(1, &mesh.model_bind_group, &[]);
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.draw(0..6, 0..mesh.num_instances);
        } else if mesh.num_indices > 0 {
            // Set the mesh's model bind group (index 1)
            render_pass.set_bind_group(1, &mesh.model_bind_group, &[]);

            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
        }
    }

    /// Rewrites the live camera uniform from `self.camera` and the live
    /// target.
    pub fn update_camera_buffer(&self) {
        self.write_view_uniforms(View::Live, &self.camera, &self.target);
    }

    /// Writes the per-view uniforms of `uniforms` for drawing `camera` into
    /// `target` (`doc/design_screen_space_lines.md` D3, D9): the camera
    /// uniform and every line mesh's `LineLodUniform`. The only writer of a
    /// camera uniform, so the viewport size and pixel scale a shader sees can
    /// only come from the target it is drawn into.
    fn write_view_uniforms(&self, uniforms: View, camera: &Camera, target: &RenderTarget) {
        let camera_buffer = match uniforms {
            View::Live => &self.camera_buffer,
            View::Offscreen => &self.thumbnail_camera_buffer,
        };
        let camera_uniform = CameraUniform::new(
            camera,
            (target.size.width, target.size.height),
            target.pixel_scale,
        );
        self.queue
            .write_buffer(camera_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));
        self.write_line_lods(uniforms, camera, target);
    }

    /// Writes the `LineLodUniform` of `uniforms` for every line mesh with LOD
    /// slots (D9). Only the live view uses and updates the hysteresis state;
    /// a thumbnail or an export takes the plain thresholds.
    fn write_line_lods(&self, uniforms: View, camera: &Camera, target: &RenderTarget) {
        let meshes = [
            &self.wireframe_mesh,
            &self.reference_line_mesh,
            &self.background_mesh,
            &self.gadget_line_mesh,
        ];
        for binding in meshes.iter().filter_map(|mesh| mesh.line_lod.as_ref()) {
            let mut was_visible = binding.was_visible.lock().unwrap();
            let entries = lod_uniform_entries(
                &binding.slots,
                camera,
                (target.size.width, target.size.height),
                target.pixel_scale,
                (uniforms == View::Live).then_some(&mut *was_visible),
            );
            self.queue.write_buffer(
                &binding.uniform(uniforms).0,
                0,
                bytemuck::cast_slice(&entries),
            );
        }
    }

    /// Sets how the merged transparent surface mesh is drawn.
    ///
    /// Pushed in from the api layer on every refresh: this crate sits below
    /// the preferences types and cannot read them, and a preference change
    /// triggers a refresh anyway.
    pub fn set_surface_transparency_mode(&mut self, mode: SurfaceTransparencyMode) {
        self.surface_transparency_mode = mode;
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
    pub fn set_camera_canonical_view(&mut self, view: crate::camera::CameraCanonicalView) {
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
    pub fn get_camera_transform(&self) -> atomcad_util::transform::Transform {
        use atomcad_util::transform::Transform;

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
    pub fn set_camera_transform(&mut self, transform: &atomcad_util::transform::Transform) {
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
        // A rotation quaternion is orthonormal in exact arithmetic; this keeps
        // the invariant exactly rather than to the quaternion's rounding.
        self.camera.orthonormalize_up();

        // Update the GPU buffers
        self.update_camera_buffer();
    }
}
