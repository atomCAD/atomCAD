// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::config::MoleculeRenderConfig;
use bevy::{
    camera::visibility::{VisibilityClass, add_visibility_class},
    prelude::*,
};
use bytemuck::{Pod, Zeroable};

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
    pub atoms: [AtomInstance; 2],
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
/// # use atomcad_molecule as molecule;
/// use molecule::{Molecule, AtomInstance, BondInstance};
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
#[require(MoleculeRenderConfig)]
#[component(on_add = add_visibility_class::<Molecule>)]
pub struct Molecule {
    pub atoms: Vec<AtomInstance>,
    pub bonds: Vec<BondInstance>,
}

// End of File
