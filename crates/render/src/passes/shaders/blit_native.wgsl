// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// See fullscreen.wgsl
#import fullscreen.wgsl as Fullscreen

@group(0) @binding(0)
var color_texture: texture_2d<f32>;
@group(0) @binding(1)
var linear_sampler: sampler;

@fragment
fn blit(in: Fullscreen::VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(color_texture, linear_sampler, in.uv);
}

// End of File
