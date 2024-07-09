// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// See fullscreen.wgsl
#import fullscreen.wgsl as Fullscreen
#import blit_native.wgsl as blit_native

fn linear_to_srgb(input_color: vec4<f32>) -> vec4<f32> {
    let cutoff = vec3<f32>(input_color.rgb < vec3(0.0031308));
    let higher = vec3(1.005) * pow(input_color.rgb, vec3(1.0 / 2.4)) - vec3(0.055);
    let lower = input_color.rgb * vec3(12.92);

    return vec4<f32>(mix(higher, lower, cutoff), input_color.a);
}

@fragment
fn blit(in: Fullscreen::VertexOutput) -> @location(0) vec4<f32> {
    // Currently, webgpu doesn't automatically convert linear rgb outputs to
    // srgb so we do it manually.
    return linear_to_srgb(blit_native::blit(in));
}

// End of File
