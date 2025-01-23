use wgpu::*;
use bytemuck;
use glam::f32::Vec3;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
  pub fn new(position: &Vec3, normal: &Vec3, color: &Vec3) -> Self {
    Self {
      position: [position.x, position.y, position.z],
      normal: [normal.x, normal.y, normal.z],
      color: [color.x, color.y, color.z],
    }
  }

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
              },
              wgpu::VertexAttribute {
                  offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                  shader_location: 2,
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

  // Returns the index of the added vertex.
  pub fn add_vertex(&mut self, vertex: Vertex) -> u32 {
    let length = self.vertices.len() as u32;
    self.vertices.push(vertex);
    return length;
  }

  pub fn add_triangle(&mut self, index0: u32, index1: u32, index2: u32) {
    self.indices.push(index0);
    self.indices.push(index1);
    self.indices.push(index2);
  }

  pub fn add_quad(&mut self, index0: u32, index1: u32, index2: u32, index3: u32) {
    self.add_triangle(index0, index1, index2);
    self.add_triangle(index2, index3, index0);
  }
}
