// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::pdb::PdbAsset;
use bevy::asset::LoadState;
use bevy::prelude::*;

/// A trait for asset libraries that can be loaded and checked
pub trait AssetLibrary: Resource {
    /// Load all assets in this library
    fn load(asset_server: &AssetServer) -> Self;

    /// Check if all assets in this library are loaded
    fn all_loaded(&self, asset_server: &AssetServer) -> bool;
}

/// Resource that holds all font assets for the application
#[derive(Resource, Default)]
pub struct FontAssetHandles {
    pub fira_sans_bold: Handle<Font>,
    pub fira_sans_regular: Handle<Font>,
}

impl AssetLibrary for FontAssetHandles {
    fn load(asset_server: &AssetServer) -> Self {
        FontAssetHandles {
            fira_sans_bold: asset_server.load("fonts/FiraSans-Bold.ttf"),
            fira_sans_regular: asset_server.load("fonts/FiraSans-Regular.ttf"),
        }
    }

    fn all_loaded(&self, asset_server: &AssetServer) -> bool {
        let handles = [&self.fira_sans_bold, &self.fira_sans_regular];

        handles.iter().all(|handle| {
            matches!(
                asset_server.get_load_state(*handle),
                Some(LoadState::Loaded)
            )
        })
    }
}

/// Resource that holds all PDB assets for the application
#[derive(Resource, Default)]
pub struct PdbAssetHandles {
    pub neon_pump_imm: Handle<PdbAsset>,
}

impl AssetLibrary for PdbAssetHandles {
    fn load(asset_server: &AssetServer) -> Self {
        PdbAssetHandles {
            neon_pump_imm: asset_server.load("pdb/neon_pump_imm.pdb"),
        }
    }

    fn all_loaded(&self, asset_server: &AssetServer) -> bool {
        let handles = [&self.neon_pump_imm];

        handles.iter().all(|handle| {
            matches!(
                asset_server.get_load_state(*handle),
                Some(LoadState::Loaded)
            )
        })
    }
}

/// Generic system to load assets using the AssetLibrary trait
pub fn load_assets<T: AssetLibrary>(mut commands: Commands, asset_server: Res<AssetServer>) {
    let assets = T::load(&asset_server);
    commands.insert_resource(assets);
}

// End of File
