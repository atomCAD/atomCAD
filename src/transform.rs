use crate::bind_groups::BindGroupLayouts;
use ultraviolet::Mat4;

pub struct Transform {
    bind_group: wgpu::BindGroup,
    buffer: wgpu::Buffer,

    transform: Mat4,
}

impl Transform {
    pub fn new(device: &wgpu::Device, bgl: &BindGroupLayouts) -> Self {

    }
}
