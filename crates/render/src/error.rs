// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fmt::Debug;
use thiserror::Error;

/// Errors that can occur during rendering context operations.
///
/// Provides a unified error type for all rendering operations, wrapping various
/// underlying wgpu errors and adding context-specific error conditions.
#[derive(Debug, Error)]
pub enum RenderContextError {
    /// Failed to create a surface for rendering.
    ///
    /// Usually occurs due to platform/windowing system issues or invalid window handles.
    #[error("Failed to create surface: {0}")]
    CreateSurfaceError(#[from] wgpu::CreateSurfaceError),

    /// Failed to obtain a GPU adapter.
    ///
    /// May occur if no compatible GPU is found or when running on unsupported hardware.
    #[error("Failed to request adapter: {0}")]
    RequestAdapterError(#[from] wgpu::RequestAdapterError),

    /// Failed to create a logical device from the adapter.
    ///
    /// Typically happens when requesting unsupported features or when GPU initialization fails.
    #[error("Failed to request device: {0}")]
    RequestDeviceError(#[from] wgpu::RequestDeviceError),

    /// Surface operation failed during rendering.
    ///
    /// Common when the surface becomes invalid (window resized/minimized) or the GPU context is lost.
    #[error("Failed to create surface: {0}")]
    SurfaceError(#[from] wgpu::SurfaceError),

    /// Attempted to use a render context that doesn't exist or has been destroyed.
    ///
    /// This may not be a fatal error.  On some platforms (e.g. mobile), the render context is
    /// destroyed when the application goes out of focus, so this could just mean we're running in
    /// the background.
    #[error("Render context not active")]
    NoRenderContext,
}

// End of File
