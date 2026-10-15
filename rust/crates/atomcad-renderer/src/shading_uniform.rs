//! The per-pass shading uniform and the settings it is built from
//! (`doc/design_depth_perception.md` D2, D3).

use crate::camera::Camera;
use glam::f64::DVec3;

/// The light rig (D3). The preference is the preset, not the lights: the
/// rig's constants are named constants here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightingPreset {
    /// The look before the studio rig: one key light 20° above the view
    /// direction (`Camera::calc_headlight_direction`), no fill, a constant
    /// ambient.
    #[default]
    Headlight,
}

/// The *Headlight* preset's key intensity.
pub const HEADLIGHT_KEY_INTENSITY: f32 = 2.0;

/// The *Headlight* preset's ambient, the same from every side
/// (sky = ground).
pub const HEADLIGHT_AMBIENT: f32 = 0.2;

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
/// | *size* | 80 |
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
    /// reaches full strength (D5). Filled in by Phase 3; 0 until then.
    pub depth_cue_front: f32,
    pub depth_cue_span: f32,
    pub depth_cue_strength: f32,
    pub ao_strength: f32,
}

impl ShadingUniform {
    /// The uniform for drawing `camera`'s view of content with
    /// `content_bounds` (`None`: no content) on `background_rgb`, shaded by
    /// `settings`, in `pass`. Pure, so it is tested without a device; the
    /// renderer writes its result before every draw.
    ///
    /// The gadget pass and a scene without content have no depth cue.
    pub fn new(
        camera: &Camera,
        content_bounds: Option<(DVec3, DVec3)>,
        background_rgb: [u8; 3],
        settings: &ShadingSettings,
        pass: ShadingPass,
    ) -> Self {
        let head_light_dir = camera.calc_headlight_direction().as_vec3().to_array();
        let (key_light_dir, key_intensity, fill_light_dir, fill_intensity, sky, ground) =
            match settings.preset {
                // The fill is off; its direction only has to be a unit
                // vector.
                LightingPreset::Headlight => (
                    head_light_dir,
                    HEADLIGHT_KEY_INTENSITY,
                    head_light_dir,
                    0.0,
                    HEADLIGHT_AMBIENT,
                    HEADLIGHT_AMBIENT,
                ),
            };
        let depth_cue_strength = match (pass, content_bounds) {
            (ShadingPass::Content, Some(_)) => settings.depth_cue_strength,
            _ => 0.0,
        };
        Self {
            key_light_dir,
            key_intensity,
            fill_light_dir,
            fill_intensity,
            hemisphere_up: camera_up(camera).as_vec3().to_array(),
            sky,
            background: background_rgb.map(|c| c as f32 / 255.0),
            ground,
            depth_cue_front: 0.0,
            depth_cue_span: 0.0,
            depth_cue_strength,
            ao_strength: settings.ao_strength,
        }
    }
}

/// The camera's up axis as the view matrix has it: the look-at basis's
/// second row, perpendicular to the view direction.
fn camera_up(camera: &Camera) -> DVec3 {
    let forward = (camera.target - camera.eye).normalize();
    let right = forward.cross(camera.up).normalize();
    right.cross(forward)
}
