// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::error::RenderContextError;
use pollster::FutureExt as _;
use std::sync::Arc;
use winit::window::Window;

/// Encapsulates resources for a single frame to separate frame setup from rendering.
/// Prevents accidentally submitting commands mid-frame by bundling the encoder with its target.
pub struct FrameEncoder {
    /// Target view for rendering this frame
    view: wgpu::TextureView,
    /// Command buffer for recording rendering operations
    encoder: wgpu::CommandEncoder,
    /// Surface texture to present when complete
    buffer: wgpu::SurfaceTexture,
}

impl FrameEncoder {
    /// Provides access to command encoder for recording render operations.
    pub fn encoder(&self) -> &wgpu::CommandEncoder {
        &self.encoder
    }

    /// Allows modifying the command buffer directly when more control is needed.
    pub fn encoder_mut(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.encoder
    }

    /// Access the current frame's texture view for creating render passes.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Mutable access to the texture view (see also [`Self::encoder_view_mut`]).
    pub fn view_mut(&mut self) -> &mut wgpu::TextureView {
        &mut self.view
    }

    /// Convenience accessor when both encoder and view are needed simultaneously.
    /// Avoids multiple borrow checks when setting up render passes.
    pub fn encoder_view_mut(&mut self) -> (&mut wgpu::CommandEncoder, &mut wgpu::TextureView) {
        (&mut self.encoder, &mut self.view)
    }
}

/// Central access point for GPU resources and rendering operations.
/// Manages the lifecycle of device, surface, and frame resources to simplify the rendering API.
pub struct RenderContext {
    /// Shared ownership of window to ensure it outlives [`Self::surface`].
    window: Arc<Window>,
    /// Current surface configuration
    surface_config: wgpu::SurfaceConfiguration,
    /// GPU drawing surface connected to the window
    surface: wgpu::Surface<'static>,
    /// GPU device for resource creation and command execution
    device: wgpu::Device,
    /// Command queue for submitting work to the GPU
    queue: wgpu::Queue,
    /// Current frame being rendered, if any, to prevent multiple frames simultaneously
    current_frame: Option<FrameEncoder>,
}

impl RenderContext {
    /// Creates a rendering context with optimal device settings for the current platform.
    ///
    /// Handles adapter selection & cross-platform compatibility concerns.  Format selection
    /// prioritizes sRGB color space for correct color reproduction.
    ///
    /// # Parameters
    /// * `window` - The window to create the rendering context for
    pub fn new(window: Window) -> Result<Self, RenderContextError> {
        async {
            let window = Arc::new(window);

            // We will attempt to create a surface to fill the entire window.
            let actual_size = window.inner_size();

            // We need to use a special utility function to create the instance so that we can
            // target WebGPU and automatically fall back to WebGL if WebGPU is not available.
            // Otherwise there are some browsers that have WebGPU supposedly enabled but not
            // actually usable, and we'd be stuck without a usable render canvas.
            let instance =
                wgpu::util::new_instance_with_webgpu_detection(&wgpu::InstanceDescriptor {
                    // wgpu will pick the best backend for the current platform, preferring any of
                    // the primary (first-class support) backends: Vulkan, Metal, DX12, or WebGPU.
                    // If none of those are available, fall back to GL.
                    backends: wgpu::Backends::PRIMARY | wgpu::Backends::GL,
                    // Turn on debugging and validation flags if cfg!(debug_assertions), and in
                    // either case take some extra validation flags from the environment.  See
                    // wgpu-rs docs for more info.
                    flags: wgpu::InstanceFlags::from_build_config().with_env(),
                    memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                    backend_options: wgpu::BackendOptions {
                        // We do not require any special options for the GL backend.
                        gl: wgpu::GlBackendOptions {
                            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
                            fence_behavior: wgpu::GlFenceBehavior::Normal,
                        },
                        dx12: wgpu::Dx12BackendOptions {
                            // FIXME: We should switch to use DynamicDxc or StaticDxc, but both
                            //        require special linking instructions. See the wgpu-rs docs for
                            //        more information.
                            shader_compiler: wgpu::Dx12Compiler::Fxc,
                            presentation_system: wgpu_types::Dx12SwapchainKind::from_env()
                                .unwrap_or_default(),
                            latency_waitable_object:
                                wgpu_types::Dx12UseFrameLatencyWaitableObject::from_env()
                                    .unwrap_or_default(),
                        },
                        // Noop is a backend that does nothing.
                        // It is useful for testing, I guess?
                        // We have no need for it.
                        noop: wgpu::NoopBackendOptions { enable: false },
                    },
                })
                .await;

            let surface = instance.create_surface(window.clone())?;

            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    // We want the best GPU available, even if it is a discrete GPU.
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    // Forcing the fallback adapter means software rendering (eww).
                    force_fallback_adapter: false,
                    // Must be able to present to this window though.
                    compatible_surface: Some(&surface),
                })
                .await?;

            // There are no beyond-baseline features that must be supported by the device.
            let required_features = wgpu::Features::empty();
            // Set limits low enough to ensure compatibility with WebGL2 devices.
            let required_limits = wgpu::Limits::downlevel_webgl2_defaults();
            // Disable experimental features for maximum stability.
            let experimental_features = wgpu_types::ExperimentalFeatures::disabled();
            // Tell the driver to optimize memory usage for maximum performance.
            let memory_hints = wgpu::MemoryHints::Performance;

            // The device is used to create resources, and queue to submit commands.
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Main Device"),
                    required_features,
                    required_limits,
                    experimental_features,
                    memory_hints,
                    // Tracing is not currently supported by wgpu-rs:
                    // https://github.com/gfx-rs/wgpu/issues/5974
                    trace: wgpu::Trace::Off,
                })
                .await?;

            let swapchain_format = if cfg!(any(target_os = "android", target_arch = "wasm32")) {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Bgra8UnormSrgb
            };

            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: swapchain_format,
                width: actual_size.width,
                height: actual_size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![swapchain_format],
            };

            surface.configure(&device, &surface_config);

            Ok(Self {
                window,
                surface_config,
                surface,
                device,
                queue,
                current_frame: None,
            })
        }
        .block_on()
    }

    /// Resizes the window surface to match new dimensions.
    ///
    /// Handles device texture size limitations to prevent crashes on extreme window sizes.
    /// Avoids unnecessary reconfiguration when dimensions haven't changed to prevent
    /// performance issues and resource churn.
    ///
    /// # Parameters
    /// * `requested_size` - The new physical size of the window
    pub fn resize_surface(&mut self, requested_size: winit::dpi::PhysicalSize<u32>) {
        let limits = self.device.limits();
        if requested_size.width > limits.max_texture_dimension_2d
            || requested_size.height > limits.max_texture_dimension_2d
        {
            log::warn!(
                "Display size {}x{} is greater than the maximum texture dimension {}; upscaling will occur.",
                requested_size.width,
                requested_size.height,
                limits.max_texture_dimension_2d
            );
        }
        let actual_size = winit::dpi::PhysicalSize::new(
            requested_size.width.min(limits.max_texture_dimension_2d),
            requested_size.height.min(limits.max_texture_dimension_2d),
        );

        if self.surface_config.width != actual_size.width
            || self.surface_config.height != actual_size.height
        {
            self.surface_config.width = actual_size.width;
            self.surface_config.height = actual_size.height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    /// Creates resources for a new frame, ensuring proper frame sequencing.
    ///
    /// Returns existing frame if called multiple times to prevent accidental resource leaks.
    /// Centralizes frame setup to ensure consistent swapchain management.
    ///
    /// # Parameters
    /// * `label` - The (optional) debugging label for the command encoder.
    pub fn frame_encoder(
        &mut self,
        label: Option<&str>,
    ) -> Result<&mut FrameEncoder, wgpu::SurfaceError> {
        // There can only be one frame encoder per frame.
        if self.current_frame.is_none() {
            // Get the next texture in the swapchain
            let buffer = self.surface.get_current_texture()?;

            // Create a view for the texture
            let view = buffer
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            // Create a command encoder for this frame
            let encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label });

            // Store the frame encoder, for use in the `finish` method.
            let current_frame = FrameEncoder {
                view,
                encoder,
                buffer,
            };
            self.current_frame = Some(current_frame);
        }

        Ok(self.current_frame.as_mut().unwrap())
    }

    /// Submits commands and presents the current frame to the display.
    ///
    /// Does nothing if no frame is in progress.
    /// Frame consumption pattern ensures proper sequencing and prevents double-presentation.
    pub fn present_frame(&mut self) {
        if let Some(frame) = self.current_frame.take() {
            // Submit the command encoder
            self.queue.submit(std::iter::once(frame.encoder.finish()));
            // Present the frame
            frame.buffer.present();
        }
    }

    /// Access to window for event handling and window management operations.
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Surface configuration for format/size queries and manual configuration.
    pub fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
        &self.surface_config
    }

    /// Direct surface access for advanced rendering scenarios.
    pub fn surface(&self) -> &wgpu::Surface<'_> {
        &self.surface
    }

    /// GPU device access for creating buffers, textures, and other resources.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Command queue access for manual command submission when needed.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

// End of File
