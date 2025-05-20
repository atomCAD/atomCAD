// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::extract::{ExtractedAtoms, ExtractedBonds};
use bevy::{
    prelude::*,
    render::{
        render_resource::{Buffer, BufferInitDescriptor, BufferUsages},
        renderer::RenderDevice,
    },
};
use bytemuck::{Pod, Zeroable};
use periodic_table::PeriodicTable;

/// Shared GPU buffers used across all molecule instances.
///
/// These buffers contain the vertex data for billboard quads and the periodic table
/// data needed for rendering atoms and bonds. They are shared to minimize memory usage
/// and improve rendering performance.
#[derive(Resource)]
pub(crate) struct SharedMoleculeGpuBuffers {
    pub(crate) sphere_billboard_vertex_buffer: Buffer,
    pub(crate) capsule_billboard_vertex_buffer: Buffer,
    pub(crate) periodic_table_buffer: Buffer,
}

impl SharedMoleculeGpuBuffers {
    pub(crate) fn new(
        render_device: &RenderDevice,
        sphere_billboard_vertices: &[Vec3],
        capsule_billboard_vertices: &[Vec3],
        periodic_table: &PeriodicTable,
    ) -> Self {
        Self {
            sphere_billboard_vertex_buffer: render_device.create_buffer_with_data(
                &BufferInitDescriptor {
                    label: Some("sphere_billboard_vertex_buffer"),
                    contents: bytemuck::cast_slice(sphere_billboard_vertices),
                    usage: BufferUsages::VERTEX,
                },
            ),
            capsule_billboard_vertex_buffer: render_device.create_buffer_with_data(
                &BufferInitDescriptor {
                    label: Some("capsule_billboard_vertex_buffer"),
                    contents: bytemuck::cast_slice(capsule_billboard_vertices),
                    usage: BufferUsages::VERTEX,
                },
            ),
            periodic_table_buffer: render_device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("periodic_table_buffer"),
                contents: bytemuck::cast_slice(&[*periodic_table]),
                usage: BufferUsages::UNIFORM,
            }),
        }
    }

    pub(crate) fn sphere_billboard_vertex_buffer(&self) -> &Buffer {
        &self.sphere_billboard_vertex_buffer
    }

    pub(crate) fn capsule_billboard_vertex_buffer(&self) -> &Buffer {
        &self.capsule_billboard_vertex_buffer
    }

    pub(crate) fn periodic_table_buffer(&self) -> &Buffer {
        &self.periodic_table_buffer
    }
}

/// Uniform buffer containing the van der Waals radius scale factor.
///
/// This structure is used to pass the vdW scale value to the GPU shaders.
/// It includes padding to ensure proper alignment for GPU buffer requirements.
/// The scale factor is applied to all atom radii during rendering.
#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct VdwScaleUniform {
    /// The scale factor applied to van der Waals radii.
    ///
    /// Values greater than 1.0 make atoms appear larger,
    /// while values less than 1.0 make them appear smaller.
    scale: f32,
    /// Padding to ensure 16-byte alignment required by GPU.
    _padding: [f32; 3],
}

impl VdwScaleUniform {
    /// Creates a new uniform with the specified scale factor.
    ///
    /// # Arguments
    ///
    /// * `scale` - The scale factor to apply to van der Waals radii
    pub(crate) fn new(scale: f32) -> Self {
        Self {
            scale,
            _padding: [0.0; 3],
        }
    }
}
/// GPU buffers specific to a single molecule's atom instances.
///
/// These buffers store the per-instance data needed to render the atoms of a
/// specific molecule, including their positions and element types.
#[derive(Component)]
pub(crate) struct AtomGpuBuffers {
    transform_buffer: Buffer,
    vdw_scale_buffer: Buffer,
    atoms_buffer: Buffer,
    atoms_count: u32,
}

impl AtomGpuBuffers {
    pub(crate) fn transform_buffer(&self) -> &Buffer {
        &self.transform_buffer
    }

    pub(crate) fn vdw_scale_buffer(&self) -> &Buffer {
        &self.vdw_scale_buffer
    }

    pub(crate) fn atoms_buffer(&self) -> &Buffer {
        &self.atoms_buffer
    }

    pub(crate) fn atoms_count(&self) -> u32 {
        self.atoms_count
    }
}

/// GPU buffers specific to a single molecule's bond instances.
///
/// These buffers store the per-instance data needed to render the bonds of a
/// specific molecule, including the denormalized positions and types of the
/// connected atoms.
#[derive(Component)]
pub(crate) struct BondGpuBuffers {
    transform_buffer: Buffer,
    vdw_scale_buffer: Buffer,
    bonds_buffer: Buffer,
    bonds_count: u32,
}

impl BondGpuBuffers {
    pub(crate) fn transform_buffer(&self) -> &Buffer {
        &self.transform_buffer
    }

    pub(crate) fn vdw_scale_buffer(&self) -> &Buffer {
        &self.vdw_scale_buffer
    }

    pub(crate) fn bonds_buffer(&self) -> &Buffer {
        &self.bonds_buffer
    }
    pub(crate) fn bonds_count(&self) -> u32 {
        self.bonds_count
    }
}

/// Prepares GPU buffers for atom instances.
///
/// This system creates the necessary GPU buffers for rendering atoms, including
/// the transform buffer and instance data buffer. It's called during the prepare
/// phase of the render pipeline.
pub(crate) fn prepare_atom_buffers(
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

        // Create vdw scale buffer
        let vdw_scale = VdwScaleUniform::new(atoms.vdw_scale);
        let vdw_scale_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("atom_vdw_scale_buffer"),
            contents: bytemuck::cast_slice(&[vdw_scale]),
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
            vdw_scale_buffer,
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
pub(crate) fn prepare_bond_buffers(
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

        // Create vdw scale buffer
        let vdw_scale = VdwScaleUniform::new(bonds.vdw_scale);
        let vdw_scale_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("bond_vdw_scale_buffer"),
            contents: bytemuck::cast_slice(&[vdw_scale]),
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
            vdw_scale_buffer,
            bonds_buffer,
            bonds_count: bonds.bonds.len() as u32,
        });
    }
}

// End of File
