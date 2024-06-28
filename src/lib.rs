// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

// Bevy uses some very complex types for specifying system inputs.
// There's just no getting around this, so silence clippy's protestations.
#![allow(clippy::type_complexity)]

mod app;
pub use app::AppPlugin;

mod start;
pub use start::start;

// End of File
