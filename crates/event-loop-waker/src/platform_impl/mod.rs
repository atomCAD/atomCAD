// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

#[cfg(not(target_family = "wasm"))]
mod desktop;
#[cfg(not(target_family = "wasm"))]
pub(crate) use desktop::*;

#[cfg(target_family = "wasm")]
mod defaults;
#[cfg(target_family = "wasm")]
pub(crate) use defaults::*;

// End of File
