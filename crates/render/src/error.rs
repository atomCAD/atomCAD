// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fmt::Debug;
use thiserror::Error;

/// Reasons why the swapchain was unable to hand out a texture for the next frame.
///
/// wgpu reports these as non-success variants of [`wgpu::CurrentSurfaceTexture`] rather than as an
/// error type, so we translate them into something that can be propagated with `?`.  None of these
/// are necessarily fatal: the usual response is to skip the frame and try again, after
/// reconfiguring or recreating the surface where indicated.
#[derive(Debug, Error)]
pub enum SurfaceAcquireError {
    /// Timed out waiting for the next texture in the swapchain.
    ///
    /// Skip this frame and try again later.
    #[error("Timed out acquiring the next surface texture")]
    Timeout,

    /// The window is not visible (minimized, or fully behind another window).
    ///
    /// Skip this frame and try again once the window is visible.
    #[error("The window is occluded")]
    Occluded,

    /// The surface has changed underneath us, invalidating its configuration.
    ///
    /// The surface must be reconfigured before the next frame.
    #[error("The surface configuration is outdated")]
    Outdated,

    /// The surface is gone and has to be created anew.
    ///
    /// If the device itself was lost, the device and all of its resources must be recreated too.
    #[error("The surface has been lost")]
    Lost,

    /// A validation error was raised while acquiring the texture, and reported to the active error
    /// scope or uncaptured error handler.
    #[error("Validation error acquiring the next surface texture")]
    Validation,
}

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
    #[error("Failed to acquire surface texture: {0}")]
    SurfaceError(#[from] SurfaceAcquireError),

    /// Attempted to use a render context that doesn't exist or has been destroyed.
    ///
    /// This may not be a fatal error.  On some platforms (e.g. mobile), the render context is
    /// destroyed when the application goes out of focus, so this could just mean we're running in
    /// the background.
    #[error("Render context not active")]
    NoRenderContext,
}

// End of File
