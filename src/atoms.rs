// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::{
    core_pipeline::core_3d::{CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey},
    ecs::{
        component::Tick,
        query::QueryItem,
        system::{Query, SystemParamItem, lifetimeless::SRes},
    },
    prelude::*,
    reflect::Reflect,
    render::{
        Render, RenderApp, RenderSet,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        mesh::PrimitiveTopology,
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem,
            RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
            ViewBinnedRenderPhases,
        },
        render_resource::{
            BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingType, Buffer,
            BufferBindingType, BufferInitDescriptor, BufferUsages, ColorTargetState, ColorWrites,
            CompareFunction, DepthStencilState, FragmentState, FrontFace, MultisampleState,
            PipelineCache, PolygonMode, PrimitiveState, RenderPipelineDescriptor, ShaderStages,
            ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines, TextureFormat,
            VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
        },
        renderer::RenderDevice,
        view::{
            ExtractedView, RenderVisibleEntities, ViewUniform, ViewUniformOffset, ViewUniforms,
            VisibilityClass, add_visibility_class,
        },
    },
};
use bytemuck::{Pod, Zeroable};
use periodic_table::PeriodicTable;

// Plugin
pub struct AtomClusterPlugin;

impl Plugin for AtomClusterPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AtomInstance>()
            .register_type::<AtomCluster>()
            .add_plugins(ExtractComponentPlugin::<AtomCluster>::default());
        app.sub_app_mut(RenderApp)
            .add_render_command::<Opaque3d, DrawAtomCluster>()
            .add_systems(
                Render,
                (
                    prepare_atom_cluster_buffers.in_set(RenderSet::PrepareResources),
                    prepare_atom_cluster_view_bind_groups.in_set(RenderSet::PrepareBindGroups),
                    queue_atom_clusters.in_set(RenderSet::Queue),
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        let render_device = render_app.world().resource::<RenderDevice>();

        // Create quad vertices (will be billboarded in vertex shader)
        let vertices = vec![
            // Bottom-left
            QuadVertex {
                position: [-1.0, -1.0, 0.0],
            },
            // Bottom-right
            QuadVertex {
                position: [1.0, -1.0, 0.0],
            },
            // Top-left
            QuadVertex {
                position: [-1.0, 1.0, 0.0],
            },
            // Top-right
            QuadVertex {
                position: [1.0, 1.0, 0.0],
            },
        ];
        let vertex_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atom_cluster_vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });

        // Create the periodic table buffer
        let periodic_table = PeriodicTable::new();
        let periodic_table_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("periodic_table_buffer"),
            contents: bytemuck::cast_slice(&[periodic_table]),
            usage: BufferUsages::UNIFORM,
        });

        let shared_atom_cluster_buffers = SharedAtomClusterGpuBuffers {
            vertex_buffer,
            periodic_table_buffer,
        };

        render_app
            .insert_resource(shared_atom_cluster_buffers)
            // Requires AssetServer, so can't be done in build()
            .init_resource::<AtomClusterPipeline>()
            .init_resource::<SpecializedRenderPipelines<AtomClusterPipeline>>();
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Reflect)]
pub struct AtomInstance {
    pub position: Vec3,
    pub kind: u32,
}

// Component that holds our atom data in the ECS
#[derive(Component, Clone, Reflect)]
#[require(VisibilityClass)]
#[component(on_add = add_visibility_class::<AtomCluster>)]
pub struct AtomCluster {
    pub atoms: Vec<AtomInstance>,
}

// Extracted component for the render world
#[derive(Component, Clone)]
pub struct ExtractedAtomCluster {
    transform: Mat4,
    atoms: Vec<AtomInstance>,
}

impl ExtractComponent for AtomCluster {
    type QueryData = (&'static AtomCluster, &'static GlobalTransform);
    type QueryFilter = ();
    type Out = ExtractedAtomCluster;

    fn extract_component(item: QueryItem<'_, Self::QueryData>) -> Option<Self::Out> {
        let (atom_cluster, transform) = item;
        Some(ExtractedAtomCluster {
            transform: transform.compute_matrix(),
            atoms: atom_cluster.atoms.clone(),
        })
    }
}

// Vertex data for a quad
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadVertex {
    position: [f32; 3],
}

// Shared across all atom clusters
#[derive(Resource)]
struct SharedAtomClusterGpuBuffers {
    vertex_buffer: Buffer,
    periodic_table_buffer: Buffer,
}

// Create a world transform uniform buffer for each atom cluster
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct AtomClusterTransform {
    transform: Mat4,
}

// GPU buffers for atom cluster
#[derive(Component)]
struct AtomClusterGpuBuffers {
    transform_buffer: Buffer,
    instance_buffer: Buffer,
    instance_count: u32,
}

// Pipeline
#[derive(Resource)]
struct AtomClusterPipeline {
    shader: Handle<Shader>,
    bind_group_layout: BindGroupLayout,
}

// Store the view bind group as a component
#[derive(Component)]
struct AtomClusterViewBindGroup {
    bind_group: BindGroup,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct AtomClusterPipelineKey {
    pub msaa_samples: u32,
}

impl AtomClusterPipelineKey {
    pub fn from_msaa_samples(msaa_samples: u32) -> Self {
        Self { msaa_samples }
    }
}

impl FromWorld for AtomClusterPipeline {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        let shader = asset_server.load("shaders/atom_cluster.wgsl");

        let render_device = world.resource::<RenderDevice>();
        let bind_group_layout = render_device.create_bind_group_layout(
            "atom_cluster_bind_group_layout",
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
                            std::num::NonZero::new(
                                std::mem::size_of::<AtomClusterTransform>() as u64
                            )
                            .unwrap(),
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
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        );

        Self {
            shader,
            bind_group_layout,
        }
    }
}

impl SpecializedRenderPipeline for AtomClusterPipeline {
    type Key = AtomClusterPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("atom_cluster_pipeline".into()),
            layout: vec![self.bind_group_layout.clone()],
            push_constant_ranges: vec![],
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: "vertex".into(),
                buffers: vec![
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
                ],
                shader_defs: vec![],
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                entry_point: "fragment".into(),
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
                bias: default(),
            }),
            multisample: MultisampleState {
                count: key.msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            zero_initialize_workgroup_memory: false,
        }
    }
}

// Prepare GPU buffers
fn prepare_atom_cluster_buffers(
    mut commands: Commands,
    query: Query<(Entity, &ExtractedAtomCluster), Without<AtomClusterGpuBuffers>>,
    render_device: Res<RenderDevice>,
) {
    for (entity, atom_cluster) in query.iter() {
        // Create transform buffer
        let transform = AtomClusterTransform {
            transform: atom_cluster.transform,
        };

        let transform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atom_cluster_transform_buffer"),
            contents: bytemuck::cast_slice(&[transform]),
            usage: BufferUsages::UNIFORM,
        });

        // Create instance buffer
        let instance_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atom_cluster_instance_buffer"),
            contents: bytemuck::cast_slice(&atom_cluster.atoms),
            usage: BufferUsages::VERTEX,
        });

        commands.entity(entity).insert(AtomClusterGpuBuffers {
            transform_buffer,
            instance_buffer,
            instance_count: atom_cluster.atoms.len() as u32,
        });
    }
}

// Create bind groups in a separate system
fn prepare_atom_cluster_view_bind_groups(
    mut commands: Commands,
    view_uniforms: Res<ViewUniforms>,
    shared_atom_cluster_buffers: Res<SharedAtomClusterGpuBuffers>,
    pipeline: Res<AtomClusterPipeline>,
    render_device: Res<RenderDevice>,
    gpu_buffers: Query<(Entity, &AtomClusterGpuBuffers)>,
) {
    for (entity, gpu_buffers) in gpu_buffers.iter() {
        let bind_group = render_device.create_bind_group(
            Some("atom_cluster_view_bind_group"),
            &pipeline.bind_group_layout,
            &[
                // Binding 0: View uniforms
                BindGroupEntry {
                    binding: 0,
                    resource: view_uniforms.uniforms.binding().unwrap(),
                },
                // Binding 1: Entity global transform
                BindGroupEntry {
                    binding: 1,
                    resource: gpu_buffers.transform_buffer.as_entire_binding(),
                },
                // Binding 2: Periodic table
                BindGroupEntry {
                    binding: 2,
                    resource: shared_atom_cluster_buffers
                        .periodic_table_buffer
                        .as_entire_binding(),
                },
            ],
        );

        commands
            .entity(entity)
            .insert(AtomClusterViewBindGroup { bind_group });
    }
}

// Queue atom clusters for rendering
fn queue_atom_clusters(
    draw_functions: Res<DrawFunctions<Opaque3d>>,
    atom_cluster_pipeline: Res<AtomClusterPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<AtomClusterPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    views: Query<(&ExtractedView, &RenderVisibleEntities, &Msaa)>,
    mut opaque_render_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut next_tick: Local<Tick>,
) {
    let draw_function = draw_functions.read().get_id::<DrawAtomCluster>().unwrap();

    for (view, view_visible_entities, msaa) in views.iter() {
        let Some(opaque_phase) = opaque_render_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };

        // Only render atom clusters that are visible from this view
        for (entity, main_entity) in view_visible_entities.get::<AtomCluster>().iter() {
            let pipeline_key = AtomClusterPipelineKey::from_msaa_samples(msaa.samples());

            let pipeline_id =
                pipelines.specialize(&pipeline_cache, &atom_cluster_pipeline, pipeline_key);

            // Bump the change tick in order to force Bevy to rebuild the bin.
            let this_tick = *next_tick;
            next_tick.set(this_tick.get() + 1);

            opaque_phase.add(
                Opaque3dBatchSetKey {
                    draw_function,
                    pipeline: pipeline_id,
                    material_bind_group_index: None,
                    lightmap_slab: None,
                    vertex_slab: default(),
                    index_slab: None,
                },
                Opaque3dBinKey {
                    asset_id: AssetId::<Mesh>::invalid().untyped(),
                },
                (*entity, *main_entity),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
                this_tick,
            );
        }
    }
}

// Render command
type DrawAtomCluster = (
    // Configures shaders, vertex layout, blend mode, etc.
    SetItemPipeline,
    // Binds the camera/view uniforms to bind group slot 0
    // Binds the entity transform to bind group slot 1
    // Binds the periodic table to bind group slot 2
    SetAtomClusterBindGroup,
    // Custom render command below
    DrawAtomClusterInstanced,
);

struct SetAtomClusterBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetAtomClusterBindGroup {
    type Param = ();
    type ViewQuery = &'static ViewUniformOffset;
    type ItemQuery = &'static AtomClusterViewBindGroup;

    fn render<'w>(
        _item: &P,
        view_offset: &'w ViewUniformOffset,
        entity: Option<&'w AtomClusterViewBindGroup>,
        _: (),
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(view_bind_group) = entity {
            pass.set_bind_group(0, &view_bind_group.bind_group, &[view_offset.offset]);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("Missing atom cluster view bind group")
        }
    }
}

struct DrawAtomClusterInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawAtomClusterInstanced {
    type Param = SRes<SharedAtomClusterGpuBuffers>;
    type ViewQuery = ();
    type ItemQuery = &'static AtomClusterGpuBuffers;

    fn render<'w>(
        _item: &P,
        _view: (),
        instance_buffers: Option<&'w AtomClusterGpuBuffers>,
        shared_buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(gpu_buffers) = instance_buffers {
            let shared_buffers = shared_buffers.into_inner();
            pass.set_vertex_buffer(0, shared_buffers.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, gpu_buffers.instance_buffer.slice(..));
            pass.draw(0..4, 0..gpu_buffers.instance_count);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("No instance buffers found")
        }
    }
}

// End of File
