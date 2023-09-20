// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::AppState;
use crate::assets::{AssetLibrary, FontAssets, load_assets};
use bevy::app::App;
use bevy::prelude::*;

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app
            // Initialize asset loading when entering the Loading state
            .add_systems(OnEnter(AppState::Loading), load_assets::<FontAssets>)
            // Continuously check if assets are ready while in the Loading state
            .add_systems(
                Update,
                check_asset_loading.run_if(in_state(AppState::Loading)),
            );
    }
}

/// System to check if all required assets are loaded and transition states when ready
fn check_asset_loading(
    mut next_state: ResMut<NextState<AppState>>,
    asset_server: Res<AssetServer>,
    font_assets: Res<FontAssets>,
) {
    // Check if font assets are loaded
    let fonts_loaded = font_assets.all_loaded(&asset_server);

    // If all assets are loaded, transition to the CadView state
    if fonts_loaded {
        info!("Font assets loaded successfully, transitioning to SplashScreen state");
        next_state.set(AppState::SplashScreen);
    }
}

// End of File
