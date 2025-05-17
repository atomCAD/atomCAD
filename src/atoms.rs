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
            BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntry, BindingType,
            Buffer, BufferBindingType, BufferInitDescriptor, BufferUsages, ColorTargetState,
            ColorWrites, CompareFunction, DepthStencilState, FragmentState, FrontFace,
            MultisampleState, PipelineCache, PolygonMode, PrimitiveState, RenderPipelineDescriptor,
            ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines,
            TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
            VertexStepMode,
        },
        renderer::RenderDevice,
        view::{
            ExtractedView, RenderVisibleEntities, ViewUniform, ViewUniformOffset, ViewUniforms,
            VisibilityClass, add_visibility_class,
        },
    },
};
use bytemuck::{Pod, Zeroable};

// Plugin
pub struct AtomClusterPlugin;

impl Plugin for AtomClusterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<AtomCluster>::default());
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
        let atom_geometry_buffers = SharedAtomClusterGpuBuffers { vertex_buffer };

        render_app
            .insert_resource(atom_geometry_buffers)
            // Requires AssetServer, so can't be done in build()
            .init_resource::<AtomClusterPipeline>()
            .init_resource::<SpecializedRenderPipelines<AtomClusterPipeline>>();
    }
}

// Component that holds our atom data in the ECS
#[derive(Component, Clone)]
#[require(VisibilityClass)]
#[component(on_add = add_visibility_class::<AtomCluster>)]
pub struct AtomCluster {
    pub atoms: Vec<Vec4>, // x, y, z, radius
}

// Extracted component for the render world
#[derive(Component, Clone)]
pub struct ExtractedAtomCluster {
    atoms: Vec<Vec4>,
}

impl ExtractComponent for AtomCluster {
    type QueryData = &'static AtomCluster;
    type QueryFilter = ();
    type Out = ExtractedAtomCluster;

    fn extract_component(item: QueryItem<'_, Self::QueryData>) -> Option<Self::Out> {
        Some(ExtractedAtomCluster {
            atoms: item.atoms.clone(),
        })
    }
}

// GPU representation of atom instance data
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuAtomInstance {
    position_radius: [f32; 4],
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
}

// GPU buffers for atom cluster
#[derive(Component)]
struct AtomClusterGpuBuffers {
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
                        array_stride: std::mem::size_of::<[f32; 4]>() as u64,
                        step_mode: VertexStepMode::Instance,
                        attributes: vec![VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 1,
                        }],
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
        // Convert atom data to GPU format
        let instances: Vec<GpuAtomInstance> = atom_cluster
            .atoms
            .iter()
            .map(|atom| GpuAtomInstance {
                position_radius: atom.to_array(),
            })
            .collect();

        let instance_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atom_cluster_instance_buffer"),
            contents: bytemuck::cast_slice(&instances),
            usage: BufferUsages::VERTEX,
        });

        commands.entity(entity).insert(AtomClusterGpuBuffers {
            instance_buffer,
            instance_count: instances.len() as u32,
        });
    }
}

// Create bind groups in a separate system
fn prepare_atom_cluster_view_bind_groups(
    mut commands: Commands,
    view_uniforms: Res<ViewUniforms>,
    pipeline: Res<AtomClusterPipeline>,
    render_device: Res<RenderDevice>,
    views: Query<(Entity, &ViewUniformOffset)>,
) {
    for (entity, _) in views.iter() {
        let bind_group = render_device.create_bind_group(
            Some("atom_cluster_view_bind_group"),
            &pipeline.bind_group_layout,
            &BindGroupEntries::single(view_uniforms.uniforms.binding().unwrap()),
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
    // Binds the model/transform uniforms to bind group slot 1
    SetAtomClusterViewBindGroup,
    // Custom render command below
    DrawAtomClusterInstanced,
);

// Simpler render command that uses the cached bind group
struct SetAtomClusterViewBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetAtomClusterViewBindGroup {
    type Param = ();
    type ViewQuery = (
        &'static ViewUniformOffset,
        &'static AtomClusterViewBindGroup,
    );
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        (view_offset, view_bind_group): (&'w ViewUniformOffset, &'w AtomClusterViewBindGroup),
        _entity: Option<()>, // Correct type
        _: (),
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(0, &view_bind_group.bind_group, &[view_offset.offset]);
        RenderCommandResult::Success
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
            warn!("No instance buffers found");
            RenderCommandResult::Failure("No instance buffers found")
        }
    }
}

// End of File
