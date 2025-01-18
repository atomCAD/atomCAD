use wgpu::*;
use bytemuck;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
  pub fn desc() -> wgpu::VertexBufferLayout<'static> {
      wgpu::VertexBufferLayout {
          array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
          step_mode: wgpu::VertexStepMode::Vertex,
          attributes: &[
              wgpu::VertexAttribute {
                  offset: 0,
                  shader_location: 0,
                  format: wgpu::VertexFormat::Float32x3,
              },
              wgpu::VertexAttribute {
                  offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                  shader_location: 1,
                  format: wgpu::VertexFormat::Float32x3,
              }
          ]
      }
  }
}

/*
 * A Triangle mesh in CPU memory.
 */
pub struct Mesh {
  pub vertices: Vec<Vertex>,
  pub indices: Vec<u32>,
}

impl Mesh {
  pub fn new() -> Self {
    Self {
      vertices: Vec::new(),
      indices: Vec::new(),    
    }
  }
}
