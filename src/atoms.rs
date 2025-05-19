// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bytemuck::{Pod, Zeroable};
use periodic_table::PeriodicTable;

use bevy::{
    camera::visibility::{VisibilityClass, add_visibility_class},
    core_pipeline::core_3d::{CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey},
    ecs::{
        change_detection::Tick,
        query::QueryItem,
        system::{Query, SystemParamItem, lifetimeless::SRes},
    },
    mesh::VertexBufferLayout,
    prelude::*,
    reflect::Reflect,
    render::{
        Render, RenderApp, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem,
            RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
            ViewBinnedRenderPhases,
        },
        render_resource::{
            BindGroup, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
            BindingType, Buffer, BufferBindingType, BufferInitDescriptor, BufferUsages,
            CachedPipelineState, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
            DepthStencilState, FragmentState, FrontFace, MultisampleState, PipelineCache,
            PolygonMode, PrimitiveState, RenderPipelineDescriptor, ShaderStages, ShaderType,
            SpecializedRenderPipeline, SpecializedRenderPipelines, TextureFormat, VertexAttribute,
            VertexFormat, VertexState, VertexStepMode,
        },
        renderer::RenderDevice,
        sync_world::MainEntity,
        view::{ExtractedView, ViewUniform, ViewUniformOffset, ViewUniforms},
    },
};
use wgpu_types::PrimitiveTopology;

/// Represents a single atom in 3D space with its position and element type.
///
/// The `kind` field stores the atomic number (element ID) of the atom, which is used
/// to look up properties like radius and color from the periodic table.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Reflect)]
pub struct AtomInstance {
    pub position: Vec3,
    pub kind: u32,
}

/// Represents a chemical bond between two atoms.
///
/// The `atoms` array contains indices into the parent molecule's atom list, allowing
/// efficient storage of bond information without duplicating atom data.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Reflect)]
pub struct BondInstance {
    pub atoms: [u32; 2],
}

/// Internal representation of a bond with denormalized atom data.
///
/// This type is used during rendering to avoid indirect lookups of atom data within GPU
/// shaders when drawing bonds. It stores the complete atom data rather than just indices.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Reflect)]
pub struct DenormalizedBondInstance {
    atoms: [AtomInstance; 2],
}

/// A component that holds the complete molecular structure data.
///
/// This component stores both the atoms and bonds that make up a molecule, along with
/// their spatial relationships. It's designed to work with Bevy's ECS and rendering
/// systems to efficiently render molecular structures.
///
/// # Example
/// ```rust
/// use bevy::prelude::*;
/// use atomcad::{Molecule, AtomInstance, BondInstance};
///
/// # #[cfg(test)]
/// # mod tests {
/// #     use super::*;
/// fn spawn_molecule(commands: &mut Commands) {
///     commands.spawn((
///         Molecule {
///             atoms: vec![
///                 AtomInstance { position: Vec3::ZERO, kind: 1 }, // Hydrogen
///                 AtomInstance { position: Vec3::X, kind: 8 },    // Oxygen
///             ],
///             bonds: vec![
///                 BondInstance { atoms: [0, 1] }, // Bond between H and O
///             ],
///         },
///         TransformBundle::default(),
///     ));
/// }
/// # }
/// ```
#[derive(Component, Clone, Reflect)]
#[require(VisibilityClass)]
#[component(on_add = add_visibility_class::<Molecule>)]
pub struct Molecule {
    pub atoms: Vec<AtomInstance>,
    pub bonds: Vec<BondInstance>,
}

/// A resource that holds handles to the shaders used for rendering molecules.
///
/// This resource stores the compiled WGSL shader handles for both atom and bond rendering.
/// The shaders are loaded during plugin initialization and shared across all molecule
/// instances to avoid redundant shader compilation.
///
/// # Fields
/// * `atoms_shader` - Handle to the shader used for rendering atom spheres
/// * `bonds_shader` - Handle to the shader used for rendering bond capsules
#[derive(Resource)]
struct MoleculeShaders {
    atoms_shader: Handle<Shader>,
    bonds_shader: Handle<Shader>,
}

/// A plugin that enables rendering of molecular structures in 3D space.
///
/// This plugin provides the necessary systems and resources for rendering atoms as spheres
/// and bonds as cylinders between atoms. It handles the GPU buffer management, shader loading,
/// and render pipeline setup required for efficient instanced rendering of molecular structures.
pub struct MoleculeRenderPlugin;

impl Plugin for MoleculeRenderPlugin {
    fn build(&self, app: &mut App) {
        // Register types for the ECS
        app.register_type::<AtomInstance>()
            .register_type::<BondInstance>()
            .register_type::<Molecule>();

        // Extract molecules from ECS to render world
        app.add_plugins(ExtractComponentPlugin::<Molecule>::default());

        // Get the render app (cannot make changes to app after this)
        let render_app = app.sub_app_mut(RenderApp);

        // Add render commands to the render app
        render_app
            .add_render_command::<Opaque3d, DrawAtoms>()
            .add_render_command::<Opaque3d, DrawBonds>();

        // Add molecule drawing systems to the render app
        render_app.add_systems(
            Render,
            (
                // Resource preparation systems
                prepare_atom_buffers.in_set(RenderSystems::PrepareResources),
                prepare_bond_buffers.in_set(RenderSystems::PrepareResources),
                // Bind group systems
                prepare_atom_uniforms_bind_group.in_set(RenderSystems::PrepareBindGroups),
                prepare_bond_uniforms_bind_group.in_set(RenderSystems::PrepareBindGroups),
                // Queue rendering
                queue_molecule_draw_commands.in_set(RenderSystems::Queue),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        // Steps that require AssetServer or RenderDevice, so can't be done in build()
        let render_app = app.sub_app_mut(RenderApp);
        let render_device = render_app.world().resource::<RenderDevice>();
        let asset_server = render_app.world().resource::<AssetServer>();

        // Load the molecule shaders
        let atoms_shader = asset_server.load("shaders/atoms.wgsl");
        let bonds_shader = asset_server.load("shaders/bonds.wgsl");
        let molecule_shaders = MoleculeShaders {
            atoms_shader,
            bonds_shader,
        };

        // Create quad vertices for atoms & bonds
        let vertices = vec![
            // Bottom-left
            Vec3::new(-1.0, -1.0, 0.0),
            // Bottom-right
            Vec3::new(1.0, -1.0, 0.0),
            // Top-left
            Vec3::new(-1.0, 1.0, 0.0),
            // Top-right
            Vec3::new(1.0, 1.0, 0.0),
        ];
        let sphere_billboard_vertex_buffer =
            render_device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("sphere_billboard_vertex_buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: BufferUsages::VERTEX,
            });
        let capsule_billboard_vertex_buffer =
            render_device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("capsule_billboard_vertex_buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: BufferUsages::VERTEX,
            });

        // Create the periodic table buffer
        let periodic_table = PeriodicTable::with_vdw_scale(0.25);
        let periodic_table_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("periodic_table_buffer"),
            contents: bytemuck::cast_slice(&[periodic_table]),
            usage: BufferUsages::UNIFORM,
        });

        let shared_molecule_buffers = SharedMoleculeGpuBuffers {
            sphere_billboard_vertex_buffer,
            capsule_billboard_vertex_buffer,
            periodic_table_buffer,
        };

        render_app
            .insert_resource(molecule_shaders)
            .insert_resource(shared_molecule_buffers)
            .init_resource::<AtomRenderPipeline>()
            .init_resource::<BondRenderPipeline>()
            .init_resource::<SpecializedRenderPipelines<AtomRenderPipeline>>()
            .init_resource::<SpecializedRenderPipelines<BondRenderPipeline>>();
    }
}

/// Prepares GPU buffers for atom instances.
///
/// This system creates the necessary GPU buffers for rendering atoms, including
/// the transform buffer and instance data buffer. It's called during the prepare
/// phase of the render pipeline.
fn prepare_atom_buffers(
    mut commands: Commands,
    query: Query<(Entity, &ExtractedAtoms), Without<AtomGpuBuffers>>,
    render_device: Res<RenderDevice>,
) {
    for (entity, atoms) in query.iter() {
        // Create transform buffer
        let transform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atom_transform_buffer"),
            contents: bytemuck::cast_slice(&[atoms.transform]),
            usage: BufferUsages::UNIFORM,
        });

        // Create atom buffer
        let atoms_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atoms_buffer"),
            contents: bytemuck::cast_slice(&atoms.atoms),
            usage: BufferUsages::VERTEX,
        });

        commands.entity(entity).insert(AtomGpuBuffers {
            transform_buffer,
            atoms_buffer,
            atoms_count: atoms.atoms.len() as u32,
        });
    }
}

/// Prepares GPU buffers for bond instances.
///
/// This system creates the necessary GPU buffers for rendering bonds, including
/// the transform buffer and instance data buffer. It's called during the prepare
/// phase of the render pipeline.
fn prepare_bond_buffers(
    mut commands: Commands,
    query: Query<(Entity, &ExtractedBonds), Without<BondGpuBuffers>>,
    render_device: Res<RenderDevice>,
) {
    for (entity, bonds) in query.iter() {
        // Create transform buffer
        let transform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("bond_transform_buffer"),
            contents: bytemuck::cast_slice(&[bonds.transform]),
            usage: BufferUsages::UNIFORM,
        });

        // Create bond buffer
        let bonds_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("bonds_buffer"),
            contents: bytemuck::cast_slice(&bonds.bonds),
            usage: BufferUsages::VERTEX,
        });

        commands.entity(entity).insert(BondGpuBuffers {
            transform_buffer,
            bonds_buffer,
            bonds_count: bonds.bonds.len() as u32,
        });
    }
}

/// A component that holds the bind group for atom rendering uniforms.
///
/// This component stores the bind group that combines all the uniform buffers needed
/// for rendering atoms, including view uniforms, entity transforms, and periodic table data.
/// It is created during the prepare phase and used during rendering to bind the uniforms
/// to the shader.
#[derive(Component)]
struct AtomUniformsBindGroup {
    bind_group: BindGroup,
}

/// A component that holds the bind group for bond rendering uniforms.
///
/// This component stores the bind group that combines all the uniform buffers needed
/// for rendering bonds, including view uniforms, entity transforms, and periodic table data.
/// It is created during the prepare phase and used during rendering to bind the uniforms
/// to the shader.
#[derive(Component)]
struct BondUniformsBindGroup {
    bind_group: BindGroup,
}

/// Prepares the bind group for atom uniforms.
///
/// This system creates the bind group that combines the view uniforms, entity transform,
/// and periodic table data needed for rendering atoms. It's called during the prepare
/// phase of the render pipeline after the GPU buffers are created.
///
/// The bind group contains:
/// - View uniforms (binding 0): Camera and viewport information
/// - Entity transform (binding 1): World transform of the molecule
/// - Periodic table (binding 2): Element properties used for rendering
fn prepare_atom_uniforms_bind_group(
    mut commands: Commands,
    view_uniforms: Res<ViewUniforms>,
    shared_molecule_buffers: Res<SharedMoleculeGpuBuffers>,
    pipeline: Res<AtomRenderPipeline>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
    atom_buffers: Query<(Entity, &AtomGpuBuffers)>,
) {
    for (entity, gpu_buffers) in atom_buffers.iter() {
        let bind_group = render_device.create_bind_group(
            Some("atom_uniforms_bind_group"),
            &pipeline_cache.get_bind_group_layout(&pipeline.bind_group_layout),
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
                    resource: shared_molecule_buffers
                        .periodic_table_buffer
                        .as_entire_binding(),
                },
            ],
        );

        commands
            .entity(entity)
            .insert(AtomUniformsBindGroup { bind_group });
    }
}

/// Prepares the bind group for bond uniforms.
///
/// This system creates the bind group that combines the view uniforms, entity transform,
/// and periodic table data needed for rendering bonds. It's called during the prepare
/// phase of the render pipeline after the GPU buffers are created.
///
/// The bind group contains:
/// - View uniforms (binding 0): Camera and viewport information
/// - Entity transform (binding 1): World transform of the molecule
/// - Periodic table (binding 2): Element properties used for rendering
fn prepare_bond_uniforms_bind_group(
    mut commands: Commands,
    view_uniforms: Res<ViewUniforms>,
    shared_molecule_buffers: Res<SharedMoleculeGpuBuffers>,
    pipeline: Res<BondRenderPipeline>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
    bond_buffers: Query<(Entity, &BondGpuBuffers)>,
) {
    for (entity, gpu_buffers) in bond_buffers.iter() {
        let bind_group = render_device.create_bind_group(
            Some("bond_uniforms_bind_group"),
            &pipeline_cache.get_bind_group_layout(&pipeline.bind_group_layout),
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
                    resource: shared_molecule_buffers
                        .periodic_table_buffer
                        .as_entire_binding(),
                },
            ],
        );

        commands
            .entity(entity)
            .insert(BondUniformsBindGroup { bind_group });
    }
}

/// Queues draw commands for atoms and bonds.
///
/// This system is responsible for queuing the actual draw commands for both atoms
/// and bonds. It handles pipeline specialization, bind group creation, and proper
/// ordering of draw calls for correct depth sorting.
fn queue_molecule_draw_commands(
    // Common resources
    draw_functions: Res<DrawFunctions<Opaque3d>>,
    atom_render_pipeline: Res<AtomRenderPipeline>,
    bond_render_pipeline: Res<BondRenderPipeline>,
    mut atom_render_pipelines: ResMut<SpecializedRenderPipelines<AtomRenderPipeline>>,
    mut bond_render_pipelines: ResMut<SpecializedRenderPipelines<BondRenderPipeline>>,
    pipeline_cache: Res<PipelineCache>,

    // View information
    views: Query<(&ExtractedView, &Msaa)>,

    // Our render entities
    atom_entities: Query<(Entity, &ExtractedAtoms)>,
    bond_entities: Query<Entity, With<ExtractedBonds>>,

    // Render phases
    mut opaque_render_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,

    // Tick management
    mut next_atom_tick: Local<Tick>,
    mut next_bond_tick: Local<Tick>,
) {
    // Get draw function ids
    let draw_atoms = draw_functions.read().get_id::<DrawAtoms>().unwrap();
    let draw_bonds = draw_functions.read().get_id::<DrawBonds>().unwrap();

    // Process each view
    for (view, msaa) in views.iter() {
        let Some(opaque_phase) = opaque_render_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };

        // Create pipeline key based on msaa samples
        let pipeline_key = MoleculeRenderPipelineKey::from_msaa_samples(msaa.samples());

        // Specialize pipelines for this key
        let atom_pipeline_id = atom_render_pipelines.specialize(
            &pipeline_cache,
            &atom_render_pipeline,
            pipeline_key.clone(),
        );
        if let CachedPipelineState::Err(e) =
            pipeline_cache.get_render_pipeline_state(atom_pipeline_id)
        {
            error!("Atom pipeline error: {e:?}");
        }

        let bond_pipeline_id =
            bond_render_pipelines.specialize(&pipeline_cache, &bond_render_pipeline, pipeline_key);
        if let CachedPipelineState::Err(e) =
            pipeline_cache.get_render_pipeline_state(bond_pipeline_id)
        {
            error!("Bond pipeline error: {e:?}");
        }

        // Render atoms
        for (atom_entity, atoms) in atom_entities.iter() {
            // Bump tick for atoms
            let atom_tick = *next_atom_tick;
            next_atom_tick.set(atom_tick.get() + 1);

            // Add atom entity to render phase
            opaque_phase.add(
                Opaque3dBatchSetKey {
                    pipeline: atom_pipeline_id,
                    draw_function: draw_atoms,
                    material_bind_group_index: Some(0),
                    vertex_slab: default(),
                    lightmap_slab: None,
                    index_slab: None,
                },
                Opaque3dBinKey {
                    asset_id: AssetId::<Mesh>::invalid().untyped(),
                },
                (atom_entity, atoms.main_entity),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
                atom_tick,
            );
        }

        // Render bonds
        for bond_entity in bond_entities.iter() {
            // Bump tick for bonds
            let bond_tick = *next_bond_tick;
            next_bond_tick.set(bond_tick.get() + 1);

            // Add bond entity to render phase
            opaque_phase.add(
                Opaque3dBatchSetKey {
                    pipeline: bond_pipeline_id,
                    draw_function: draw_bonds,
                    material_bind_group_index: Some(1),
                    vertex_slab: default(),
                    lightmap_slab: None,
                    index_slab: None,
                },
                Opaque3dBinKey {
                    asset_id: AssetId::<Mesh>::invalid().untyped(),
                },
                // IMPORTANT: This may be a Bevy bug!
                //
                // Note that the entity used below is not the bonds.main_entity, which is what it
                // should be. For some reason if both the atom pass and the bond pass use the same
                // main-world entity (which they should), neither one renders. Likewise, if the
                // main-world entity isn't used for either, then we likewise get a blank screen. ONE
                // of the two must use the main-world entity, and the other must use a different
                // value. We arbitrarily pick the render-world entity ID, which should be
                // meaningless in the main-world. This works, but the whole thing smells like an
                // upstream bug.
                (bond_entity, bond_entity.into()),
                // (bond_entity, bonds.main_entity)
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
                bond_tick,
            );
        }
    }
}

/// Extracted component containing atom data ready for rendering.
///
/// This component is created during the extraction phase and contains the
/// transformed atom positions and instance data needed for GPU rendering.
#[derive(Component, Clone)]
pub struct ExtractedAtoms {
    pub transform: Mat4,
    pub atoms: Vec<AtomInstance>,
    pub main_entity: MainEntity,
}

/// Extracted component containing bond data ready for rendering.
///
/// This component is created during the extraction phase and contains the
/// transformed bond data with denormalized atom positions for GPU rendering.
#[derive(Component, Clone)]
pub struct ExtractedBonds {
    pub transform: Mat4,
    pub bonds: Vec<DenormalizedBondInstance>,
    pub main_entity: MainEntity,
}

impl ExtractComponent for Molecule {
    type QueryData = (
        Entity,
        &'static Molecule,
        &'static GlobalTransform,
        &'static ViewVisibility,
    );
    type QueryFilter = ();
    type Out = (ExtractedAtoms, ExtractedBonds);

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        let (entity, molecule, transform, visibility) = item;

        // Skip extraction for non-visible molecules
        if !visibility.get() {
            return None;
        }

        // Compute the transform matrix
        let transform = transform.to_matrix();

        // Denormalize bond data
        let bonds = molecule
            .bonds
            .iter()
            .map(|bond| DenormalizedBondInstance {
                atoms: [
                    molecule.atoms[bond.atoms[0] as usize],
                    molecule.atoms[bond.atoms[1] as usize],
                ],
            })
            .collect();

        // Return extracted components
        Some((
            // Create atoms
            ExtractedAtoms {
                transform,
                atoms: molecule.atoms.clone(),
                main_entity: entity.into(),
            },
            // Create bonds
            ExtractedBonds {
                transform,
                bonds,
                main_entity: entity.into(),
            },
        ))
    }
}

/// Shared GPU buffers used across all molecule instances.
///
/// These buffers contain the vertex data for billboard quads and the periodic table
/// data needed for rendering atoms and bonds. They are shared to minimize memory usage
/// and improve rendering performance.
#[derive(Resource)]
struct SharedMoleculeGpuBuffers {
    sphere_billboard_vertex_buffer: Buffer,
    capsule_billboard_vertex_buffer: Buffer,
    periodic_table_buffer: Buffer,
}

/// GPU buffers specific to a single molecule's atom instances.
///
/// These buffers store the per-instance data needed to render the atoms of a
/// specific molecule, including their positions and element types.
#[derive(Component)]
struct AtomGpuBuffers {
    transform_buffer: Buffer,
    atoms_buffer: Buffer,
    atoms_count: u32,
}

/// GPU buffers specific to a single molecule's bond instances.
///
/// These buffers store the per-instance data needed to render the bonds of a
/// specific molecule, including the denormalized positions and types of the
/// connected atoms.
#[derive(Component)]
struct BondGpuBuffers {
    transform_buffer: Buffer,
    bonds_buffer: Buffer,
    bonds_count: u32,
}

/// A trait that defines the common interface for molecule render pipelines.
///
/// This trait allows us to share code between atom and bond rendering pipelines
/// while maintaining type safety and specialization capabilities.
trait MoleculeRenderPipeline {
    fn new(shader: Handle<Shader>, bind_group_layout: BindGroupLayoutDescriptor) -> Self;
    fn shader(&self) -> &Handle<Shader>;
    fn bind_group_layout(&self) -> &BindGroupLayoutDescriptor;
}

/// A key used to specialize the molecule render pipeline based on MSAA settings.
///
/// This allows the render pipeline to be optimized for different anti-aliasing
/// configurations while maintaining the same basic rendering approach.
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct MoleculeRenderPipelineKey {
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
struct AtomRenderPipeline {
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
struct BondRenderPipeline {
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

/// A render command that sets the atom uniforms bind group.
///
/// This command is used in the render phase to bind the atom uniforms to the shader.
/// It retrieves the bind group from the entity and sets it in the render pass,
/// making the uniform data available to the shader.
struct SetAtomUniformsBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetAtomUniformsBindGroup {
    type Param = ();
    type ViewQuery = &'static ViewUniformOffset;
    type ItemQuery = &'static AtomUniformsBindGroup;

    fn render<'w>(
        _item: &P,
        view_offset: &'w ViewUniformOffset,
        entity: Option<&'w AtomUniformsBindGroup>,
        _: (),
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(uniforms_bind_group) = entity {
            pass.set_bind_group(0, &uniforms_bind_group.bind_group, &[view_offset.offset]);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("Missing atom uniforms bind group")
        }
    }
}

/// A render command that sets the bond uniforms bind group.
///
/// This command is used in the render phase to bind the bond uniforms to the shader.
/// It retrieves the bind group from the entity and sets it in the render pass,
/// making the uniform data available to the shader.
struct SetBondUniformsBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetBondUniformsBindGroup {
    type Param = ();
    type ViewQuery = &'static ViewUniformOffset;
    type ItemQuery = &'static BondUniformsBindGroup;

    fn render<'w>(
        _item: &P,
        view_offset: &'w ViewUniformOffset,
        entity: Option<&'w BondUniformsBindGroup>,
        _: (),
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(uniforms_bind_group) = entity {
            pass.set_bind_group(0, &uniforms_bind_group.bind_group, &[view_offset.offset]);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("Missing bond uniforms bind group")
        }
    }
}

/// A tuple of render commands that handle atom rendering.
///
/// This type alias defines the sequence of render commands needed to draw atoms:
/// 1. `SetItemPipeline` - Configures the shader pipeline, vertex layout, and blend mode
/// 2. `SetAtomUniformsBindGroup` - Binds the uniforms (view, transform, periodic table)
/// 3. `DrawAtomsInstanced` - Issues the actual draw call for the atom billboards
type DrawAtoms = (
    // Configures shaders, vertex layout, blend mode, etc.
    SetItemPipeline,
    // Binds the camera/view uniforms to bind group slot 0
    // Binds the entity transform to bind group slot 1
    // Binds the periodic table to bind group slot 2
    SetAtomUniformsBindGroup,
    // Custom render command
    DrawAtomsInstanced,
);

/// A tuple of render commands that handle bond rendering.
///
/// This type alias defines the sequence of render commands needed to draw bonds:
/// 1. `SetItemPipeline` - Configures the shader pipeline, vertex layout, and blend mode
/// 2. `SetBondUniformsBindGroup` - Binds the uniforms (view, transform, periodic table)
/// 3. `DrawBondsInstanced` - Issues the actual draw call for the bond billboards
type DrawBonds = (
    // Configures shaders, vertex layout, blend mode, etc.
    SetItemPipeline,
    // Binds the camera/view uniforms to bind group slot 0
    // Binds the entity transform to bind group slot 1
    // Binds the periodic table to bind group slot 2
    SetBondUniformsBindGroup,
    // Custom render command
    DrawBondsInstanced,
);

/// Draws instanced atom billboards.
///
/// This command handles the actual drawing of atom instances using the prepared GPU buffers.
/// It sets up the vertex buffers and issues the draw call for the instanced billboards
/// that represent atoms as spheres.
struct DrawAtomsInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawAtomsInstanced {
    type Param = SRes<SharedMoleculeGpuBuffers>;
    type ViewQuery = ();
    type ItemQuery = &'static AtomGpuBuffers;

    fn render<'w>(
        _item: &P,
        _view: (),
        instance_buffers: Option<&'w AtomGpuBuffers>,
        shared_buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(gpu_buffers) = instance_buffers {
            let shared_buffers = shared_buffers.into_inner();
            pass.set_vertex_buffer(0, shared_buffers.sphere_billboard_vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, gpu_buffers.atoms_buffer.slice(..));
            pass.draw(0..4, 0..gpu_buffers.atoms_count);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("No atom instance buffers found")
        }
    }
}

/// Draws instanced bond billboards.
///
/// This command handles the actual drawing of bond instances using the prepared GPU buffers.
/// It sets up the vertex buffers and issues the draw call for the instanced billboards
/// that represent bonds as capsules between atoms.
struct DrawBondsInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawBondsInstanced {
    type Param = SRes<SharedMoleculeGpuBuffers>;
    type ViewQuery = ();
    type ItemQuery = &'static BondGpuBuffers;

    fn render<'w>(
        _item: &P,
        _view: (),
        instance_buffers: Option<&'w BondGpuBuffers>,
        shared_buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        if let Some(gpu_buffers) = instance_buffers {
            let shared_buffers = shared_buffers.into_inner();
            pass.set_vertex_buffer(0, shared_buffers.capsule_billboard_vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, gpu_buffers.bonds_buffer.slice(..));
            pass.draw(0..4, 0..gpu_buffers.bonds_count);
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("No bond instance buffers found")
        }
    }
}

// End of File
