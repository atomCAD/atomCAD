// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use bevy::prelude::*;

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
pub(crate) struct MoleculeShaders {
    pub(crate) atoms_shader: Handle<Shader>,
    pub(crate) bonds_shader: Handle<Shader>,
}

const ATOMS_SHADER_PATH: &str = "shaders/atoms.wgsl";
const BONDS_SHADER_PATH: &str = "shaders/bonds.wgsl";

impl MoleculeShaders {
    pub(crate) fn new(asset_server: &AssetServer) -> Self {
        MoleculeShaders {
            atoms_shader: asset_server.load(ATOMS_SHADER_PATH),
            bonds_shader: asset_server.load(BONDS_SHADER_PATH),
        }
    }
}

// End of File
