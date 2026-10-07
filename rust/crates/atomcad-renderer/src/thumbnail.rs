//! GPU-free pieces of node network thumbnails (`doc/design_network_thumbnails.md`):
//! the bounds of the content meshes (D4), the automatic framing (D4), the
//! 2×2 box downsample (D5), PNG encoding and decoding, and the "did the
//! picture really change" comparison (D3).
//!
//! The draw itself is `Renderer::render_thumbnail`; everything here is a pure
//! function so it can be tested without a GPU.

use crate::atom_impostor_mesh::AtomImpostorMesh;
use crate::bond_impostor_mesh::BondImpostorMesh;
use crate::camera::Camera;
use crate::line_mesh::LineMesh;
use crate::mesh::Mesh;
use crate::transparent_impostor_mesh::TransparentImpostorMesh;
use glam::f64::DVec3;

/// Side of the stored thumbnail, in pixels (D5, D7).
pub const THUMBNAIL_SIZE: u32 = 128;

/// Side of the render the thumbnail is downsampled from (D5): impostors are
/// not antialiased, so the 2×2 box filter is what smooths their edges.
pub const THUMBNAIL_RENDER_SIZE: u32 = THUMBNAIL_SIZE * 2;

/// The light thumbnail background (D5), for dark content (carbon, geometry).
pub const THUMBNAIL_LIGHT_BACKGROUND_RGB: [u8; 3] = [216, 220, 226];

/// The dark thumbnail background (D5), for light content (silicon, hydrogen).
pub const THUMBNAIL_DARK_BACKGROUND_RGB: [u8; 3] = [42, 46, 52];

/// A content pixel "stands out" from a background when their luminances
/// differ by at least this much (out of 255).
const BACKGROUND_CONTRAST_MIN: f64 = 64.0;

/// Two renders agree on a pixel — it is opaque content, not background — when
/// no channel differs by more than this.
const CONTENT_PIXEL_TOLERANCE: u8 = 2;

/// A pixel counts as different when one of its channels differs by more than
/// this (D3, out of 255). Tuned against renderer noise, not against small
/// edits — do not raise it to hide edits.
pub const PIXEL_DIFF_THRESHOLD: u8 = 24;

/// The image counts as changed when more than this fraction of its pixels
/// differ (D3).
pub const CHANGED_PIXEL_FRACTION: f64 = 0.005;

/// Margin around the bounding sphere in the automatic framing (D4).
const FIT_MARGIN: f64 = 1.1;

/// Smallest bounding-sphere radius the framing uses, so a single point (or a
/// degenerate mesh) still gets a sensible camera.
const MIN_FIT_RADIUS: f64 = 0.5;

/// A growing axis-aligned box.
#[derive(Default)]
struct BoundsAccumulator {
    bounds: Option<(DVec3, DVec3)>,
}

impl BoundsAccumulator {
    fn add_sphere(&mut self, center: [f32; 3], radius: f32) {
        let c = DVec3::new(center[0] as f64, center[1] as f64, center[2] as f64);
        if !c.is_finite() {
            return;
        }
        let r = DVec3::splat(radius.max(0.0) as f64);
        let (lo, hi) = (c - r, c + r);
        self.bounds = Some(match self.bounds {
            None => (lo, hi),
            Some((min, max)) => (min.min(lo), max.max(hi)),
        });
    }

    fn add_point(&mut self, p: [f32; 3]) {
        self.add_sphere(p, 0.0);
    }
}

/// The axis-aligned bounding box of the content meshes (D4): triangle and
/// wireframe vertices, atom impostor centres grown by their radius, bond
/// impostor endpoints grown by their radius, transparent impostors and
/// transparent isosurface vertices. `None` when there is nothing to frame.
///
/// Gadgets, the lightweight mesh, labels and background lines are not
/// content and are not passed in.
pub fn content_bounds(
    main_mesh: &Mesh,
    wireframe_mesh: &LineMesh,
    atom_impostor_mesh: &AtomImpostorMesh,
    bond_impostor_mesh: &BondImpostorMesh,
    transparent_impostor_mesh: &TransparentImpostorMesh,
    isosurface_transparent_mesh: &Mesh,
) -> Option<(DVec3, DVec3)> {
    let mut acc = BoundsAccumulator::default();
    // Only indexed vertices are drawn, but every mesh here indexes all of its
    // vertices, so the vertex list is the cheaper exact answer.
    if !main_mesh.indices.is_empty() {
        main_mesh
            .vertices
            .iter()
            .for_each(|v| acc.add_point(v.position));
    }
    if !isosurface_transparent_mesh.indices.is_empty() {
        isosurface_transparent_mesh
            .vertices
            .iter()
            .for_each(|v| acc.add_point(v.position));
    }
    if !wireframe_mesh.indices.is_empty() {
        wireframe_mesh
            .vertices
            .iter()
            .for_each(|v| acc.add_point(v.position));
    }
    if !atom_impostor_mesh.indices.is_empty() {
        atom_impostor_mesh
            .vertices
            .iter()
            .for_each(|v| acc.add_sphere(v.center_position, v.radius));
    }
    if !bond_impostor_mesh.indices.is_empty() {
        for v in &bond_impostor_mesh.vertices {
            acc.add_sphere(v.start_position, v.radius);
            acc.add_sphere(v.end_position, v.radius);
        }
    }
    if !transparent_impostor_mesh.indices.is_empty() {
        for v in &transparent_impostor_mesh.vertices {
            acc.add_sphere(v.position_a, v.radius);
            // `position_b` is unused for atoms (kind 0).
            if v.kind == 1 {
                acc.add_sphere(v.position_b, v.radius);
            }
        }
    }
    acc.bounds
}

/// The automatic thumbnail camera (D4): `camera`'s viewing **direction** and
/// roll, at the distance that fits the bounding sphere of `bounds`, square.
///
/// Perspective: `eye = c + dir · (1.1 · r / sin(fovy / 2))`. Orthographic: the
/// same eye (any eye outside the sphere would do) and
/// `ortho_half_height = 1.1 · r`. The clip range covers the sphere with a
/// margin, whatever the structure's size.
pub fn fit_camera_to_bounds(camera: &Camera, bounds: (DVec3, DVec3)) -> Camera {
    let (min, max) = bounds;
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(MIN_FIT_RADIUS);

    let dir = (camera.eye - camera.target)
        .try_normalize()
        .unwrap_or_else(|| {
            (Camera::default_pose(1.0).eye - Camera::default_pose(1.0).target).normalize()
        });
    let half_fovy = (camera.fovy * 0.5).clamp(1e-3, std::f64::consts::FRAC_PI_2);
    let distance = FIT_MARGIN * radius / half_fovy.sin();

    let mut fitted = camera.clone();
    fitted.aspect = 1.0;
    fitted.target = center;
    fitted.eye = center + dir * distance;
    fitted.pivot_point = center;
    fitted.ortho_half_height = FIT_MARGIN * radius;
    // The clip range wanted: the sphere with a margin. `distance > 1.1 r`, so
    // the near end stays in front of the eye.
    let near = (distance - 1.2 * radius).max(distance * 1e-3);
    let far = distance + 1.2 * radius;
    fitted.zfar = far;
    fitted.znear = if fitted.orthographic {
        near
    } else {
        // `perspective_rh_gl` maps depth to [-1, 1] but wgpu clips to [0, 1],
        // so the plane that actually clips is where GL depth is 0:
        // `2·n·f / (n + f)`, not `n`. With the live camera's 1.5 / 2400 that is
        // ~3 and goes unnoticed; with a clip range fitted this tightly it lands
        // a third of the radius in front of the centre and cuts the front off
        // the part. Solve for the `n` that puts that plane at `near`.
        near * far / (2.0 * far - near)
    };
    fitted.orthonormalize_up();
    fitted
}

/// The live camera as a square thumbnail (D6): same pose and vertical field
/// of view, `aspect = 1`, so the image is the centre of the viewport.
pub fn square_camera(camera: &Camera) -> Camera {
    let mut square = camera.clone();
    square.aspect = 1.0;
    square
}

/// Converts a tightly packed BGRA image (what the render target reads back)
/// into RGBA, in place.
pub fn bgra_to_rgba_in_place(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
}

/// 2×2 box downsample of a tightly packed 4-channel image whose sides are
/// even (D5). Returns an image of `width / 2 × height / 2`.
pub fn downsample_2x2(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let (ow, oh) = (w / 2, h / 2);
    let mut out = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            for c in 0..4 {
                let at = |xx: usize, yy: usize| pixels[(yy * w + xx) * 4 + c] as u32;
                let sum = at(2 * x, 2 * y)
                    + at(2 * x + 1, 2 * y)
                    + at(2 * x, 2 * y + 1)
                    + at(2 * x + 1, 2 * y + 1);
                out[(y * ow + x) * 4 + c] = ((sum + 2) / 4) as u8;
            }
        }
    }
    out
}

/// Encodes a tightly packed RGBA image as PNG.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "pixel buffer does not match the image size".to_string())?;
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Decodes a PNG into `(rgba, width, height)`.
pub fn decode_png(png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Ok((image.into_raw(), width, height))
}

/// Whether two RGBA images of the same size differ by more than renderer
/// noise (D3): more than [`CHANGED_PIXEL_FRACTION`] of the pixels have a
/// channel differing by more than [`PIXEL_DIFF_THRESHOLD`].
pub fn images_differ(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() || !a.len().is_multiple_of(4) {
        return true;
    }
    let pixel_count = a.len() / 4;
    if pixel_count == 0 {
        return false;
    }
    let differing = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(pa, pb)| {
            pa.iter()
                .zip(pb.iter())
                .any(|(x, y)| x.abs_diff(*y) > PIXEL_DIFF_THRESHOLD)
        })
        .count();
    differing as f64 > CHANGED_PIXEL_FRACTION * pixel_count as f64
}

/// Whether a freshly rendered thumbnail should replace the stored one (D3).
/// `true` when there is no stored image, when it cannot be decoded or has
/// another size, or when the pictures really differ.
pub fn thumbnail_changed(stored_png: Option<&[u8]>, rgba: &[u8], width: u32, height: u32) -> bool {
    let Some(stored_png) = stored_png else {
        return true;
    };
    match decode_png(stored_png) {
        Ok((stored, w, h)) if w == width && h == height => images_differ(&stored, rgba),
        _ => true,
    }
}

/// Which of the two backgrounds a thumbnail is drawn on (D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbnailBackground {
    Light,
    Dark,
}

impl ThumbnailBackground {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            ThumbnailBackground::Light => THUMBNAIL_LIGHT_BACKGROUND_RGB,
            ThumbnailBackground::Dark => THUMBNAIL_DARK_BACKGROUND_RGB,
        }
    }
}

/// Rec. 709 luma of an RGB triple, on the stored 0–255 values.
fn luminance(rgb: &[u8]) -> f64 {
    0.2126 * rgb[0] as f64 + 0.7152 * rgb[1] as f64 + 0.0722 * rgb[2] as f64
}

/// Picks the background the content stands out on (D5), from the same view
/// rendered on both backgrounds (tightly packed RGBA of the same size).
///
/// Pixels on which the two renders agree are opaque content; everything else
/// is background, an antialiased edge or transparent content, and is ignored.
/// Each background scores the content pixels that differ from it in luminance
/// by at least [`BACKGROUND_CONTRAST_MIN`]; the higher score wins. That
/// follows the majority material in mixed content (a carbon tip with some
/// hydrogen gets the light background) where a mean luminance would land in
/// the middle. A tie — including no opaque content at all — goes to the
/// background farther from the mean content luminance, then to light.
///
/// Depends only on the content, so the stored image still does not depend
/// on who saved the file.
pub fn choose_background(on_light: &[u8], on_dark: &[u8]) -> ThumbnailBackground {
    let light = luminance(&THUMBNAIL_LIGHT_BACKGROUND_RGB);
    let dark = luminance(&THUMBNAIL_DARK_BACKGROUND_RGB);
    let (mut light_score, mut dark_score) = (0usize, 0usize);
    let (mut sum, mut count) = (0.0, 0usize);
    for (a, b) in on_light.chunks_exact(4).zip(on_dark.chunks_exact(4)) {
        let agree = a[..3]
            .iter()
            .zip(&b[..3])
            .all(|(x, y)| x.abs_diff(*y) <= CONTENT_PIXEL_TOLERANCE);
        if !agree {
            continue;
        }
        let l = luminance(a);
        if (l - light).abs() >= BACKGROUND_CONTRAST_MIN {
            light_score += 1;
        }
        if (l - dark).abs() >= BACKGROUND_CONTRAST_MIN {
            dark_score += 1;
        }
        sum += l;
        count += 1;
    }
    if light_score != dark_score {
        return if light_score > dark_score {
            ThumbnailBackground::Light
        } else {
            ThumbnailBackground::Dark
        };
    }
    if count > 0 {
        let mean = sum / count as f64;
        if (mean - dark).abs() > (mean - light).abs() {
            return ThumbnailBackground::Dark;
        }
    }
    ThumbnailBackground::Light
}
