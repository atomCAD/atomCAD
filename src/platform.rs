// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

//! Platform-specific code.

use std::ffi::{OsStr, OsString};
use std::sync::{LazyLock, RwLock};

use crate::platform_impl;

static PROCESS_NAME: LazyLock<RwLock<OsString>> =
    LazyLock::new(|| RwLock::new(platform_impl::get_process_name()));

/// Returns the name of the current process, as seen by the operating system.  This would be the
/// name of the running process as seen when running `ps` on UNIX, in the Activity Monitor on macOS,
/// or in the Task Manager on Windows.
///
/// ```rust
/// assert_eq!(
///     atomcad::platform::get_process_name(),
///     std::path::PathBuf::from(std::env::args().next().unwrap()).file_name().unwrap()
/// );
/// ```
pub fn get_process_name() -> OsString {
    PROCESS_NAME.read().expect("Poisoned RwLock").clone()
}

/// Changes the name of the current process, as seen by the operating system.  This typically
/// changes the name of the executable in the process list, so that it is no longer based on the
/// executable filename, and may also change the name of the application in the ”About” dialog box
/// on some platforms.
///
/// **Note:** On some platforms the process name is queried only once when specific features which
///           depend on it are setup.  If you want these features to see the updated name, then you
///           need to call `set_process_name` as early as possible in the initialization of your
///           application.  It is suggested to call this function as soon as possible in `main`.
///
/// ```rust
/// # use std::ffi::OsString;
/// # let old_name = atomcad::platform::get_process_name();
/// let name = OsString::from("My Application");
/// atomcad::platform::set_process_name(&name);
/// assert_eq!(atomcad::platform::get_process_name(), name);
/// # atomcad::platform::set_process_name(&old_name);
/// ```
pub fn set_process_name(name: &OsStr) {
    let mut process_name = PROCESS_NAME.write().expect("Poisoned RwLock");
    platform_impl::set_process_name(name);
    *process_name = name.to_owned();
}

// End of File
