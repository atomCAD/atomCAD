//! The render pipelines (`doc/design_viewport_antialiasing.md`, Where the code
//! goes).

use crate::label_mesh::LabelVertex;
use crate::line_mesh::LineSegmentInstance;
use crate::mesh::Vertex;
use crate::render_target::{COLOR_FORMAT, DEPTH_FORMAT};
use crate::renderer::STRAIGHT_ALPHA_BLENDING;
use crate::transparent_impostor_mesh::TransparentImpostorVertex;
use std::collections::HashMap;
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

/// The marker before the refined frame's entry point at the end of each
/// impostor shader (`doc/design_viewport_antialiasing.md` D5).
pub const PER_SAMPLE_SHADING_MARKER: &str = "// ---- Per-sample shading (D5) ----";

/// `source` without its per-sample entry point: everything from
/// [`PER_SAMPLE_SHADING_MARKER`] on. A module that uses
/// `@builtin(sample_index)` fails to compile on an adapter without
/// `DownlevelFlags::MULTISAMPLED_SHADING`, even when no pipeline uses that
/// entry point, so on such an adapter the impostor shaders are built without
/// it. A source without the marker is returned whole.
pub fn without_per_sample_shading(source: &str) -> &str {
    source
        .find(PER_SAMPLE_SHADING_MARKER)
        .map_or(source, |at| &source[..at])
}

/// The marker in each lit shader where [`LIGHTING_MODULE`] is inserted
/// (`doc/design_depth_perception.md` D1).
pub const LIGHTING_MARKER: &str = "// ---- lighting ----";

/// The lighting shared by the lit shaders (`lighting.wgsl`): the
/// `ShadingUniform`, the PBR helpers, `light_surface` and `to_display`. It
/// reads the host's `camera` without declaring it, so it compiles only as
/// part of a host ([`compose_lit_shader`]).
pub const LIGHTING_MODULE: &str = include_str!("lighting.wgsl");

/// The lit shaders, by file name: the ones composed with
/// [`LIGHTING_MODULE`].
pub const LIT_SHADERS: [&str; 4] = [
    "atom_impostor.wgsl",
    "bond_impostor.wgsl",
    "mesh.wgsl",
    "transparent_impostor.wgsl",
];

/// `host` with [`LIGHTING_MODULE`] inserted at its [`LIGHTING_MARKER`]
/// (`doc/design_depth_perception.md` D1): the host's header (its uniform
/// declarations, which the module reads), then the module, then the rest of
/// the host, marker included. WGSL has no `#include`; this is the one
/// composition step, and what `Shaders::new` compiles. Run it before
/// [`without_per_sample_shading`], whose marker is in the host's tail.
///
/// # Panics
///
/// When `host` does not contain the marker exactly once: a lit shader that
/// lost it would otherwise fail to compile only on the GPU.
pub fn compose_lit_shader(host: &str) -> String {
    let markers = host.matches(LIGHTING_MARKER).count();
    assert_eq!(
        markers, 1,
        "a lit shader must contain `{LIGHTING_MARKER}` exactly once, found {markers}"
    );
    let at = host.find(LIGHTING_MARKER).expect("counted above");
    let (header, rest) = host.split_at(at);
    let mut composed = String::with_capacity(host.len() + LIGHTING_MODULE.len() + 1);
    composed.push_str(header);
    composed.push_str(LIGHTING_MODULE);
    composed.push('\n');
    composed.push_str(rest);
    composed
}

impl Shaders {
    /// `per_sample_shading`: whether the adapter has
    /// `DownlevelFlags::MULTISAMPLED_SHADING`. Without it the impostor shaders
    /// lose their `fs_main_per_sample` entry point (D5).
    pub fn new(device: &Device, per_sample_shading: bool) -> Self {
        let module = |label: &str, source: &str| {
            device.create_shader_module(ShaderModuleDescriptor {
                label: Some(label),
                source: ShaderSource::Wgsl(source.into()),
            })
        };
        let impostor = |label: &str, host: &str| {
            let source = compose_lit_shader(host);
            module(
                label,
                if per_sample_shading {
                    &source
                } else {
                    without_per_sample_shading(&source)
                },
            )
        };
        Self {
            triangle: module(
                "Triangle Shader",
                &compose_lit_shader(include_str!("mesh.wgsl")),
            ),
            line: module("Line Shader", include_str!("line_mesh.wgsl")),
            atom_impostor: impostor("Atom Impostor Shader", include_str!("atom_impostor.wgsl")),
            bond_impostor: impostor("Bond Impostor Shader", include_str!("bond_impostor.wgsl")),
            transparent_impostor: impostor(
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
    /// D5: per-sample-shading variants of the impostor pipelines, for the
    /// refined frame. `Some` only in a multisampled set, and only on an
    /// adapter with `DownlevelFlags::MULTISAMPLED_SHADING`.
    pub refined: Option<RefinedImpostorPipelines>,
}

/// The impostor pipelines with the shaders' `fs_main_per_sample` entry point
/// (`doc/design_viewport_antialiasing.md` D5): everything else as in the
/// set's own impostor pipelines.
pub(crate) struct RefinedImpostorPipelines {
    pub atom_impostor: RenderPipeline,
    pub bond_impostor: RenderPipeline,
    pub transparent_impostor: RenderPipeline,
}

impl PipelineSet {
    /// `per_sample_shading`: whether `shaders` carry the per-sample entry
    /// points (D5); the refined pipelines are built only then, and only for a
    /// multisampled set.
    pub fn new(
        device: &Device,
        layouts: &PipelineLayouts,
        shaders: &Shaders,
        sample_count: u32,
        per_sample_shading: bool,
    ) -> Self {
        let impostors = |fragment_entry: &str| {
            (
                create_atom_impostor_pipeline(
                    device,
                    &layouts.main,
                    &shaders.atom_impostor,
                    sample_count,
                    fragment_entry,
                ),
                create_bond_impostor_pipeline(
                    device,
                    &layouts.main,
                    &shaders.bond_impostor,
                    sample_count,
                    fragment_entry,
                ),
                create_transparent_impostor_pipeline(
                    device,
                    &layouts.main,
                    &shaders.transparent_impostor,
                    sample_count,
                    fragment_entry,
                ),
            )
        };
        let (atom_impostor, bond_impostor, transparent_impostor) = impostors("fs_main");
        let refined = (sample_count > 1 && per_sample_shading).then(|| {
            let (atom_impostor, bond_impostor, transparent_impostor) =
                impostors("fs_main_per_sample");
            RefinedImpostorPipelines {
                atom_impostor,
                bond_impostor,
                transparent_impostor,
            }
        });
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
            atom_impostor,
            bond_impostor,
            transparent_impostor,
            surface_no_cull: surface(None),
            surface_front_cull: surface(Some(wgpu::Face::Front)),
            surface_back_cull: surface(Some(wgpu::Face::Back)),
            label: create_label_pipeline(device, &layouts.label, &shaders.label, sample_count),
            refined,
        }
    }
}

/// The impostor shaders' `override SAMPLE_COUNT`
/// (`doc/design_viewport_antialiasing.md` D4): they decide coverage per
/// sample at the target's sample positions, so they must know the count.
fn sample_count_constants(sample_count: u32) -> HashMap<String, f64> {
    HashMap::from([("SAMPLE_COUNT".to_string(), f64::from(sample_count))])
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
    fragment_entry: &str,
) -> RenderPipeline {
    let constants = sample_count_constants(sample_count);
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
            entry_point: Some(fragment_entry),
            targets: &[Some(ColorTargetState {
                format: COLOR_FORMAT,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
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
    fragment_entry: &str,
) -> RenderPipeline {
    let constants = sample_count_constants(sample_count);
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
            entry_point: Some(fragment_entry),
            targets: &[Some(ColorTargetState {
                format: COLOR_FORMAT,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
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
    fragment_entry: &str,
) -> RenderPipeline {
    let constants = sample_count_constants(sample_count);
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
            entry_point: Some(fragment_entry),
            targets: &[Some(ColorTargetState {
                format: COLOR_FORMAT,
                blend: Some(STRAIGHT_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
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
