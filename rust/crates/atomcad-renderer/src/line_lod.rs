//! Grid level of detail (`doc/design_screen_space_lines.md` D9).
//!
//! A grid registers its line families as [`LodSlot`]s in its `LineMesh`, and
//! tags each line with the slot it belongs to. Per draw, [`slot_lod`] decides
//! for every slot whether its lines are shown at all and between which two eye
//! distances they fade out. The result goes to the GPU as a `LineLodUniform`
//! ([`lod_uniform_entries`]); the shader knows nothing about grids, only those
//! two numbers per slot.
//!
//! Every function here is pure, so it can be tested with a real `Camera` and
//! no device.

use crate::camera::Camera;
use glam::f32::Vec3;

/// Entries in a `LineLodUniform`: slot ids `0..LINE_LOD_SLOTS`, id 0 being
/// "no slot", so a mesh can register up to `LINE_LOD_SLOTS - 1` slots.
pub const LINE_LOD_SLOTS: usize = 32;

/// A slot whose densest lines are closer than this on screen (logical pixels)
/// even where they are farthest apart is hidden.
pub const T_DROP_PX: f32 = 4.0;
/// A visible slot starts fading where its lines across the view come this
/// close on screen (logical pixels)…
pub const T_FADE_START_PX: f32 = 4.0;
/// …and is gone where they come this close.
pub const T_FADE_END_PX: f32 = 1.5;
/// A hidden slot comes back only at `HYSTERESIS × T_DROP_PX`, so an orbit
/// hovering at the threshold doesn't flicker.
pub const HYSTERESIS: f32 = 1.25;

/// Below this eye distance from its plane, a slot is seen edge-on and hidden.
const MIN_EYE_HEIGHT: f64 = 1e-6;

/// One family of grid lines that are equally spaced and in one plane: all the
/// minor lines of a grid, or all its major lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodSlot {
    pub plane_point: Vec3,
    /// Need not be normalised.
    pub plane_normal: Vec3,
    /// World distance between neighbouring lines of this level. For a
    /// parallelogram grid, the smaller of its two line distances.
    pub spacing: f32,
}

/// What one slot does in one draw. "No fade" is `fade_end <= fade_start`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotLod {
    pub visible: bool,
    /// Eye distances (world units) between which the slot fades out.
    pub fade_start: f32,
    pub fade_end: f32,
}

impl SlotLod {
    /// Visible, no fade: entry 0, and every entry of a mesh without slots.
    pub const ALWAYS: SlotLod = SlotLod {
        visible: true,
        fade_start: 0.0,
        fade_end: 0.0,
    };

    const HIDDEN: SlotLod = SlotLod {
        visible: false,
        fade_start: 0.0,
        fade_end: 0.0,
    };

    /// The `vec4(visible, fade_start, fade_end, 0)` the shader reads.
    pub fn to_gpu(self) -> [f32; 4] {
        [
            if self.visible { 1.0 } else { 0.0 },
            self.fade_start,
            self.fade_end,
            0.0,
        ]
    }
}

/// The level of detail of `slot` drawn by `camera` into a target of
/// `target_size` render pixels at `pixel_scale` (D9). All thresholds are in
/// logical pixels × `pixel_scale`, so a supersampled target makes the same
/// choices as the screen.
///
/// `was_visible` is the slot's visibility in the previous live frame, for the
/// hysteresis; `None` (a thumbnail, an export, a fresh mesh) takes the plain
/// threshold.
pub fn slot_lod(
    slot: &LodSlot,
    camera: &Camera,
    target_size: (u32, u32),
    pixel_scale: f32,
    was_visible: Option<bool>,
) -> SlotLod {
    let Some(normal) = slot.plane_normal.as_dvec3().try_normalize() else {
        return SlotLod::ALWAYS;
    };
    let spacing = slot.spacing as f64;
    let target_height = target_size.1 as f64;
    let scale = pixel_scale as f64;
    let drop_px = T_DROP_PX as f64
        * scale
        * if was_visible == Some(false) {
            HYSTERESIS as f64
        } else {
            1.0
        };

    if camera.orthographic {
        // Render pixels per world unit, and how far the plane faces the view.
        let k = target_height / (2.0 * camera.ortho_half_height);
        let Some(view_dir) = (camera.target - camera.eye).try_normalize() else {
            return SlotLod::HIDDEN;
        };
        let c = view_dir.dot(normal).abs();
        // No distance fade: eye distance means nothing in orthographic.
        return if spacing * k * c >= drop_px {
            SlotLod::ALWAYS
        } else {
            SlotLod::HIDDEN
        };
    }

    let h = (camera.eye - slot.plane_point.as_dvec3()).dot(normal).abs();
    if h < MIN_EYE_HEIGHT {
        return SlotLod::HIDDEN;
    }
    // Focal length in render pixels.
    let f = target_height / 2.0 / (camera.fovy / 2.0).tan();
    // Lines are farthest apart on screen straight below the eye (D = h).
    if spacing * f / h < drop_px {
        return SlotLod::HIDDEN;
    }
    // Where the across-view spacing `s·f·h / D²` falls to each threshold.
    let fade_at = |threshold_px: f32| (spacing * f * h / (threshold_px as f64 * scale)).sqrt();
    SlotLod {
        visible: true,
        fade_start: fade_at(T_FADE_START_PX) as f32,
        fade_end: fade_at(T_FADE_END_PX) as f32,
    }
}

/// A mesh's whole `LineLodUniform`: entry 0 and every entry past `slots` are
/// [`SlotLod::ALWAYS`], entry `i + 1` is `slots[i]`'s [`slot_lod`].
///
/// With `hysteresis` (the live viewport only), each slot's previous
/// visibility is read from it and its new one written back; it is resized to
/// `slots` first.
pub fn lod_uniform_entries(
    slots: &[LodSlot],
    camera: &Camera,
    target_size: (u32, u32),
    pixel_scale: f32,
    mut hysteresis: Option<&mut Vec<Option<bool>>>,
) -> [[f32; 4]; LINE_LOD_SLOTS] {
    let mut entries = [SlotLod::ALWAYS.to_gpu(); LINE_LOD_SLOTS];
    if let Some(state) = hysteresis.as_deref_mut() {
        state.resize(slots.len(), None);
    }
    for (i, slot) in slots.iter().take(LINE_LOD_SLOTS - 1).enumerate() {
        let was_visible = hysteresis.as_deref().and_then(|state| state[i]);
        let lod = slot_lod(slot, camera, target_size, pixel_scale, was_visible);
        if let Some(state) = hysteresis.as_deref_mut() {
            state[i] = Some(lod.visible);
        }
        entries[i + 1] = lod.to_gpu();
    }
    entries
}
