// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

// Bevy uses some very complex types for specifying system inputs.
// There's just no getting around this, so silence clippy's protestations.
#![allow(clippy::type_complexity)]

mod app;
pub use app::{AppPlugin, AppState};

pub mod assets;
use assets::{FontAssets, PdbAssets};

mod atoms;
pub use atoms::{AtomCluster, AtomClusterPlugin, AtomInstance};

pub mod camera;
pub use camera::{CadCamera, CadCameraPlugin};

mod pdb;
pub use pdb::{PdbAsset, PdbLoaderPlugin};

pub mod platform;
pub(crate) mod platform_impl;
pub use platform::{bevy::PlatformTweaks, get_process_name, set_process_name};

mod start;
pub use start::start;

mod state;
pub use state::{CadViewPlugin, LoadingPlugin, SplashScreenPlugin};

pub const APP_NAME: &str = "atomCAD";

// End of File
