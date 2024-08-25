// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

// Only web implements any platform-specific application initialization code, at this time.  All
// other platforms pull the default implementation, which provides stub plugins that don't do
// anything.

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod desktop;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) use self::desktop::*;

#[cfg(any(target_os = "android", target_os = "ios", target_family = "wasm"))]
mod default;
#[cfg(any(target_os = "android", target_os = "ios", target_family = "wasm"))]
pub(crate) use self::default::*;

// End of File
