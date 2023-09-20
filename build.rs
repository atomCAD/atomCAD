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
}

// End of File
