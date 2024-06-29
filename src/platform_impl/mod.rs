// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

/// The `defaults` module provides default implementations/stubs for platform specific code.
/// Platforms which do not need customization for specific features can simply re-export from this
/// module.
mod defaults;
pub(crate) use defaults::*;

// End of File
