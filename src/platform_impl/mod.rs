// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

#[cfg(any(
    target_os = "android",
    target_os = "ios",
    target_os = "linux",
    target_family = "wasm",
    target_os = "windows"
))]
mod default;
#[cfg(any(
    target_os = "android",
    target_os = "ios",
    target_os = "linux",
    target_family = "wasm",
    target_os = "windows"
))]
pub use default::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

// End of File
