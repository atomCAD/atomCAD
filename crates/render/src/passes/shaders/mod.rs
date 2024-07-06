// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[include_wgsl_oil::include_wgsl_oil("atom.wgsl")]
pub mod atom {}

#[include_wgsl_oil::include_wgsl_oil("bond.wgsl")]
pub mod bond {}

pub mod blit {
    #[include_wgsl_oil::include_wgsl_oil("blit-native.wgsl")]
    pub mod native {}

    #[include_wgsl_oil::include_wgsl_oil("blit-web.wgsl")]
    pub mod web {}
}

#[include_wgsl_oil::include_wgsl_oil("fullscreen.wgsl")]
pub mod fullscreen {}

#[include_wgsl_oil::include_wgsl_oil("fxaa.wgsl")]
pub mod fxaa {}

// End of File
