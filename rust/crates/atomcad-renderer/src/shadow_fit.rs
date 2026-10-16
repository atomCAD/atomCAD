//! The shadow fit (`doc/design_shadows.md` D5, D6, D8): where the key
//! light's shadow map looks from, what it covers, and the uniform the
//! receiver reads it through. Pure, so it is tested without a device; the
//! GPU side is `shadow_map.rs`.
//!
//! The casters' view-depth extent is cut into as many cascades as the view
//! needs (D6), usually one.

use crate::camera::Camera;
use crate::render_target::TargetSpec;
use crate::renderer::CameraUniform;
use crate::shading_uniform::{
    LightingPreset, ShadingSettings, camera_basis, key_light_direction, view_depth_extent,
};
use glam::f64::{DVec2, DVec3};

/// Side of each cascade of the shadow map, in texels (D9).
pub const SHADOW_MAP_SIZE: u32 = 2048;

/// The most cascades a view gets (D6), and the shadow map's layer count.
pub const MAX_CASCADES: usize = 4;

/// The PCF radius where the map can resolve it, in Å (D7): the penumbra is
/// this wide in every cascade and at every zoom.
pub const SHADOW_SOFTNESS_A: f64 = 0.15;

/// How many output pixels a shadow texel may cover where its cascade starts
/// (D6, `κ`).
pub const SHADOW_TEXELS_PER_PIXEL: f64 = 2.0;

/// The share of a cascade's depth range over which it blends into the next
/// one (D7).
pub const SHADOW_BLEND_FRACTION: f64 = 0.1;

/// The texel below which a finer cascade buys nothing (D6, `t_floor`): the
/// PCF radius is `max(SHADOW_SOFTNESS_A, 1.5·t)`, so every texel up to this
/// one gives the same penumbra. In Å.
pub const SHADOW_TEXEL_FLOOR: f64 = SHADOW_SOFTNESS_A / 1.5;

/// Bisection steps for a cascade's far split (D6): the split is then within
/// `2^-48` of the remaining depth extent of the deepest acceptable one.
const SPLIT_BISECTION_STEPS: u32 = 48;

/// The margin round a cascade's square beyond the filter radius, in texels
/// (D5 step 4): the receiver's normal offset (at most one texel,
/// `NORMAL_OFFSET_TEXELS` in `lighting.wgsl`), the hardware comparison's
/// bilinear footprint (one texel), and half a texel for snapping the square's
/// centre to the texel grid.
const SHADOW_MARGIN_TEXELS: f64 = 2.5;

/// The PCF radius for a cascade whose texel is `texel_world` Å: the
/// softness, or 1.5 texels where the cascade is too coarse to resolve it
/// (D7). In Å.
pub fn shadow_filter_radius(texel_world: f64) -> f64 {
    SHADOW_SOFTNESS_A.max(1.5 * texel_world)
}

/// The light's view of one frame (D5): its direction and up, and the
/// cascades. The fit's numbers rather than `Camera`s, so `PartialEq` is
/// derived (D9 compares setups) and nothing is cloned per frame;
/// [`ShadowSetup::light_camera`] builds the camera where one is needed.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowSetup {
    /// The direction the key light travels, `L`: exactly
    /// `key_light_direction`, normalized.
    pub light_dir: DVec3,
    /// The light's up, `u`: the camera's up made perpendicular to `L`.
    pub light_up: DVec3,
    /// 1..=[`MAX_CASCADES`] cascades, nearest first.
    pub cascades: Vec<ShadowCascadeFit>,
}

/// One cascade: a square of light space, the depth range along `L` it
/// stores, and the slice of view depth it serves (D5, D6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCascadeFit {
    /// The square's centre in light space `(x_L, y_L)`, snapped to the
    /// texel grid.
    pub centre_l: DVec2,
    /// The square's side `s`, quantised to quarter-octave steps, in Å.
    pub side: f64,
    /// Where the depth range starts along `L`, as `z_L`.
    pub z_near_l: f64,
    /// The depth range's length, in Å.
    pub depth_range: f64,
    /// The view depth where the cascade starts.
    pub split_near: f64,
    /// The view depth where it ends.
    pub split_far: f64,
    /// The texel's size `t = s / N`, in Å.
    pub texel_world: f64,
}

impl ShadowSetup {
    /// The light space's x axis, `L × u`: the light camera's right, so the
    /// texture's columns run along it.
    pub fn light_right(&self) -> DVec3 {
        self.light_dir.cross(self.light_up)
    }

    /// `p` in light space: `(x_L, y_L, z_L)`, measured from the world
    /// origin along `L × u`, `u` and `L` (D5).
    pub fn to_light_space(&self, p: DVec3) -> DVec3 {
        DVec3::new(
            p.dot(self.light_right()),
            p.dot(self.light_up),
            p.dot(self.light_dir),
        )
    }

    /// Cascade `k`'s light camera (D5): orthographic and square, looking
    /// along `L` with up `u`, its eye at the start of the depth range over
    /// the square's centre, clipping exactly the depth range.
    pub fn light_camera(&self, k: usize) -> Camera {
        let cascade = &self.cascades[k];
        let eye = self.light_right() * cascade.centre_l.x
            + self.light_up * cascade.centre_l.y
            + self.light_dir * cascade.z_near_l;
        Camera {
            eye,
            target: eye + self.light_dir,
            up: self.light_up,
            aspect: 1.0,
            // Unused by an orthographic camera.
            fovy: std::f64::consts::FRAC_PI_2,
            znear: 0.0,
            zfar: cascade.depth_range,
            orthographic: true,
            ortho_half_height: 0.5 * cascade.side,
            pivot_point: eye,
            nav_up: self.light_up,
            nav_up_label: String::new(),
        }
    }
}

/// The shadow cascades for drawing `camera`'s view of casters with
/// `caster_bounds` into a target of `target`, under `settings` (D5, D6);
/// `None` when no shadow is drawn: the strength is 0, the preset is
/// *Headlight*, there are no casters, or every caster is behind the camera
/// (or beyond its far plane). Pure, so it is tested without a device.
///
/// The casters' view-depth extent `[front, back]` is cut into slices, nearest
/// first, each as deep as it can be while its texel stays within
/// [`shadow_texel_limit`] of its near edge (D6).
pub fn shadow_setup(
    camera: &Camera,
    caster_bounds: Option<(DVec3, DVec3)>,
    settings: &ShadingSettings,
    target: &TargetSpec,
) -> Option<ShadowSetup> {
    if settings.shadow_strength <= 0.0 || settings.preset != LightingPreset::Studio {
        return None;
    }
    let (min, max) = caster_bounds?;
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let (front, back) = view_depth_extent(camera, (min, max));
    let (front, back) = (front.max(camera.znear), back.min(camera.zfar));
    if back <= front {
        return None;
    }

    let light_dir = key_light_direction(camera, settings.preset).normalize();
    let (_, _, camera_up) = camera_basis(camera);
    // Never parallel: the key sits 35° above the view direction (D5).
    let light_up = (camera_up - light_dir * camera_up.dot(light_dir)).normalize();
    let mut setup = ShadowSetup {
        light_dir,
        light_up,
        cascades: Vec::with_capacity(MAX_CASCADES),
    };
    let centre = 0.5 * (min + max);
    let radius = 0.5 * (max - min).length();
    let fit = |near: f64, far: f64| fit_cascade(camera, &setup, centre, radius, near, far);
    // Whether a cascade fitted over `[near, far]` has a texel within `limit`.
    // A slice whose footprint misses the casters is acceptable: it stores
    // nothing.
    let meets = |near: f64, far: f64, limit: f64| {
        fit(near, far).is_none_or(|cascade| cascade.texel_world <= limit)
    };

    let mut cascades: Vec<ShadowCascadeFit> = Vec::with_capacity(MAX_CASCADES);
    let mut a = front;
    for k in 0..MAX_CASCADES {
        // D5 step 0: a later cascade is fitted back over the previous one's
        // blend band, where receivers are shaded with both.
        let fit_near = cascades.last().map_or(a, |previous| {
            a - SHADOW_BLEND_FRACTION * (previous.split_far - previous.split_near)
        });
        let limit = shadow_texel_limit(camera, target, a);
        let b = if k + 1 == MAX_CASCADES || meets(fit_near, back, limit) {
            back
        } else {
            // `t` grows with `b`: the deepest `b` that meets the criterion.
            let (mut lo, mut hi) = (a, back);
            for _ in 0..SPLIT_BISECTION_STEPS {
                let mid = 0.5 * (lo + hi);
                if meets(fit_near, mid, limit) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            if lo > a {
                lo
            } else {
                // Not even a thin slice meets it (an output wider than
                // `κ·N` pixels): an even share of what is left.
                a + (back - a) / (MAX_CASCADES - k) as f64
            }
        };
        match fit(fit_near, b) {
            Some(mut cascade) => {
                cascade.split_near = a;
                cascades.push(cascade);
            }
            // The slice's footprint misses the casters, so every receiver in
            // it is lit. The previous cascade serves it: inside its square
            // the map is right about every caster, and outside it reads lit.
            None => {
                if let Some(previous) = cascades.last_mut() {
                    previous.split_far = b;
                }
            }
        }
        if b >= back {
            break;
        }
        a = b;
    }
    if cascades.is_empty() {
        return None;
    }
    setup.cascades = cascades;
    Some(setup)
}

/// The coarsest texel a cascade starting at view depth `depth` may have
/// (D6): `κ` output pixels there, but never below [`SHADOW_TEXEL_FLOOR`].
/// In Å.
pub fn shadow_texel_limit(camera: &Camera, target: &TargetSpec, depth: f64) -> f64 {
    (SHADOW_TEXELS_PER_PIXEL * output_pixel_world(camera, target, depth)).max(SHADOW_TEXEL_FLOOR)
}

/// The world size of an output pixel at view depth `depth` (D6): output
/// pixels, so HiDPI and the export's supersampling count once.
fn output_pixel_world(camera: &Camera, target: &TargetSpec, depth: f64) -> f64 {
    let output_height = target.size.height as f64 / target.pixel_scale as f64;
    let view_height = if camera.orthographic {
        2.0 * camera.ortho_half_height
    } else {
        2.0 * depth * (0.5 * camera.fovy).tan()
    };
    view_height / output_height
}

/// The cascade serving view depths `[near, far]` of `camera`, fitted to the
/// casters' bounding sphere `(centre, radius)` (D5): the visible slice's
/// footprint in light space cut to the sphere's shadow, made square with the
/// filter's margin, quantised and snapped; the whole sphere's depth range.
/// `None` when the slice's footprint misses the sphere's shadow. Its
/// `split_near` is `near`; a cascade fitted back over a blend band has its
/// own start set by the caller.
fn fit_cascade(
    camera: &Camera,
    setup: &ShadowSetup,
    centre: DVec3,
    radius: f64,
    near: f64,
    far: f64,
) -> Option<ShadowCascadeFit> {
    // 1–2. The slice's frustum corners, and their bounding rectangle `F` in
    // light space.
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for corner in frustum_corners(camera, near, far) {
        let p = setup.to_light_space(corner).truncate();
        lo = lo.min(p);
        hi = hi.max(p);
    }
    // 3. Cut to `Q`, the casters' sphere's shadow.
    let centre_l = setup.to_light_space(centre);
    lo = lo.max(centre_l.truncate() - radius);
    hi = hi.min(centre_l.truncate() + radius);
    if lo.x > hi.x || lo.y > hi.y {
        return None;
    }
    // 4. Square, with the margin, then quantised: rounding the side up keeps
    // the margin, which is in texels.
    let side = quantise_side(side_with_margin((hi - lo).max_element()));
    let texel = side / SHADOW_MAP_SIZE as f64;
    // The centre on the texel grid, from the world origin: a pan moves the
    // square by whole texels.
    let centre_square = (0.5 * (lo + hi) / texel).round() * texel;
    Some(ShadowCascadeFit {
        centre_l: centre_square,
        side,
        // The whole caster sphere, with a texel to spare at each end.
        z_near_l: centre_l.z - radius - texel,
        depth_range: 2.0 * radius + 2.0 * texel,
        split_near: near,
        split_far: far,
        texel_world: texel,
    })
}

/// The side a square must have so that one of side `inner` fits inside it
/// with the filter radius and [`SHADOW_MARGIN_TEXELS`] on every side, both of
/// which depend on the texel and so on the side itself:
/// `s ≥ inner + 2·(max(softness, 1.5·s/N) + m·s/N)`. Each branch of the
/// `max` is linear in `s`, so the answer is the larger of the two
/// solutions.
fn side_with_margin(inner: f64) -> f64 {
    let n = SHADOW_MAP_SIZE as f64;
    let m = SHADOW_MARGIN_TEXELS;
    let softness_bound = (inner + 2.0 * SHADOW_SOFTNESS_A) / (1.0 - 2.0 * m / n);
    let texel_bound = inner / (1.0 - 2.0 * (1.5 + m) / n);
    softness_bound.max(texel_bound)
}

/// `side` rounded up to a quarter-octave step, `2^(⌈4·log₂ s⌉ / 4)` (D5), so
/// a zoom changes the texel in steps of about 19 % rather than
/// continuously.
fn quantise_side(side: f64) -> f64 {
    2f64.powf((4.0 * side.log2()).ceil() / 4.0)
}

/// The eight corners of `camera`'s view frustum between view depths `near`
/// and `far`.
fn frustum_corners(camera: &Camera, near: f64, far: f64) -> [DVec3; 8] {
    let (forward, right, up) = camera_basis(camera);
    let half_height = |depth: f64| {
        if camera.orthographic {
            camera.ortho_half_height
        } else {
            depth * (0.5 * camera.fovy).tan()
        }
    };
    let mut corners = [DVec3::ZERO; 8];
    let mut i = 0;
    for depth in [near, far] {
        let h = half_height(depth);
        let w = h * camera.aspect;
        for (x, y) in [(-w, -h), (w, -h), (w, h), (-w, h)] {
            corners[i] = camera.eye + forward * depth + right * x + up * y;
            i += 1;
        }
    }
    corners
}

/// One cascade as the receiver reads it (D8). Twin of `ShadowCascade` in
/// `lighting.wgsl`; `shader_test.rs` checks the offsets.
///
/// | Field | Offset |
/// |---|---|
/// | `world_to_clip` | 0 |
/// | `split_far`, `texel_world` | 64, 68 |
/// | `filter_radius_uv`, `depth_range` | 72, 76 |
/// | *size* | 80 |
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowCascade {
    /// The light camera's `view_proj`, the matrix the caster pass draws
    /// with.
    pub world_to_clip: [[f32; 4]; 4],
    /// The view depth where the cascade ends (D6).
    pub split_far: f32,
    /// `t`, in Å.
    pub texel_world: f32,
    /// The PCF radius over the square's side: `max(softness, 1.5 t) / s`.
    pub filter_radius_uv: f32,
    /// The light camera's `zfar − znear`, in Å.
    pub depth_range: f32,
}

/// The uniform at group 2, binding 0 of every lit shader (D8). Twin of
/// `ShadowUniform` in `lighting.wgsl`; `shader_test.rs` checks the offsets.
///
/// | Field | Offset |
/// |---|---|
/// | `cascades` | 0 (stride 80) |
/// | `count`, `blend_fraction` | 320, 324 |
/// | `debug_tint`, `front` | 328, 332 |
/// | *size* | 336 |
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowUniform {
    pub cascades: [ShadowCascade; MAX_CASCADES],
    pub count: u32,
    pub blend_fraction: f32,
    /// Colour each receiver by its cascade (D9).
    pub debug_tint: u32,
    /// The view depth where cascade 0 starts. Every later cascade starts
    /// where the previous one ends; the blend band at the end of a cascade
    /// is a share of its whole slice (D7), so the receiver needs this one.
    pub front: f32,
}

impl ShadowUniform {
    /// The uniform for a draw with shadow setup `setup`; `None`: `count = 0`
    /// and every cascade slot zeroed. Pure; `ShadowMap::prepare` writes it on
    /// every draw.
    pub fn new(setup: Option<&ShadowSetup>, debug_tint: bool) -> Self {
        let mut uniform: Self = bytemuck::Zeroable::zeroed();
        uniform.debug_tint = debug_tint as u32;
        let Some(setup) = setup else {
            return uniform;
        };
        let count = setup.cascades.len().min(MAX_CASCADES);
        uniform.count = count as u32;
        uniform.blend_fraction = SHADOW_BLEND_FRACTION as f32;
        uniform.front = setup.cascades.first().map_or(0.0, |c| c.split_near as f32);
        for (k, cascade) in setup.cascades.iter().take(count).enumerate() {
            uniform.cascades[k] = ShadowCascade {
                world_to_clip: light_uniform(setup, k).view_proj,
                split_far: cascade.split_far as f32,
                texel_world: cascade.texel_world as f32,
                filter_radius_uv: (shadow_filter_radius(cascade.texel_world) / cascade.side) as f32,
                depth_range: cascade.depth_range as f32,
            };
        }
        uniform
    }
}

/// The camera uniform cascade `k`'s caster pass draws with: the receiver's
/// `world_to_clip` is its `view_proj`, so the two cannot disagree.
pub fn light_uniform(setup: &ShadowSetup, k: usize) -> CameraUniform {
    CameraUniform::new(
        &setup.light_camera(k),
        (SHADOW_MAP_SIZE, SHADOW_MAP_SIZE),
        1.0,
    )
}
