use crate::renderer::tessellator::tessellator::Tessellatable;
use glam::f64::DVec3;

/// Camera-derived information for gadget picking, allowing hit tests to
/// enforce a minimum grab size in *screen* pixels regardless of zoom level.
///
/// A zeroed context (`GadgetPickContext::disabled()`) yields a zero
/// world-per-pixel scale, which disables the pixel minimum and falls back to
/// the gadget's world-space hit geometry.
#[derive(Clone, Copy, Debug)]
pub struct GadgetPickContext {
    /// Camera eye position (world space).
    pub eye: DVec3,
    /// Perspective mode: world units covered by one pixel at unit distance
    /// from the eye — `2 * tan(fovy / 2) / viewport_height_px`.
    pub perspective_world_per_pixel: f64,
    /// Orthographic mode: world units covered by one pixel —
    /// `2 * ortho_half_height / viewport_height_px`.
    pub ortho_world_per_pixel: f64,
    /// Whether the camera is in orthographic mode.
    pub orthographic: bool,
}

impl GadgetPickContext {
    /// A context with no camera information; pixel-based hit minimums degrade
    /// to plain world-space hit testing.
    pub fn disabled() -> Self {
        Self {
            eye: DVec3::ZERO,
            perspective_world_per_pixel: 0.0,
            ortho_world_per_pixel: 0.0,
            orthographic: false,
        }
    }

    /// World-space length of one screen pixel at the given world point.
    pub fn world_per_pixel_at(&self, point: &DVec3) -> f64 {
        if self.orthographic {
            self.ortho_world_per_pixel
        } else {
            (*point - self.eye).length() * self.perspective_world_per_pixel
        }
    }
}

impl Default for GadgetPickContext {
    fn default() -> Self {
        Self::disabled()
    }
}

pub trait Gadget: Tessellatable {
    // Returns the index of the handle that was hit, or None if no handle was hit.
    fn hit_test(
        &self,
        ray_origin: DVec3,
        ray_direction: DVec3,
        pick_ctx: &GadgetPickContext,
    ) -> Option<i32>;

    // Start dragging the handle with the given index.
    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3);

    // Drag the handle with the given index.
    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3);

    // End dragging the handle.
    fn end_drag(&mut self);
}
