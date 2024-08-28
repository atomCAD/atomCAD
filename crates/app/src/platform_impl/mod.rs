// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

// Only web implements any platform-specific application initialization code, at this time.  All
// other platforms pull the default implementation, which provides stub plugins that don't do
// anything.

#[cfg(target_family = "wasm")]
mod web;
#[cfg(target_family = "wasm")]
#[allow(unused_imports)]
pub use self::web::*;

#[cfg(not(target_family = "wasm"))]
mod default;
#[cfg(not(target_family = "wasm"))]
#[allow(unused_imports)]
pub use self::default::*;

// End of File
