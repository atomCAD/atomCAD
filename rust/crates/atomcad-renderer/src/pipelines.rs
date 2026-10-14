//! The render pipelines (`doc/design_viewport_antialiasing.md`, Where the code
//! goes).

use crate::label_mesh::LabelVertex;
use crate::line_mesh::LineSegmentInstance;
use crate::mesh::Vertex;
use crate::render_target::{COLOR_FORMAT, DEPTH_FORMAT};
use crate::renderer::STRAIGHT_ALPHA_BLENDING;
use crate::transparent_impostor_mesh::TransparentImpostorVertex;
use wgpu::*;

/// The bind group layouts every pipeline is built on, by shape: the shared
/// `[camera, model]` layout, the line pipeline's `[camera, model, line LOD]`
/// and the label pipeline's `[camera, model, atlas]`.
pub(crate) struct PipelineLayouts {
    pub main: PipelineLayout,
    pub line: PipelineLayout,
    pub label: PipelineLayout,
}

/// The shader modules, compiled once and shared by every [`PipelineSet`].
pub(crate) struct Shaders {
    pub triangle: ShaderModule,
    pub line: ShaderModule,
    pub atom_impostor: ShaderModule,
    pub bond_impostor: ShaderModule,
    pub transparent_impostor: ShaderModule,
    pub label: ShaderModule,
}

impl Shaders {
    pub fn new(device: &Device) -> Self {
        let module = |label: &str, source: &'static str| {
            device.create_shader_module(ShaderModuleDescriptor {
                label: Some(label),
                source: ShaderSource::Wgsl(source.into()),
            })
        };
        Self {
            triangle: module("Triangle Shader", include_str!("mesh.wgsl")),
            line: module("Line Shader", include_str!("line_mesh.wgsl")),
            atom_impostor: module("Atom Impostor Shader", include_str!("atom_impostor.wgsl")),
            bond_impostor: module("Bond Impostor Shader", include_str!("bond_impostor.wgsl")),
            transparent_impostor: module(
                "Transparent Impostor Shader",
                include_str!("transparent_impostor.wgsl"),
            ),
            label: module("Label Shader", include_str!("label.wgsl")),
        }
    }
}

/// Every render pipeline, compiled for one sample count
/// (`doc/design_viewport_antialiasing.md` D2).
///
/// A wgpu pipeline can only draw into attachments of the sample count it was
/// compiled for, and the live target is multisampled while the offscreen ones
/// are not, so the renderer holds one set per count. **A new pipeline goes
/// here**, which is the one place that makes it exist at every count.
pub(crate) struct PipelineSet {
    pub sample_count: u32,
    pub triangle: RenderPipeline,
    /// Every line mesh: instanced screen-space segments, blended, depth
    /// tested but not depth writing (`doc/design_screen_space_lines.md` D6).
    /// Its own layout, `[camera, model, line LOD]`.
    pub line: RenderPipeline,
    pub atom_impostor: RenderPipeline,
    pub bond_impostor: RenderPipeline,
    pub transparent_impostor: RenderPipeline,
    /// The three transparent-surface pipelines, all reading `mesh.wgsl` with
    /// `Vertex::desc()` and differing only in `cull_mode`. Alpha blending on,
    /// depth test on, depth **write off** — or the first transparent fragment
    /// kills the surface's own second layer. See
    /// `doc/design_isosurface_node.md` §Pipelines and blend mode.
    pub surface_no_cull: RenderPipeline,
    pub surface_front_cull: RenderPipeline,
    pub surface_back_cull: RenderPipeline,
    /// Atom labels. The only pipeline with a texture, so the only one built on
    /// its own three-group layout (camera, model, atlas) — see
    /// `doc/design_atom_labels.md` §Pipeline changes.
    pub label: RenderPipeline,
}

impl PipelineSet {
    pub fn new(
        device: &Device,
        layouts: &PipelineLayouts,
        shaders: &Shaders,
        sample_count: u32,
    ) -> Self {
        // One transparent-surface pipeline per cull mode. Three, where the
        // isosurface design names two: it counts the two-pass pair, and the
        // `SinglePass` control mode it describes separately needs a
        // `cull_mode: None` pipeline of its own.
        let surface = |cull_mode| {
            create_surface_transparent_pipeline(
                device,
                &layouts.main,
                &shaders.triangle,
                cull_mode,
                sample_count,
            )
        };
        Self {
            sample_count,
            triangle: create_triangle_pipeline(
                device,
                &layouts.main,
                &shaders.triangle,
                false, // Normal depth testing
                sample_count,
            ),
            line: create_line_pipeline(device, &layouts.line, &shaders.line, sample_count),
            atom_impostor: create_atom_impostor_pipeline(
                device,
                &layouts.main,
                &shaders.atom_impostor,
                sample_count,
            ),
            bond_impostor: create_bond_impostor_pipeline(
                device,
                &layouts.main,
                &shaders.bond_impostor,
                sample_count,
            ),
            transparent_impostor: create_transparent_impostor_pipeline(
                device,
                &layouts.main,
                &shaders.transparent_impostor,
                sample_count,
            ),
            surface_no_cull: surface(None),
            surface_front_cull: surface(Some(wgpu::Face::Front)),
            surface_back_cull: surface(Some(wgpu::Face::Back)),
            label: create_label_pipeline(device, &layouts.label, &shaders.label, sample_count),
        }
    }
}

/// The line pipeline (`doc/design_screen_space_lines.md` D4, D6): six
/// vertices per instance, expanded by `line_mesh.wgsl`; straight-alpha
/// blending; depth test `Less` with **no depth write**, or the
/// anti-aliasing fringe of one grid line would reject the crossing line at
/// every intersection; no depth bias (the triangle pipeline's negative
/// bias already makes a face win a coplanar tie against an edge in it).
fn create_line_pipeline(
    device: &Device,
    line_pipeline_layout: &wgpu::PipelineLayout,
    line_shader: &wgpu::ShaderModule,
    sample_count: u32,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Line Render Pipeline"),
        layout: Some(line_pipeline_layout),
        vertex: VertexState {
            module: line_shader,
            entry_point: Some("vs_main"),
            buffers: &[LineSegmentInstance::desc()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(FragmentState {
            module: line_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: COLOR_FORMAT,
                blend: Some(STRAIGHT_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            // The corners' winding follows the screen direction.
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
            count: sample_count,
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
    sample_count: u32,
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
                format: COLOR_FORMAT,
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
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

/// A transparent-surface pipeline, differing from the opaque
/// `triangle_pipeline` in exactly three ways: alpha blending on, depth
/// writes off, and a caller-chosen `cull_mode`.
///
/// [`STRAIGHT_ALPHA_BLENDING`] is non-premultiplied source-over, the blend
/// every blended pipeline uses. The depth **test** stays on so
/// opaque geometry (drawn first, with depth writes) occludes correctly; the
/// depth **write** must stay off or the first transparent fragment would
/// kill the surface's own second layer, which is precisely the layer the
/// two-pass draw exists to show. No depth bias either: the opaque pipeline
/// biases to win coplanar ties against wireframes, and a surface that does
/// not write depth has no tie to win.
fn create_surface_transparent_pipeline(
    device: &Device,
    pipeline_layout: &wgpu::PipelineLayout,
    triangle_shader: &wgpu::ShaderModule,
    cull_mode: Option<wgpu::Face>,
    sample_count: u32,
) -> RenderPipeline {
    let label = match cull_mode {
        None => "Transparent Surface Pipeline (no cull)",
        Some(wgpu::Face::Front) => "Transparent Surface Pipeline (back faces)",
        Some(wgpu::Face::Back) => "Transparent Surface Pipeline (front faces)",
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
                format: COLOR_FORMAT,
                blend: Some(STRAIGHT_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode,
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
            count: sample_count,
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
    sample_count: u32,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Atom Impostor Render Pipeline"),
        layout: Some(pipeline_layout),
        vertex: VertexState {
            module: atom_impostor_shader,
            entry_point: Some("vs_main"),
            buffers: &[crate::atom_impostor_mesh::AtomImpostorVertex::desc()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(FragmentState {
            module: atom_impostor_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: COLOR_FORMAT,
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
            count: sample_count,
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
    sample_count: u32,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Bond Impostor Render Pipeline"),
        layout: Some(pipeline_layout),
        vertex: VertexState {
            module: bond_impostor_shader,
            entry_point: Some("vs_main"),
            buffers: &[crate::bond_impostor_mesh::BondImpostorVertex::desc()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(FragmentState {
            module: bond_impostor_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: COLOR_FORMAT,
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
            count: sample_count,
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
    sample_count: u32,
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
                format: COLOR_FORMAT,
                blend: Some(STRAIGHT_ALPHA_BLENDING),
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
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

/// Blended, depth-**writing** label pipeline.
///
/// An alpha-tested label would throw away the
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
    sample_count: u32,
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
                format: COLOR_FORMAT,
                blend: Some(STRAIGHT_ALPHA_BLENDING),
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
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}
