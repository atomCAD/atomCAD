//! Screenshot capture API for CLI/AI agent use.
//!
//! Provides functionality to capture the current viewport to a PNG file.

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

    // Save original viewport size
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

    // Set viewport size if different
    let size_changed = target_width != orig_width || target_height != orig_height;
    if size_changed {
        renderer.set_viewport_size(target_width, target_height);
    }

    // Render
    let bg_color = background_rgb
        .as_ref()
        .filter(|v| v.len() >= 3)
        .map(|v| [v[0], v[1], v[2]])
        .unwrap_or([30, 30, 30]); // Default dark gray
    let mut pixels = renderer.render(bg_color);

    // Restore viewport if changed
    if size_changed {
        renderer.set_viewport_size(orig_width, orig_height);
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
