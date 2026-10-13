//! GPU-free pieces of *File > Export viewport image*: the size rule.
//!
//! The image is rendered at [`IMAGE_EXPORT_SUPERSAMPLING`]× its size and box
//! downsampled, which is what anti-aliases it. The render target therefore has
//! to hold twice the image on each side, and [`IMAGE_EXPORT_MAX_SIZE`] is set so
//! that it always fits inside wgpu's default texture limit (8192) with room to
//! spare. Larger images would need tiled rendering, which is deliberately not
//! implemented.

/// Factor the image is rendered at before being downsampled. Fixed: the box
/// filter is [`crate::thumbnail::downsample_2x2`].
pub const IMAGE_EXPORT_SUPERSAMPLING: u32 = 2;

/// Largest exported image side, in pixels.
pub const IMAGE_EXPORT_MAX_SIZE: u32 = 2048;

/// The size an export of `width × height` is actually written at: unchanged
/// when both sides fit in [`IMAGE_EXPORT_MAX_SIZE`], otherwise scaled down
/// uniformly so the longer side is exactly the limit (aspect ratio kept, each
/// side at least 1). Returns `(width, height, was_reduced)`.
pub fn fit_export_size(width: u32, height: u32) -> (u32, u32, bool) {
    let (width, height) = (width.max(1), height.max(1));
    let longest = width.max(height);
    if longest <= IMAGE_EXPORT_MAX_SIZE {
        return (width, height, false);
    }
    let scale = IMAGE_EXPORT_MAX_SIZE as f64 / longest as f64;
    let fit = |side: u32| ((side as f64 * scale).round() as u32).clamp(1, IMAGE_EXPORT_MAX_SIZE);
    (fit(width), fit(height), true)
}
