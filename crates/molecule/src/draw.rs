// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    buffers::{AtomGpuBuffers, BondGpuBuffers, SharedMoleculeGpuBuffers},
    extract::{ExtractedAtoms, ExtractedBonds},
    pipelines::{AtomRenderPipeline, BondRenderPipeline, MoleculeRenderPipelineKey},
    uniforms::{SetAtomUniformsBindGroup, SetBondUniformsBindGroup},
};
use bevy::{
    core_pipeline::core_3d::{Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey},
    ecs::{
        change_detection::Tick,
        system::{SystemParamItem, lifetimeless::SRes},
    },
    prelude::*,
    render::{
        render_phase::{
            BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewBinnedRenderPhases,
        },
        render_resource::{CachedPipelineState, PipelineCache, SpecializedRenderPipelines},
        view::ExtractedView,
    },
};

/// Queues draw commands for atoms and bonds.
///
/// This system is responsible for queuing the actual draw commands for both atoms
/// and bonds. It handles pipeline specialization, bind group creation, and proper
/// ordering of draw calls for correct depth sorting.
pub(crate) fn queue_molecule_draw_commands(
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

/// A tuple of render commands that handle atom rendering.
///
/// This type alias defines the sequence of render commands needed to draw atoms:
/// 1. `SetItemPipeline` - Configures the shader pipeline, vertex layout, and blend mode
/// 2. `SetAtomUniformsBindGroup` - Binds the uniforms (view, transform, periodic table)
/// 3. `DrawAtomsInstanced` - Issues the actual draw call for the atom billboards
pub(crate) type DrawAtoms = (
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
pub(crate) type DrawBonds = (
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
pub(crate) struct DrawAtomsInstanced;

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
            pass.set_vertex_buffer(0, shared_buffers.sphere_billboard_vertex_buffer().slice(..));
            pass.set_vertex_buffer(1, gpu_buffers.atoms_buffer().slice(..));
            pass.draw(0..4, 0..gpu_buffers.atoms_count());
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
pub(crate) struct DrawBondsInstanced;

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
            pass.set_vertex_buffer(
                0,
                shared_buffers.capsule_billboard_vertex_buffer().slice(..),
            );
            pass.set_vertex_buffer(1, gpu_buffers.bonds_buffer().slice(..));
            pass.draw(0..4, 0..gpu_buffers.bonds_count());
            RenderCommandResult::Success
        } else {
            RenderCommandResult::Failure("No bond instance buffers found")
        }
    }
}

// End of File
