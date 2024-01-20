// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
#[allow(unused_imports)]
pub use self::android::*;

#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
#[allow(unused_imports)]
pub use self::ios::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use self::linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

#[cfg(target_family = "wasm")]
mod web;
#[cfg(target_family = "wasm")]
pub use self::web::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
#[allow(unused_imports)]
pub use self::windows::*;

// End of File
