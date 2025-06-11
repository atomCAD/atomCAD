use bytemuck;
use glam::f32::Vec3;

pub struct Material {
  albedo: Vec3,
  roughness: f32,
  metallic: f32,
}

impl Material {
  pub fn new(albedo: &Vec3, roughness: f32, metallic: f32) -> Self {
    Self {
      albedo: *albedo,
      roughness,
      metallic,
    }
  }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub albedo: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
}

impl Vertex {
  pub fn new(position: &Vec3, normal: &Vec3, material: &Material) -> Self {
    Self {
      position: [position.x, position.y, position.z],
      normal: [normal.x, normal.y, normal.z],
      albedo: [material.albedo.x, material.albedo.y, material.albedo.z],
      roughness: material.roughness,
      metallic: material.metallic,
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
              },
              wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 9]>() as wgpu::BufferAddress,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32,
              },
              wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 10]>() as wgpu::BufferAddress,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32,
              },
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

  /// Adds a polygon to the mesh by triangulating it
  /// Uses simple fan triangulation, which works for convex polygons
  ///
  /// # Arguments
  /// * `vertices` - An array of vertex indices representing the polygon vertices
  pub fn add_polygon(&mut self, vertices: &[u32]) {
    // Need at least 3 vertices to form a triangle
    if vertices.len() < 3 {
      return;
    }
    
    // For a triangle, just add it directly
    if vertices.len() == 3 {
      self.add_triangle(vertices[0], vertices[1], vertices[2]);
      return;
    }
    
    // For quads, use the built-in add_quad which creates two triangles
    if vertices.len() == 4 {
      self.add_quad(vertices[0], vertices[1], vertices[2], vertices[3]);
      return;
    }
    
    // For polygons with more than 4 vertices, use fan triangulation
    let anchor = vertices[0];
    for i in 1..(vertices.len() - 1) {
      self.add_triangle(anchor, vertices[i], vertices[i + 1]);
    }
  }
}
