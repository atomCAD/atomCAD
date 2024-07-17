// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

/// The `defaults` module provides default implementations/stubs for platform specific code.
/// Platforms which do not need customization for specific features can simply re-export from this
/// module.
mod defaults;
#[cfg(not(target_family = "wasm"))]
pub(crate) use defaults::*;

#[cfg(target_family = "wasm")]
mod web;
#[cfg(target_family = "wasm")]
pub(crate) use web::*;

// End of File
