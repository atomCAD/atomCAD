// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

#[cfg(any(
    // FIXME: Should use the Android APIs to setup a hamburger menu for our “menubar.”
    target_os = "android",
    // FIXME: Should use the UiKit APIs to setup a hamburger menu for our “menubar.”
    target_os = "ios",
    // FIXME: Should use the gtk APIs to setup the menubar for the main window(s).
    target_os = "linux",
    // FIXME: We should investigate options for creating a menubar on web.
    target_family = "wasm",
    // FIXME: Should use the Win32 or WinUI APIs to setup the menubar for each window.
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
pub use self::default::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

// End of File
