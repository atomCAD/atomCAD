// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use objc2_foundation::{NSProcessInfo, NSString};
use std::ffi::{OsStr, OsString};

// No custom bevy code needed for macOS.
pub(crate) use super::defaults::tweak_bevy_app;

pub(crate) fn get_process_name() -> OsString {
    let process_info = NSProcessInfo::processInfo();
    let process_name = process_info.processName();
    process_name.to_string().into()
}

pub(crate) fn set_process_name(name: &OsStr) {
    let process_info = NSProcessInfo::processInfo();
    unsafe {
        process_info.setProcessName(&NSString::from_str(&name.to_string_lossy()));
    }
}

// End of File
