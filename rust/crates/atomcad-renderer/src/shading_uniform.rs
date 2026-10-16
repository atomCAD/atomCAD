//! The per-pass shading uniform and the settings it is built from
//! (`doc/design_depth_perception.md` D2, D3).

use crate::camera::Camera;
use crate::shading::AO_KEY;
use crate::shadow_fit::ShadowSetup;
use glam::f64::DVec3;

/// The light rig (D3). The preference is the preset, not the lights: the
/// rig's constants are named constants here.
///
/// The `Default` is the renderer's neutral *Headlight* (D2); the user's
/// default, *Studio*, lives in the preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightingPreset {
    /// The look before the studio rig: one key light 20° above the view
    /// direction (`Camera::calc_headlight_direction`), no fill, a constant
    /// ambient.
    #[default]
    Headlight,
    /// A camera-relative studio rig: a key light from the upper left, a
    /// diffuse-only fill from the lower right and a hemisphere ambient,
    /// brighter from above.
    Studio,
}

/// The *Headlight* preset's key intensity.
pub const HEADLIGHT_KEY_INTENSITY: f32 = 2.0;

/// The *Headlight* preset's ambient, the same from every side
/// (sky = ground).
pub const HEADLIGHT_AMBIENT: f32 = 0.2;

/// Where the *Studio* key light sits relative to the view direction, in
/// degrees: above it, and to its left (D3).
pub const STUDIO_KEY_ELEVATION_DEG: f64 = 35.0;
pub const STUDIO_KEY_AZIMUTH_DEG: f64 = 30.0;
/// The *Studio* key light's intensity.
pub const STUDIO_KEY_INTENSITY: f32 = 2.2;

/// Where the *Studio* fill light sits, in the same convention as the key:
/// below the view direction and to its right, so both angles are negative.
pub const STUDIO_FILL_ELEVATION_DEG: f64 = -20.0;
pub const STUDIO_FILL_AZIMUTH_DEG: f64 = -40.0;
/// The *Studio* fill light's intensity. It is diffuse only.
pub const STUDIO_FILL_INTENSITY: f32 = 0.6;

/// The *Studio* hemisphere ambient: from straight above the view (the sky)
/// and from straight below it (the ground).
pub const STUDIO_SKY: f32 = 0.26;
pub const STUDIO_GROUND: f32 = 0.12;

/// How the content is shaded: the renderer-side image of the *Lighting*
/// preferences (D8).
///
/// `Default` is the renderer's **neutral** setting, today's look: *Headlight*,
/// no depth cue, no ambient occlusion (D2). The user's defaults live in the
/// preferences, so every renderer test that sets nothing sees the old image.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadingSettings {
    pub preset: LightingPreset,
    /// 0 is off (D5).
    pub depth_cue_strength: f32,
    /// 0 is off (D7).
    pub ao_strength: f32,
    /// How much of the key a fully shadowed surface loses
    /// (`doc/design_shadows.md` D1); 0 is off. Shadows are drawn only under
    /// *Studio*.
    pub shadow_strength: f32,
}

/// Which render pass a [`ShadingUniform`] is for (D2). The gadget pass draws
/// editing aids (gizmo, guide dots), which never fade with depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadingPass {
    Content,
    Gadget,
}

/// The shading uniform at group 0, binding 1 of every lit shader (D2):
/// one buffer per pass, written by `draw_to_target` on every draw.
///
/// `lighting.wgsl` declares the same struct, and `shader_test.rs` checks the
/// offsets naga computes for it in each lit shader against `offset_of!` here.
/// Every `vec3<f32>` is followed by an `f32` that fills its 4-byte tail, so
/// there is no padding.
///
/// | Field | Offset |
/// |---|---|
/// | `key_light_dir`, `key_intensity` | 0, 12 |
/// | `fill_light_dir`, `fill_intensity` | 16, 28 |
/// | `hemisphere_up`, `sky` | 32, 44 |
/// | `background`, `ground` | 48, 60 |
/// | `depth_cue_front`, `depth_cue_span` | 64, 68 |
/// | `depth_cue_strength`, `ao_strength` | 72, 76 |
/// | `shadow_strength`, `ao_key` | 80, 84 |
/// | *size* | 96 |
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadingUniform {
    /// The direction the key light travels, world space, like
    /// `CameraUniform::head_light_dir`.
    pub key_light_dir: [f32; 3],
    pub key_intensity: f32,
    /// The direction the fill light travels. Always a unit vector, even with
    /// the fill off: the shader normalizes it.
    pub fill_light_dir: [f32; 3],
    pub fill_intensity: f32,
    /// The camera's up axis: the sky side of the hemisphere ambient.
    pub hemisphere_up: [f32; 3],
    pub sky: f32,
    /// The pass's clear colour in display space, 0–1 per channel: what the
    /// depth cue fogs towards (D5).
    pub background: [f32; 3],
    pub ground: f32,
    /// The view depth where the depth cue starts, and the depth over which it
    /// reaches full strength (D5): see [`depth_cue_range`]. The span is
    /// always positive, so the shader can divide by it.
    pub depth_cue_front: f32,
    pub depth_cue_span: f32,
    /// 0 is no cue: the gadget pass, a scene without content, or the cue
    /// turned off.
    pub depth_cue_strength: f32,
    pub ao_strength: f32,
    /// How much of the key a fully shadowed surface loses
    /// (`doc/design_shadows.md` D1, D8). 0 is no shadow: the gadget pass, or
    /// no shadow setup for the draw (*Headlight*, no casters, shadows off).
    pub shadow_strength: f32,
    /// AO's share of the key (`doc/design_shadows.md` D10), what used to be
    /// the shader constant `AO_KEY`.
    pub ao_key: f32,
    /// WGSL rounds the struct up to its 16-byte alignment.
    pub _padding: [f32; 2],
}

impl ShadingUniform {
    /// The uniform for drawing `camera`'s view of content with
    /// `content_bounds` (`None`: no content) on `background_rgb`, shaded by
    /// `settings`, in `pass`, with the draw's shadow setup (`None`: no shadow
    /// is drawn, `doc/design_shadows.md` D8). Pure, so it is tested without
    /// a device; the renderer writes its result before every draw.
    ///
    /// The gadget pass and a scene without content have no depth cue; the
    /// gadget pass and a draw without a setup have no shadow.
    pub fn new(
        camera: &Camera,
        content_bounds: Option<(DVec3, DVec3)>,
        background_rgb: [u8; 3],
        settings: &ShadingSettings,
        pass: ShadingPass,
        shadow: Option<&ShadowSetup>,
    ) -> Self {
        let key_light_dir = key_light_direction(camera, settings.preset)
            .as_vec3()
            .to_array();
        let (_, _, up) = camera_basis(camera);
        let (key_intensity, fill_light_dir, fill_intensity, sky, ground) = match settings.preset {
            // The fill is off; its direction only has to be a unit vector.
            LightingPreset::Headlight => (
                HEADLIGHT_KEY_INTENSITY,
                key_light_dir,
                0.0,
                HEADLIGHT_AMBIENT,
                HEADLIGHT_AMBIENT,
            ),
            LightingPreset::Studio => (
                STUDIO_KEY_INTENSITY,
                light_travel_direction(camera, STUDIO_FILL_ELEVATION_DEG, STUDIO_FILL_AZIMUTH_DEG),
                STUDIO_FILL_INTENSITY,
                STUDIO_SKY,
                STUDIO_GROUND,
            ),
        };
        // The range is the same in both passes; only the strength tells the
        // gadget pass apart.
        let range = content_bounds.and_then(|bounds| depth_cue_range(camera, bounds));
        let (depth_cue_front, depth_cue_span) =
            range.map_or((0.0, 1.0), |(front, span)| (front as f32, span as f32));
        let depth_cue_strength = match (pass, range) {
            (ShadingPass::Content, Some(_)) => settings.depth_cue_strength,
            _ => 0.0,
        };
        let shadow_strength = match (pass, shadow) {
            (ShadingPass::Content, Some(_)) => settings.shadow_strength,
            _ => 0.0,
        };
        Self {
            key_light_dir,
            key_intensity,
            fill_light_dir,
            fill_intensity,
            hemisphere_up: up.as_vec3().to_array(),
            sky,
            background: background_rgb.map(|c| c as f32 / 255.0),
            ground,
            depth_cue_front,
            depth_cue_span,
            depth_cue_strength,
            ao_strength: settings.ao_strength,
            shadow_strength,
            // The coupling with the shadows (D10) comes with the preference.
            ao_key: AO_KEY,
            _padding: [0.0; 2],
        }
    }
}

/// The direction the key light travels under `preset`, world space: *the*
/// key direction, which the shading uniform and the shadow fit both take
/// (`doc/design_shadows.md` D5). A map built along a second derivation of it
/// would put shadows where `N·L` says there is light.
pub fn key_light_direction(camera: &Camera, preset: LightingPreset) -> DVec3 {
    match preset {
        LightingPreset::Headlight => camera.calc_headlight_direction(),
        LightingPreset::Studio => {
            light_travel_dir(camera, STUDIO_KEY_ELEVATION_DEG, STUDIO_KEY_AZIMUTH_DEG)
        }
    }
}

/// The camera basis as the view matrix has it, `(forward, right, up)`: `up`
/// is perpendicular to the view direction.
pub(crate) fn camera_basis(camera: &Camera) -> (DVec3, DVec3, DVec3) {
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    (forward, right, up)
}

/// The view depths of the nearest and the farthest point of the AABB
/// `bounds` along `camera`'s view axis, `(front, back)`: its support points,
/// unclamped. Shared by the depth cue and the shadow fit
/// (`doc/design_shadows.md` D6).
pub(crate) fn view_depth_extent(camera: &Camera, bounds: (DVec3, DVec3)) -> (f64, f64) {
    let (min, max) = bounds;
    let centre = 0.5 * (min + max);
    let half = 0.5 * (max - min);
    let axis = (camera.target - camera.eye).normalize();
    let centre_depth = (centre - camera.eye).dot(axis);
    let support = half.dot(axis.abs());
    (centre_depth - support, centre_depth + support)
}

/// Where the depth cue starts and the depth over which it reaches full
/// strength, `(front, span)`, for content with the AABB `bounds` seen by
/// `camera` (D5). Depths are view-space distances along the view axis, in
/// both projections.
///
/// `front` is the depth of the AABB's nearest point along the view axis (its
/// support point), clamped to the near plane, so the nearest content is never
/// fogged whatever its shape. `span` is the diameter of the AABB's bounding
/// sphere, fixed while the camera orbits, so the cue's gradient (fog per Å)
/// does not change with the orientation. `None` for an AABB without extent:
/// a cue over a span of 0 would be undefined.
pub fn depth_cue_range(camera: &Camera, bounds: (DVec3, DVec3)) -> Option<(f64, f64)> {
    let (min, max) = bounds;
    let span = 2.0 * (0.5 * (max - min)).length();
    if !span.is_finite() || span <= 0.0 {
        return None;
    }
    let (front, _) = view_depth_extent(camera, bounds);
    Some((front.max(camera.znear), span))
}

/// The direction a light travels when it sits `elevation_deg` above the view
/// direction and `azimuth_deg` to its left (negative: below, to the right),
/// as seen from the eye. Built from the camera basis, so the light turns
/// with the camera (D3). The travel direction is what
/// `CameraUniform::head_light_dir` holds too.
pub fn light_travel_direction(camera: &Camera, elevation_deg: f64, azimuth_deg: f64) -> [f32; 3] {
    light_travel_dir(camera, elevation_deg, azimuth_deg)
        .as_vec3()
        .to_array()
}

/// [`light_travel_direction`] in f64, before the cast.
fn light_travel_dir(camera: &Camera, elevation_deg: f64, azimuth_deg: f64) -> DVec3 {
    let (forward, right, up) = camera_basis(camera);
    let (elevation, azimuth) = (elevation_deg.to_radians(), azimuth_deg.to_radians());
    // Towards the light: back along the view, up by the elevation, left by
    // the azimuth. The light travels the other way.
    let towards_light =
        elevation.cos() * (-azimuth.cos() * forward - azimuth.sin() * right) + elevation.sin() * up;
    (-towards_light).normalize()
}
