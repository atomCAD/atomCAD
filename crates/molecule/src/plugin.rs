// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    assets::MoleculeShaders,
    buffers::{SharedMoleculeGpuBuffers, prepare_atom_buffers, prepare_bond_buffers},
    components::{AtomInstance, BondInstance, Molecule},
    draw::{DrawAtoms, DrawBonds, queue_molecule_draw_commands},
    pipelines::{AtomRenderPipeline, BondRenderPipeline},
    uniforms::{prepare_atom_uniforms_bind_group, prepare_bond_uniforms_bind_group},
};
use bevy::{
    core_pipeline::core_3d::Opaque3d,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, extract_component::ExtractComponentPlugin,
        render_phase::AddRenderCommand, render_resource::SpecializedRenderPipelines,
        renderer::RenderDevice,
    },
};
use periodic_table::PeriodicTable;

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
                // Create GPU buffers for extracted atoms and bonds
                prepare_atom_buffers.in_set(RenderSystems::PrepareResources),
                prepare_bond_buffers.in_set(RenderSystems::PrepareResources),
                // Prepare bind groups for uniforms buffers
                // TODO: This probably shouldn't be done every frame?
                prepare_atom_uniforms_bind_group.in_set(RenderSystems::PrepareBindGroups),
                prepare_bond_uniforms_bind_group.in_set(RenderSystems::PrepareBindGroups),
                // Queue rendering steps for atoms and bonds
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
        let molecule_shaders = MoleculeShaders::new(asset_server);

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

        // Create the periodic table buffer
        let periodic_table = PeriodicTable::new();

        let shared_molecule_buffers =
            SharedMoleculeGpuBuffers::new(render_device, &vertices, &vertices, &periodic_table);

        render_app
            .insert_resource(molecule_shaders)
            .insert_resource(shared_molecule_buffers)
            .init_resource::<AtomRenderPipeline>()
            .init_resource::<BondRenderPipeline>()
            .init_resource::<SpecializedRenderPipelines<AtomRenderPipeline>>()
            .init_resource::<SpecializedRenderPipelines<BondRenderPipeline>>();
    }
}

// End of File
