
mod after_use;

pub use after_use::AfterUse;

use winit::dpi::PhysicalSize;

pub struct MiddlewareResize {
    size: PhysicalSize<u32>,
    format: wgpu::TextureFormat,
}

pub trait MiddlewareRenderer {
    type InitData;
    fn new(device: &wgpu::Device, resize: MiddlewareResize, data: Self::InitData) -> Self;
    fn prepare(&mut self, );
}
