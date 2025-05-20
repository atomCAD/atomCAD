// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::{
    components::{AtomInstance, DenormalizedBondInstance, Molecule},
    config::MoleculeRenderConfig,
};
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
    pub vdw_scale: f32,
    pub atoms: Vec<AtomInstance>,
    pub main_entity: Option<MainEntity>,
}

/// Extracted component containing bond data ready for rendering.
///
/// This component is created during the extraction phase and contains the
/// transformed bond data with denormalized atom positions for GPU rendering.
#[derive(Component, Clone)]
pub struct ExtractedBonds {
    pub transform: Mat4,
    pub vdw_scale: f32,
    pub bonds: Vec<DenormalizedBondInstance>,
    pub main_entity: Option<MainEntity>,
}

impl ExtractComponent for Molecule {
    type QueryData = (
        Entity,
        &'static Molecule,
        &'static GlobalTransform,
        &'static ViewVisibility,
        &'static MoleculeRenderConfig,
    );
    type QueryFilter = ();
    type Out = (ExtractedAtoms, ExtractedBonds);

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        let (entity, molecule, transform, visibility, config) = item;

        // Skip extraction for non-visible molecules
        if !visibility.get() {
            return None;
        }

        // Compute the transform matrix
        let transform = transform.to_matrix();

        // Extract atoms
        let atoms = if config.show_atoms {
            ExtractedAtoms {
                transform,
                vdw_scale: config.vdw_scale,
                atoms: molecule.atoms.clone(),
                main_entity: Some(entity.into()),
            }
        } else {
            ExtractedAtoms {
                transform,
                vdw_scale: config.vdw_scale,
                atoms: Vec::new(),
                main_entity: None,
            }
        };

        // Extract bonds
        let bonds = if config.show_bonds {
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

            ExtractedBonds {
                transform,
                vdw_scale: config.vdw_scale,
                bonds,
                // IMPORTANT: This may be a Bevy bug!
                //
                // If atoms.main_entity is set, then bonds.main_entity will not be. For some reason
                // if both the atom pass and the bond pass use the same main-world entity (which
                // they should), neither one renders. Likewise, if the main-world entity isn't used
                // for either, then we likewise get a blank screen. ONE of the two must use the
                // main-world entity, and the other must use a different value. In the draw code we
                // arbitrarily pick the render-world entity ID, which should be meaningless in the
                // main-world. This works, but the whole thing smells like an upstream bug.
                main_entity: match atoms.main_entity {
                    Some(_) => None,
                    None => Some(entity.into()),
                },
            }
        } else {
            ExtractedBonds {
                transform,
                vdw_scale: config.vdw_scale,
                bonds: Vec::new(),
                main_entity: None,
            }
        };

        // Return extracted components
        Some((atoms, bonds))
    }
}

// End of File
