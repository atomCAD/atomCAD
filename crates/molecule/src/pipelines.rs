// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::{
    core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT,
    mesh::VertexBufferLayout,
    prelude::*,
    render::{
        render_resource::{
            BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType,
            ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
            FragmentState, FrontFace, MultisampleState, PolygonMode, PrimitiveState,
            RenderPipelineDescriptor, ShaderStages, ShaderType, SpecializedRenderPipeline,
            TextureFormat, VertexAttribute, VertexState, VertexStepMode,
        },
        view::ViewUniform,
    },
};
use periodic_table::PeriodicTable;
use wgpu_types::{PrimitiveTopology, VertexFormat};

use crate::{
    assets::MoleculeShaders,
    buffers::VdwScaleUniform,
    components::{AtomInstance, DenormalizedBondInstance},
};

/// A trait that defines the common interface for molecule render pipelines.
///
/// This trait allows us to share code between atom and bond rendering pipelines
/// while maintaining type safety and specialization capabilities.
pub(crate) trait MoleculeRenderPipeline {
    fn new(shader: Handle<Shader>, bind_group_layout: BindGroupLayoutDescriptor) -> Self;
    fn shader(&self) -> &Handle<Shader>;
    fn bind_group_layout(&self) -> &BindGroupLayoutDescriptor;
}

/// A key used to specialize the molecule render pipeline based on MSAA settings.
///
/// This allows the render pipeline to be optimized for different anti-aliasing
/// configurations while maintaining the same basic rendering approach.
#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct MoleculeRenderPipelineKey {
    pub msaa_samples: u32,
}

impl MoleculeRenderPipelineKey {
    pub fn from_msaa_samples(msaa_samples: u32) -> Self {
        Self { msaa_samples }
    }
}

/// Specifies the type of molecule render pipeline being created.
///
/// This enum is used to differentiate between atom and bond rendering pipelines
/// when creating specialized render pipelines. It allows the pipeline creation
/// code to handle the different vertex layouts and shader configurations needed
/// for each type of molecular structure.
///
/// # Variants
/// * `Atom` - Pipeline for rendering atom spheres using billboard quads
/// * `Bond` - Pipeline for rendering bond capsules between atoms
enum RenderPipelineType {
    Atom,
    Bond,
}

/// Pipeline for rendering atom spheres using instanced billboards.
///
/// This pipeline renders atoms as spheres using billboard quads with a custom
/// fragment shader to create the spherical appearance. It uses instanced rendering
/// for efficient drawing of many atoms.
#[derive(Resource)]
pub(crate) struct AtomRenderPipeline {
    shader: Handle<Shader>,
    bind_group_layout: BindGroupLayoutDescriptor,
}

impl MoleculeRenderPipeline for AtomRenderPipeline {
    fn new(shader: Handle<Shader>, bind_group_layout: BindGroupLayoutDescriptor) -> Self {
        Self {
            shader,
            bind_group_layout,
        }
    }

    fn shader(&self) -> &Handle<Shader> {
        &self.shader
    }

    fn bind_group_layout(&self) -> &BindGroupLayoutDescriptor {
        &self.bind_group_layout
    }
}

impl FromWorld for AtomRenderPipeline {
    fn from_world(world: &mut World) -> Self {
        render_pipeline_from_world::<Self>(RenderPipelineType::Atom, world)
    }
}

impl SpecializedRenderPipeline for AtomRenderPipeline {
    type Key = MoleculeRenderPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        specialize_render_pipeline(RenderPipelineType::Atom, self, key)
    }
}

/// Pipeline for rendering bond capsules using instanced billboards.
///
/// This pipeline renders bonds as capsules between atoms using billboard quads
/// with a custom fragment shader. It uses instanced rendering and interpolates
/// between the connected atoms' properties.
#[derive(Resource)]
pub(crate) struct BondRenderPipeline {
    shader: Handle<Shader>,
    bind_group_layout: BindGroupLayoutDescriptor,
}

impl MoleculeRenderPipeline for BondRenderPipeline {
    fn new(shader: Handle<Shader>, bind_group_layout: BindGroupLayoutDescriptor) -> Self {
        Self {
            shader,
            bind_group_layout,
        }
    }

    fn shader(&self) -> &Handle<Shader> {
        &self.shader
    }

    fn bind_group_layout(&self) -> &BindGroupLayoutDescriptor {
        &self.bind_group_layout
    }
}

impl FromWorld for BondRenderPipeline {
    fn from_world(world: &mut World) -> Self {
        render_pipeline_from_world::<Self>(RenderPipelineType::Bond, world)
    }
}

impl SpecializedRenderPipeline for BondRenderPipeline {
    type Key = MoleculeRenderPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        specialize_render_pipeline(RenderPipelineType::Bond, self, key)
    }
}

/// Creates a render pipeline for either atoms or bonds based on the given type.
///
/// This function sets up the necessary bind group layout and shader configuration
/// for rendering molecular structures. It handles both atom spheres and bond capsules
/// using the same basic pipeline structure but with different vertex layouts.
fn render_pipeline_from_world<T: MoleculeRenderPipeline>(
    pipeline_type: RenderPipelineType,
    world: &mut World,
) -> T {
    let name = match pipeline_type {
        RenderPipelineType::Atom => "atom",
        RenderPipelineType::Bond => "bond",
    };

    let molecule_shaders = world.resource::<MoleculeShaders>();
    let shader = match pipeline_type {
        RenderPipelineType::Atom => molecule_shaders.atoms_shader.clone(),
        RenderPipelineType::Bond => molecule_shaders.bonds_shader.clone(),
    };

    let bind_group_layout = BindGroupLayoutDescriptor::new(
        format!("{}_pipeline_bind_group_layout", name),
        &[
            // Binding 0: View uniform buffer
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(ViewUniform::min_size()),
                },
                count: None,
            },
            // Binding 1: Entity global transform
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZero::new(std::mem::size_of::<Mat4>() as u64).unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 2: Periodic table
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZero::new(std::mem::size_of::<PeriodicTable>() as u64)
                            .unwrap(),
                    ),
                },
                count: None,
            },
            // Binding 3: VDW scale
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZero::new(std::mem::size_of::<VdwScaleUniform>() as u64)
                            .unwrap(),
                    ),
                },
                count: None,
            },
        ],
    );

    T::new(shader, bind_group_layout)
}

/// Specializes a render pipeline for specific rendering requirements.
///
/// This function configures the vertex buffer layouts and pipeline state based on
/// whether we're rendering atoms or bonds, and applies any MSAA settings from the
/// pipeline key.
fn specialize_render_pipeline<T: MoleculeRenderPipeline>(
    pipeline_type: RenderPipelineType,
    pipeline: &T,
    key: MoleculeRenderPipelineKey,
) -> RenderPipelineDescriptor {
    let name = match pipeline_type {
        RenderPipelineType::Atom => "atom",
        RenderPipelineType::Bond => "bond",
    };

    let mut vertex_buffer_layout = vec![
        // Buffer 0: Shared quad vertices
        VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 3]>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: vec![VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        },
    ];
    match pipeline_type {
        RenderPipelineType::Atom => {
            vertex_buffer_layout.push(
                // Buffer 1: Per-atom instance data
                VertexBufferLayout {
                    array_stride: std::mem::size_of::<AtomInstance>() as u64,
                    step_mode: VertexStepMode::Instance,
                    attributes: vec![
                        // Position
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 1,
                        },
                        // Element ID
                        VertexAttribute {
                            format: VertexFormat::Uint32,
                            offset: 12,
                            shader_location: 2,
                        },
                    ],
                },
            );
        }
        RenderPipelineType::Bond => {
            vertex_buffer_layout.push(
                // Buffer 1: Per-bond instance data
                VertexBufferLayout {
                    array_stride: std::mem::size_of::<DenormalizedBondInstance>() as u64,
                    step_mode: VertexStepMode::Instance,
                    attributes: vec![
                        // First atom position
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 1,
                        },
                        // First atom element ID
                        VertexAttribute {
                            format: VertexFormat::Uint32,
                            offset: 12,
                            shader_location: 2,
                        },
                        // Second atom position
                        VertexAttribute {
                            format: VertexFormat::Float32x3,
                            offset: 16,
                            shader_location: 3,
                        },
                        // Second atom element ID
                        VertexAttribute {
                            format: VertexFormat::Uint32,
                            offset: 28,
                            shader_location: 4,
                        },
                    ],
                },
            );
        }
    };

    RenderPipelineDescriptor {
        label: Some(format!("{}_render_pipeline", name).into()),
        layout: vec![pipeline.bind_group_layout().clone()],
        push_constant_ranges: vec![],
        vertex: VertexState {
            shader: pipeline.shader().clone(),
            entry_point: Some("vertex".into()),
            buffers: vertex_buffer_layout,
            shader_defs: vec![],
        },
        fragment: Some(FragmentState {
            shader: pipeline.shader().clone(),
            entry_point: Some("fragment".into()),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::bevy_default(),
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            shader_defs: vec![],
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: CORE_3D_DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: default(),
            bias: match pipeline_type {
                RenderPipelineType::Atom => DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
                RenderPipelineType::Bond => DepthBiasState {
                    constant: 1,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            },
        }),
        multisample: MultisampleState {
            count: key.msaa_samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        zero_initialize_workgroup_memory: false,
    }
}

// End of File
