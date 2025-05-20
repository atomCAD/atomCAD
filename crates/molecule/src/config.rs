// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::prelude::*;

/// Configuration component for molecule rendering.
///
/// This component allows customization of how molecules are rendered in the scene.
/// Attach this component to an entity with a [`Molecule`] component to control
/// its visual representation.
#[derive(Component)]
pub struct MoleculeRenderConfig {
    /// Scale factor for van der Waals radii.
    ///
    /// Values greater than 1.0 will make atoms appear larger,
    /// while values less than 1.0 will make them appear smaller.
    /// A value of 1.0 represents the standard vdW radius.
    pub vdw_scale: f32,

    /// Controls whether atoms are rendered.
    ///
    /// When `true`, atoms will be visible as spheres colored
    /// according to their element type.
    pub show_atoms: bool,

    /// Controls whether bonds are rendered.
    ///
    /// When `true`, bonds between atoms will be visible as
    /// cylindrical connections.
    pub show_bonds: bool,
}

impl Default for MoleculeRenderConfig {
    /// Defaults to space-filling model using van der Waals radii.
    fn default() -> Self {
        Self {
            vdw_scale: 1.0,
            show_atoms: true,
            show_bonds: false,
        }
    }
}

// End of File
