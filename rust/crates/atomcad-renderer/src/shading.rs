//! Rust twins of the shared lighting module, `lighting.wgsl`
//! (`doc/design_depth_perception.md` Testing layer 3).
//!
//! They tune the rig's constants against the D3 calibration constraint and
//! give the GPU tests their expected pixel values. **A change to
//! `lighting.wgsl` is mirrored here**; the GPU harness's twin-agreement test
//! is what catches a twin that drifted.

use crate::camera::Camera;
use crate::shading_uniform::ShadingUniform;
use glam::f32::{Vec3, Vec4};
use glam::f64::DVec3;

const PI: f32 = std::f32::consts::PI;

/// The rim band's width in output pixels (D4). Twin of `RIM_WIDTH_PX`.
pub const RIM_WIDTH_PX: f32 = 3.0;

/// The direction from a surface point towards the viewer: the camera's
/// backward axis in orthographic mode, towards the eye in perspective.
/// Twin of `camera_view_vector`.
pub fn view_vector(camera: &Camera, world_position: DVec3) -> Vec3 {
    if camera.orthographic {
        (camera.eye - camera.target).normalize().as_vec3()
    } else {
        (camera.eye - world_position).normalize().as_vec3()
    }
}

fn fresnel_schlick(cos_theta: f32, f0: Vec3) -> Vec3 {
    f0 + (Vec3::ONE - f0) * (1.0 - cos_theta).powf(5.0)
}

fn distribution_ggx(n: Vec3, h: Vec3, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let n_dot_h = n.dot(h).max(0.0);
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (PI * denom * denom)
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    n_dot_v / (n_dot_v * (1.0 - k) + k)
}

fn geometry_smith(n: Vec3, v: Vec3, l: Vec3, roughness: f32) -> f32 {
    geometry_schlick_ggx(n.dot(v).max(0.0), roughness)
        * geometry_schlick_ggx(n.dot(l).max(0.0), roughness)
}

/// What a fully buried surface keeps of the light AO darkens (D7).
/// Twin of `AO_FLOOR`.
pub const AO_FLOOR: f32 = 0.25;
/// The visibility above which AO takes nothing. Twin of `AO_OPEN`.
pub const AO_OPEN: f32 = 0.6;
/// The key light's share of the AO darkening, without shadows
/// (`doc/design_shadows.md` D10). The shader reads it from
/// `ShadingUniform::ao_key`.
pub const AO_KEY: f32 = 0.35;
/// The key light's share of the AO darkening at full shadow strength
/// (`doc/design_shadows.md` D10): the shadows darken the creases the key
/// does not reach, so AO cedes most of the key to them.
pub const AO_KEY_WITH_SHADOWS: f32 = 0.1;

/// AO's share of the key under the uniform's `shadow_strength` (D10):
/// exactly `AO_KEY` wherever no shadow is drawn.
pub fn ao_key(shadow_strength: f32) -> f32 {
    AO_KEY * (1.0 - shadow_strength) + AO_KEY_WITH_SHADOWS * shadow_strength
}

/// The packed exposure `(4b, E)` of a surface nothing occludes, as the
/// shader reads it. Twin of `FULLY_EXPOSED`.
pub const FULLY_EXPOSED: Vec4 = Vec4::new(0.0, 0.0, 0.0, 1.0);

/// The packed `Snorm8x4` exposure as the GPU decodes it: each component
/// over 127, at least −1.
pub fn decode_exposure(packed: [i8; 4]) -> Vec4 {
    Vec4::from_array(packed.map(|c| (c as f32 / 127.0).max(-1.0)))
}

/// The cosine-weighted visible fraction of the sky around `normal`, from the
/// packed `(4b, E)` (D10). Twin of `ao_visibility`.
pub fn ao_visibility(exposure: Vec4, normal: Vec3) -> f32 {
    (exposure.w + 0.5 * exposure.truncate().dot(normal)).clamp(0.0, 1.0)
}

/// The factors burial applies to the fill and hemisphere (`.0`) and to the
/// key (`.1`) under `ao_strength`, the key taking `ao_key` of it (D7;
/// `doc/design_shadows.md` D10). Twin of `ao_factors`.
pub fn ao_factors(ao_strength: f32, ao_key: f32, exposure: Vec4, normal: Vec3) -> (f32, f32) {
    if ao_strength <= 0.0 {
        return (1.0, 1.0);
    }
    let ao =
        AO_FLOOR + (1.0 - AO_FLOOR) * smoothstep(0.0, AO_OPEN, ao_visibility(exposure, normal));
    (
        1.0 + (ao - 1.0) * ao_strength,
        1.0 + (ao - 1.0) * ao_strength * ao_key,
    )
}

/// What reaches a surface of the key light, from the shadow lookup's `v`
/// (1 fully lit, 0 in full shadow) and the uniform's `shadow_strength`
/// (`doc/design_shadows.md` D1): exactly 1 at strength 0, whatever `v`.
/// Twin of `key_visibility`.
pub fn key_visibility(shadow_strength: f32, v: f32) -> f32 {
    1.0 - shadow_strength * (1.0 - v)
}

/// The linear (HDR) colour of a surface point with normal `normal`, seen
/// from direction `view` (towards the eye, see [`view_vector`]), under the
/// rig in `shading` (D3), fully exposed and unshadowed. Twin of
/// `light_surface` with `FULLY_EXPOSED` and a `key_visibility` of 1.
pub fn light_surface(
    shading: &ShadingUniform,
    view: Vec3,
    normal: Vec3,
    albedo: Vec3,
    roughness: f32,
    metallic: f32,
) -> Vec3 {
    light_surface_exposed(
        shading,
        view,
        normal,
        albedo,
        roughness,
        metallic,
        FULLY_EXPOSED,
        1.0,
    )
}

/// [`light_surface`] with a packed exposure `(4b, E)` (D7, D10), as the
/// vertex delivers it (see [`decode_exposure`]), and the share of the key
/// that reaches the surface, `key_visibility` (`doc/design_shadows.md` D1,
/// see [`key_visibility`]). Twin of `light_surface`.
#[allow(clippy::too_many_arguments)]
pub fn light_surface_exposed(
    shading: &ShadingUniform,
    view: Vec3,
    normal: Vec3,
    albedo: Vec3,
    roughness: f32,
    metallic: f32,
    exposure: Vec4,
    key_visibility: f32,
) -> Vec3 {
    let v = view;
    let n = normal.normalize();
    let l = -Vec3::from_array(shading.key_light_dir).normalize();
    let h = (v + l).normalize();

    let f0 = Vec3::splat(0.04).lerp(albedo, metallic);
    let f = fresnel_schlick(h.dot(v).max(0.0), f0);
    let d = distribution_ggx(n, h, roughness);
    let g = geometry_smith(n, v, l, roughness);
    let denominator = 4.0 * n.dot(v).max(0.0) * n.dot(l).max(0.0) + 0.001;
    let specular = d * g * f / denominator;
    let k_d = (Vec3::ONE - f) * (1.0 - metallic);
    let diffuse = k_d * albedo / PI;

    let (ambient_factor, key_factor) = ao_factors(shading.ao_strength, shading.ao_key, exposure, n);

    let n_dot_l = n.dot(l).max(0.0);
    let key = shading.key_intensity * key_factor * key_visibility * n_dot_l;

    let t = 0.5 + 0.5 * n.dot(Vec3::from_array(shading.hemisphere_up));
    let ambient_light = (shading.ground + (shading.sky - shading.ground) * t) * ambient_factor;
    let ambient_diffuse = ambient_light * albedo;
    let ambient_specular = ambient_light * fresnel_schlick(v.dot(n).max(0.0), f0);
    let ambient = ambient_diffuse.lerp(ambient_specular, metallic);

    let l_fill = -Vec3::from_array(shading.fill_light_dir).normalize();
    let fill = shading.fill_intensity * ambient_factor * n.dot(l_fill).max(0.0) * diffuse;

    key * (diffuse + specular) + ambient + fill
}

/// Linear HDR colour to display colour, 0–1: Reinhard, then gamma 2.2.
/// Twin of `to_display`.
pub fn to_display(linear: Vec3) -> Vec3 {
    let mapped = linear / (linear + Vec3::ONE);
    mapped.powf(1.0 / 2.2)
}

/// A display colour fogged towards the pass's background by its view depth
/// (D5): untouched at `depth_cue_front`, mixed by `depth_cue_strength` a
/// span's depth behind it. `color` and the result are display colours, 0–1.
/// Twin of `apply_depth_cue`.
pub fn apply_depth_cue(
    shading: &ShadingUniform,
    camera: &Camera,
    color: Vec3,
    world_position: DVec3,
) -> Vec3 {
    if shading.depth_cue_strength <= 0.0 {
        return color;
    }
    let f = shading.depth_cue_strength * depth_cue_fraction(shading, camera, world_position);
    color.lerp(Vec3::from_array(shading.background), f)
}

/// How far into the depth cue's span `world_position` lies, 0–1, before the
/// strength is applied: 0 at the nearest content, 1 a span behind it.
pub fn depth_cue_fraction(shading: &ShadingUniform, camera: &Camera, world_position: DVec3) -> f32 {
    let forward = (camera.target - camera.eye).normalize();
    let depth = (world_position - camera.eye).dot(forward) as f32;
    ((depth - shading.depth_cue_front) / shading.depth_cue_span).clamp(0.0, 1.0)
}

/// How much of the rim colour a sphere's pixel takes (D4): `distance` is how
/// far the view ray passes outside the silhouette in world units (negative
/// inside), `pixel_world` the world size of a render pixel, `pixel_scale`
/// the target's render pixels per output pixel. Twin of `rim_weight`.
pub fn rim_weight(distance: f32, pixel_world: f32, radius: f32, pixel_scale: f32) -> f32 {
    let inside_px = -distance / pixel_world;
    let width_px = (RIM_WIDTH_PX * pixel_scale).min(0.35 * radius / pixel_world);
    1.0 - smoothstep(width_px - 0.5, width_px + 0.5, inside_px)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The luminance of a linear colour (Rec. 709 weights), for the D3
/// calibration constraint.
pub fn luminance(linear: Vec3) -> f32 {
    linear.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}
