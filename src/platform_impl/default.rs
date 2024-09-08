// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

use std::ffi::{OsStr, OsString};

pub fn get_process_name() -> OsString {
    // Option 1: Try std::env::args()
    if let Some(arg0) = std::env::args().next() {
        if let Some(name) = std::path::PathBuf::from(arg0).file_name() {
            return name.to_owned();
        }
    }

    // Option 2: Using std::env::current_exe()
    if let Ok(path) = std::env::current_exe() {
        if let Some(name) = path.file_name() {
            return name.to_owned();
        }
    }

    // Fallback: Use the crate name in Cargo.toml
    OsString::from(env!("CARGO_PKG_NAME"))
}

pub fn set_process_name(name: &OsStr) {
    let _ = name;
}

// End of File
