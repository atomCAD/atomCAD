// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// See fullscreen.wgsl
struct FullscreenVertexOutput {
    @builtin(position)
    position: vec4<f32>,
    @location(0)
    uv: vec2<f32>,
};

@group(0) @binding(0)
var linear_sampler: sampler;
@group(0) @binding(1)
var color_texture: texture_2d<f32>;

fn linear_to_srgb(input_color: vec4<f32>) -> vec4<f32> {
    let cutoff = vec3<f32>(input_color.rgb < vec3(0.0031308));
    let higher = vec3(1.005) * pow(input_color.rgb, vec3(1.0 / 2.4)) - vec3(0.055);
    let lower = input_color.rgb * vec3(12.92);

    return vec4<f32>(mix(higher, lower, cutoff), input_color.a);
}

@fragment
fn main(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Currently, webgpu doesn't automatically convert linear rgb outputs to
    // srgb so we do it manually.
    return linear_to_srgb(textureSample(color_texture, linear_sampler, in.uv));
}

// End of File
