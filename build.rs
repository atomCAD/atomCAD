// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

extern crate embed_resource;
use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();
    if target.contains("windows") {
        // on windows we will set our app icon as icon for the executable
        embed_resource::compile("build/windows/icon.rc");
    }
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let profile = env::var("PROFILE").unwrap();
    if !target_arch.contains("wasm") && profile.contains("debug") {
        // Use dynamic linking for faster recompilation
        println!("cargo:rustc-cfg=feature=\"bevy/dynamic_linking\"");
        // Enable bevy's asset hot-reloading capability
        println!("cargo:rustc-cfg=feature=\"bevy/file_watcher\"");
    }
}

// End of File
