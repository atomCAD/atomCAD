// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub mod atom {
    pub const SOURCE: &str = include_str!("atom.wgsl");
}

pub mod bond {
    pub const SOURCE: &str = include_str!("bond.wgsl");
}

pub mod blit {
    pub mod native {
        pub const SOURCE: &str = include_str!("blit_native.wgsl");
    }

    pub mod srgb {
        pub const SOURCE: &str = include_str!("blit_srgb.wgsl");
    }
}

pub mod fullscreen {
    pub const SOURCE: &str = include_str!("fullscreen.wgsl");
}

pub mod fxaa {
    pub const SOURCE: &str = include_str!("fxaa.wgsl");
}

// End of File
