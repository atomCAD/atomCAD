//! Viewport capture to PNG.
//!
//! [`capture_screenshot`] is the CLI / AI agent path (`atomcad-cli screenshot`):
//! the live frame, gadgets included, at the requested size. [`export_viewport_image`]
//! is the user's *File > Export viewport image*: supersampled, without gadgets.

use crate::api::api_common::{CADInstance, with_mut_cad_instance_or};
use image::{ImageBuffer, Rgba};
use std::path::Path;

/// Result of a screenshot capture operation
#[derive(Debug, Clone)]
pub struct ScreenshotResult {
    pub success: bool,
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub error_message: Option<String>,
}

/// Capture the current viewport to a PNG file.
///
/// # Arguments
/// * `output_path` - Path where the PNG file will be written
/// * `width` - Optional width override (uses current viewport if None)
/// * `height` - Optional height override (uses current viewport if None)
/// * `background_rgb` - Background color as [R, G, B] (0-255), defaults to dark gray [30, 30, 30]
///
/// # Returns
/// `ScreenshotResult` indicating success/failure and metadata
#[flutter_rust_bridge::frb(sync)]
pub fn capture_screenshot(
    output_path: String,
    width: Option<u32>,
    height: Option<u32>,
    background_rgb: Option<Vec<u8>>,
) -> ScreenshotResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                capture_screenshot_impl(cad_instance, &output_path, width, height, background_rgb)
            },
            ScreenshotResult {
                success: false,
                output_path: output_path.clone(),
                width: 0,
                height: 0,
                error_message: Some("CAD instance not initialized".to_string()),
            },
        )
    }
}

/// Maximum resolution allowed for screenshots (4096x4096)
const MAX_RESOLUTION: u32 = 4096;

fn capture_screenshot_impl(
    cad_instance: &mut CADInstance,
    output_path: &str,
    width: Option<u32>,
    height: Option<u32>,
    background_rgb: Option<Vec<u8>>,
) -> ScreenshotResult {
    let renderer = &mut cad_instance.renderer;

    // What is on screen, in device pixels: the size of an unsized capture.
    let (orig_width, orig_height) = renderer.get_viewport_size();

    // Determine target size
    let target_width = width.unwrap_or(orig_width);
    let target_height = height.unwrap_or(orig_height);

    // Validate resolution limits
    if target_width == 0 || target_width > MAX_RESOLUTION {
        return ScreenshotResult {
            success: false,
            output_path: output_path.to_string(),
            width: target_width,
            height: target_height,
            error_message: Some(format!(
                "Width must be between 1 and {}, got {}",
                MAX_RESOLUTION, target_width
            )),
        };
    }
    if target_height == 0 || target_height > MAX_RESOLUTION {
        return ScreenshotResult {
            success: false,
            output_path: output_path.to_string(),
            width: target_width,
            height: target_height,
            error_message: Some(format!(
                "Height must be between 1 and {}, got {}",
                MAX_RESOLUTION, target_height
            )),
        };
    }

    // A sized capture renders the live target at exactly the requested pixels,
    // at ratio 1, and puts the request back afterwards
    // (`doc/design_viewport_antialiasing.md` D7). An unsized one renders the
    // target as it is.
    let sized = width.is_some() || height.is_some();
    let saved_request = renderer.viewport_request();
    if sized {
        renderer.set_viewport_request(target_width as f64, target_height as f64, 1.0);
    }

    // Render
    let bg_color = background_rgb
        .as_ref()
        .filter(|v| v.len() >= 3)
        .map(|v| [v[0], v[1], v[2]])
        .unwrap_or([30, 30, 30]); // Default dark gray
    // A screenshot is a still: the refined frame, without waiting for the
    // view to rest (`doc/design_viewport_antialiasing.md` D5).
    let mut pixels = renderer.render_still(bg_color);

    if sized {
        renderer.set_viewport_request(
            saved_request.logical_width,
            saved_request.logical_height,
            saved_request.device_pixel_ratio,
        );
    }

    // Convert BGRA -> RGBA (wgpu uses BGRA8Unorm)
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2); // Swap B and R
    }

    // Create image buffer and save as PNG
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        match ImageBuffer::from_raw(target_width, target_height, pixels) {
            Some(img) => img,
            None => {
                return ScreenshotResult {
                    success: false,
                    output_path: output_path.to_string(),
                    width: target_width,
                    height: target_height,
                    error_message: Some("Failed to create image buffer".to_string()),
                };
            }
        };

    // Save to file
    let path = Path::new(output_path);
    if let Err(e) = img.save(path) {
        return ScreenshotResult {
            success: false,
            output_path: output_path.to_string(),
            width: target_width,
            height: target_height,
            error_message: Some(format!("Failed to save PNG: {}", e)),
        };
    }

    ScreenshotResult {
        success: true,
        output_path: output_path.to_string(),
        width: target_width,
        height: target_height,
        error_message: None,
    }
}

/// Size an export of `width × height` will actually be written at
/// (*File > Export viewport image*): unchanged when it fits in the
/// 2048 px limit, otherwise scaled down with the aspect ratio kept. The
/// dialog shows this before exporting; [`export_viewport_image`] applies it.
#[flutter_rust_bridge::frb(sync)]
pub fn fit_viewport_image_size(width: u32, height: u32) -> ViewportImageSize {
    let (width, height, reduced) = atomcad_renderer::image_export::fit_export_size(width, height);
    ViewportImageSize {
        width,
        height,
        reduced,
    }
}

/// See [`fit_viewport_image_size`].
#[derive(Debug, Clone)]
pub struct ViewportImageSize {
    pub width: u32,
    pub height: u32,
    /// The requested size was over the limit and has been scaled down.
    pub reduced: bool,
}

/// The current viewport size in logical pixels, as `[width, height]` — the
/// default size of an exported viewport image. Logical, so the image shows
/// lines at their authored width (`doc/design_viewport_antialiasing.md` D7,
/// open question 1).
#[flutter_rust_bridge::frb(sync)]
pub fn get_viewport_image_size() -> Vec<u32> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let (width, height) = cad_instance.renderer.get_viewport_logical_size();
                vec![
                    (width.round() as u32).max(1),
                    (height.round() as u32).max(1),
                ]
            },
            vec![0, 0],
        )
    }
}

/// *File > Export viewport image*: renders the scene from the live camera at
/// `width × height` (fitted by [`fit_viewport_image_size`]), 2× supersampled,
/// without the gadget pass, and writes it to `output_path` as PNG.
///
/// `background_rgb` (0-255) overrides the background; `None` uses the
/// background colour from the preferences, i.e. the one on screen.
#[flutter_rust_bridge::frb(sync)]
pub fn export_viewport_image(
    output_path: String,
    width: u32,
    height: u32,
    background_rgb: Option<Vec<u8>>,
) -> ScreenshotResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                export_viewport_image_impl(
                    cad_instance,
                    &output_path,
                    width,
                    height,
                    background_rgb,
                )
            },
            ScreenshotResult {
                success: false,
                output_path: output_path.clone(),
                width: 0,
                height: 0,
                error_message: Some("CAD instance not initialized".to_string()),
            },
        )
    }
}

fn export_viewport_image_impl(
    cad_instance: &mut CADInstance,
    output_path: &str,
    width: u32,
    height: u32,
    background_rgb: Option<Vec<u8>>,
) -> ScreenshotResult {
    let (width, height, _) = atomcad_renderer::image_export::fit_export_size(width, height);
    let bg_color = match background_rgb.as_ref().filter(|v| v.len() >= 3) {
        Some(v) => [v[0], v[1], v[2]],
        None => {
            let c = &cad_instance
                .structure_designer
                .preferences
                .background_preferences
                .background_color;
            [c.x as u8, c.y as u8, c.z as u8]
        }
    };
    let rgba = cad_instance.renderer.render_image(width, height, bg_color);
    let result = |error_message: Option<String>| ScreenshotResult {
        success: error_message.is_none(),
        output_path: output_path.to_string(),
        width,
        height,
        error_message,
    };
    match atomcad_renderer::thumbnail::encode_png(&rgba, width, height) {
        Ok(png) => match std::fs::write(output_path, png) {
            Ok(()) => result(None),
            Err(e) => result(Some(format!("Failed to write {}: {}", output_path, e))),
        },
        Err(e) => result(Some(format!("Failed to encode PNG: {}", e))),
    }
}
