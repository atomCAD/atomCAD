// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    buffers::{AtomGpuBuffers, BondGpuBuffers, SharedMoleculeGpuBuffers},
    pipelines::{AtomRenderPipeline, BondRenderPipeline, MoleculeRenderPipeline},
};
use bevy::{
    prelude::*,
    render::{
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{BindGroup, BindGroupEntry, PipelineCache},
        renderer::RenderDevice,
        view::{ViewUniformOffset, ViewUniforms},
    },
};

/// A component that holds the bind group for atom rendering uniforms.
///
/// This component stores the bind group that combines all the uniform buffers needed
/// for rendering atoms, including view uniforms, entity transforms, and periodic table data.
/// It is created during the prepare phase and used during rendering to bind the uniforms
/// to the shader.
#[derive(Component)]
pub(crate) struct AtomUniformsBindGroup {
    bind_group: BindGroup,
}

/// A component that holds the bind group for bond rendering uniforms.
///
/// This component stores the bind group that combines all the uniform buffers needed
/// for rendering bonds, including view uniforms, entity transforms, and periodic table data.
/// It is created during the prepare phase and used during rendering to bind the uniforms
/// to the shader.
#[derive(Component)]
pub(crate) struct BondUniformsBindGroup {
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
pub(crate) fn prepare_atom_uniforms_bind_group(
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
            &pipeline_cache.get_bind_group_layout(pipeline.bind_group_layout()),
            &[
                // Binding 0: View uniforms
                BindGroupEntry {
                    binding: 0,
                    resource: view_uniforms.uniforms.binding().unwrap(),
                },
                // Binding 1: Entity global transform
                BindGroupEntry {
                    binding: 1,
                    resource: gpu_buffers.transform_buffer().as_entire_binding(),
                },
                // Binding 2: Periodic table
                BindGroupEntry {
                    binding: 2,
                    resource: shared_molecule_buffers
                        .periodic_table_buffer()
                        .as_entire_binding(),
                },
                // Binding 3: VDW scale
                BindGroupEntry {
                    binding: 3,
                    resource: gpu_buffers.vdw_scale_buffer().as_entire_binding(),
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
pub(crate) fn prepare_bond_uniforms_bind_group(
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
            &pipeline_cache.get_bind_group_layout(pipeline.bind_group_layout()),
            &[
                // Binding 0: View uniforms
                BindGroupEntry {
                    binding: 0,
                    resource: view_uniforms.uniforms.binding().unwrap(),
                },
                // Binding 1: Entity global transform
                BindGroupEntry {
                    binding: 1,
                    resource: gpu_buffers.transform_buffer().as_entire_binding(),
                },
                // Binding 2: Periodic table
                BindGroupEntry {
                    binding: 2,
                    resource: shared_molecule_buffers
                        .periodic_table_buffer()
                        .as_entire_binding(),
                },
                // Binding 3: VDW scale
                BindGroupEntry {
                    binding: 3,
                    resource: gpu_buffers.vdw_scale_buffer().as_entire_binding(),
                },
            ],
        );

        commands
            .entity(entity)
            .insert(BondUniformsBindGroup { bind_group });
    }
}

/// A render command that sets the atom uniforms bind group.
///
/// This command is used in the render phase to bind the atom uniforms to the shader.
/// It retrieves the bind group from the entity and sets it in the render pass,
/// making the uniform data available to the shader.
pub(crate) struct SetAtomUniformsBindGroup;

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
pub(crate) struct SetBondUniformsBindGroup;

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

// End of File
