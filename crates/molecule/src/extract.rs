// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::components::{AtomInstance, DenormalizedBondInstance, Molecule};
use bevy::{
    ecs::query::QueryItem, prelude::*, render::extract_component::ExtractComponent,
    render::sync_world::MainEntity,
};

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

// End of File
