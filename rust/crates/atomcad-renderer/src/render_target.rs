//! What one draw renders into, and the pure rules that size it
//! (`doc/design_viewport_antialiasing.md` D1, D7, D8).

use wgpu::*;

pub(crate) const COLOR_FORMAT: TextureFormat = TextureFormat::Bgra8Unorm;
pub(crate) const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

/// The live viewport's sample count when anti-aliasing is on (D1). WebGPU
/// guarantees 1 and 4 for both [`COLOR_FORMAT`] and [`DEPTH_FORMAT`].
pub const LIVE_SAMPLE_COUNT: u32 = 4;

/// The size, pixel scale and sample count of a [`RenderTarget`]: everything
/// about it that is decided before a device is involved, so the rules can be
/// tested without one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetSpec {
    /// In render pixels.
    pub size: Extent3d,
    /// Render-target pixels per output pixel (`doc/design_screen_space_lines.md`
    /// D2).
    pub pixel_scale: f32,
    /// Samples per pixel of the colour and depth attachments (D1).
    pub sample_count: u32,
}

impl TargetSpec {
    /// An offscreen image (`out_w × out_h`) rendered at `supersampling`×:
    /// size = out × supersampling, pixel_scale = supersampling, 1 sample.
    /// The thumbnail and the export (D1: offscreen targets are supersampled,
    /// never multisampled).
    pub fn image(out_w: u32, out_h: u32, supersampling: u32) -> Self {
        Self {
            size: Extent3d {
                width: out_w * supersampling,
                height: out_h * supersampling,
                depth_or_array_layers: 1,
            },
            pixel_scale: supersampling as f32,
            sample_count: 1,
        }
    }

    /// The live viewport (D7): `logical_w × logical_h` logical pixels at
    /// `device_pixel_ratio`, multisampled at `sample_count`. Each side is
    /// rounded and at least 1; the pixel scale is the ratio.
    pub fn viewport(
        logical_w: f64,
        logical_h: f64,
        device_pixel_ratio: f64,
        sample_count: u32,
    ) -> Self {
        let side = |logical: f64| ((logical * device_pixel_ratio).round() as u32).max(1);
        Self {
            size: Extent3d {
                width: side(logical_w),
                height: side(logical_h),
                depth_or_array_layers: 1,
            },
            pixel_scale: device_pixel_ratio as f32,
            sample_count,
        }
    }
}

/// The live viewport's quality settings: the renderer-side image of the
/// *Rendering* preferences (D8), pushed in from the api on every refresh.
/// `Default` matches the preference defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderQuality {
    /// 4× MSAA for the live viewport (D1).
    pub anti_aliasing: bool,
    /// Render the live viewport at the device pixel ratio (D7). Off: at the
    /// logical size, which Flutter scales up.
    pub full_display_resolution: bool,
}

impl Default for RenderQuality {
    fn default() -> Self {
        Self {
            anti_aliasing: true,
            full_display_resolution: true,
        }
    }
}

/// The live target for a `logical_w × logical_h` viewport on a display of
/// `device_pixel_ratio` under `quality` (D7, D8). `msaa_supported` is the
/// adapter check of D2: without it the target has one sample whatever the
/// preference says.
pub fn live_target_spec(
    logical_w: f64,
    logical_h: f64,
    device_pixel_ratio: f64,
    quality: RenderQuality,
    msaa_supported: bool,
) -> TargetSpec {
    let sample_count = if quality.anti_aliasing && msaa_supported {
        LIVE_SAMPLE_COUNT
    } else {
        1
    };
    let ratio = if quality.full_display_resolution {
        sanitized_ratio(device_pixel_ratio)
    } else {
        1.0
    };
    TargetSpec::viewport(logical_w, logical_h, ratio, sample_count)
}

/// What Flutter laid out for the live viewport (D7): its size in logical
/// pixels and the display's device pixel ratio. The renderer keeps the last
/// one and derives the live target from it and the [`RenderQuality`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportRequest {
    pub logical_width: f64,
    pub logical_height: f64,
    pub device_pixel_ratio: f64,
}

impl ViewportRequest {
    /// The live target for this request: [`live_target_spec`].
    pub fn target_spec(&self, quality: RenderQuality, msaa_supported: bool) -> TargetSpec {
        live_target_spec(
            self.logical_width,
            self.logical_height,
            self.device_pixel_ratio,
            quality,
            msaa_supported,
        )
    }
}

/// A device pixel ratio a target can be built from: Flutter never reports a
/// non-positive or non-finite one, but a bad value from a caller must not
/// become a zero pixel scale (every screen-space length would collapse).
fn sanitized_ratio(device_pixel_ratio: f64) -> f64 {
    if device_pixel_ratio.is_finite() && device_pixel_ratio > 0.0 {
        device_pixel_ratio
    } else {
        1.0
    }
}

/// What one draw renders into: colour and depth textures of one size and the
/// buffer the colour is read back through. The live viewport owns one and the
/// thumbnail owns another, so a thumbnail capture never resizes or
/// reallocates the viewport's (`doc/design_network_thumbnails.md` D5).
///
/// A target owns its **pixel scale** (`doc/design_screen_space_lines.md`
/// D2) and its **sample count** (D1): both are fixed where the target is
/// created, so nothing drawn into it, and no export code, has to know about
/// supersampling or multisampling.
pub struct RenderTarget {
    /// Single-sample: the image read back. With MSAA, the resolve target.
    pub(crate) texture: Texture,
    pub(crate) texture_view: TextureView,
    /// The multisampled colour attachment; `None` when `sample_count == 1`.
    pub(crate) msaa_view: Option<TextureView>,
    /// `sample_count` samples.
    pub(crate) depth_texture_view: TextureView,
    pub(crate) output_buffer: Buffer,
    /// In render pixels.
    pub(crate) size: Extent3d,
    /// Render-target pixels per output pixel.
    pub(crate) pixel_scale: f32,
    pub(crate) sample_count: u32,
}

impl RenderTarget {
    pub fn new(device: &Device, spec: TargetSpec) -> Self {
        let size = spec.size;
        let texture = create_color_texture(device, &size, 1, "Render Target Texture");
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let msaa_view = (spec.sample_count > 1).then(|| {
            create_color_texture(device, &size, spec.sample_count, "MSAA Colour Texture")
                .create_view(&TextureViewDescriptor::default())
        });
        let depth_texture = create_depth_texture(device, &size, spec.sample_count);
        let depth_texture_view = depth_texture.create_view(&TextureViewDescriptor::default());
        let output_buffer = create_output_buffer(device, &size);
        Self {
            texture,
            texture_view,
            msaa_view,
            depth_texture_view,
            output_buffer,
            size,
            pixel_scale: spec.pixel_scale,
            sample_count: spec.sample_count,
        }
    }

    /// The spec this target was created from.
    pub fn spec(&self) -> TargetSpec {
        TargetSpec {
            size: self.size,
            pixel_scale: self.pixel_scale,
            sample_count: self.sample_count,
        }
    }

    /// The colour attachment of a pass that starts with `load` (D3). On a
    /// multisampled target the pass draws into the multisampled texture; the
    /// last pass (`resolve`) resolves it into `texture` and discards it, an
    /// earlier one stores it for the next. On a single-sample target the pass
    /// draws into `texture` and stores it, whatever `resolve` says.
    pub(crate) fn color_attachment(
        &self,
        load: LoadOp<Color>,
        resolve: bool,
    ) -> RenderPassColorAttachment<'_> {
        match &self.msaa_view {
            Some(msaa_view) => RenderPassColorAttachment {
                view: msaa_view,
                resolve_target: resolve.then_some(&self.texture_view),
                ops: Operations {
                    load,
                    store: if resolve {
                        StoreOp::Discard
                    } else {
                        StoreOp::Store
                    },
                },
            },
            None => RenderPassColorAttachment {
                view: &self.texture_view,
                resolve_target: None,
                ops: Operations {
                    load,
                    store: StoreOp::Store,
                },
            },
        }
    }

    /// The depth attachment of a pass, cleared to the far plane (D3). Depth
    /// is never read after a pass, so a multisampled target discards it; a
    /// single-sample one stores it, as before the design.
    pub(crate) fn depth_attachment(&self) -> RenderPassDepthStencilAttachment<'_> {
        RenderPassDepthStencilAttachment {
            view: &self.depth_texture_view,
            depth_ops: Some(Operations {
                load: LoadOp::Clear(1.0),
                store: if self.msaa_view.is_some() {
                    StoreOp::Discard
                } else {
                    StoreOp::Store
                },
            }),
            stencil_ops: None,
        }
    }
}

fn create_color_texture(
    device: &Device,
    texture_size: &Extent3d,
    sample_count: u32,
    label: &str,
) -> Texture {
    // Only the single-sample texture is copied out; the multisampled one is
    // resolved into it.
    let usage = if sample_count == 1 {
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC
    } else {
        TextureUsages::RENDER_ATTACHMENT
    };
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: *texture_size,
        mip_level_count: 1,
        sample_count,
        dimension: TextureDimension::D2,
        format: COLOR_FORMAT,
        usage,
        view_formats: &[],
    })
}

fn create_depth_texture(device: &Device, texture_size: &Extent3d, sample_count: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("Depth Buffer"),
        size: *texture_size,
        mip_level_count: 1,
        sample_count,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn create_output_buffer(device: &Device, texture_size: &Extent3d) -> Buffer {
    // Calculate aligned bytes per row (must be a multiple of 256 in WebGPU)
    let bytes_per_row = 4 * texture_size.width; // 4 bytes per pixel (RGBA8)
    let aligned_bytes_per_row = (bytes_per_row + 255) & !255;
    let buffer_size = aligned_bytes_per_row * texture_size.height;

    device.create_buffer(&BufferDescriptor {
        label: Some("Output Buffer"),
        size: buffer_size as BufferAddress,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}
