// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

// Bevy uses some very complex types for specifying system inputs.
// There's just no getting around this, so silence clippy's protestations.
#![allow(clippy::too_many_arguments)]

mod assets;
mod buffers;
mod components;
mod config;
mod draw;
mod extract;
mod pipelines;
mod plugin;
mod uniforms;

pub use components::{AtomInstance, BondInstance, Molecule};
pub use plugin::MoleculeRenderPlugin;

// End of File
