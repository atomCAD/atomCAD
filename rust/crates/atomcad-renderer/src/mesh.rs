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
    /// Opacity, `1.0` for every opaque producer.
    ///
    /// Alpha lives on the **vertex** rather than in `ModelUniform` because the
    /// renderer has no per-surface mesh: `tessellate_scene_content` builds a
    /// fixed set of singleton meshes, one per pipeline, each merging every
    /// displayed node, so "per-mesh" means "per-pipeline". Two transparent
    /// isosurfaces would otherwise share one alpha. Four bytes per vertex buys
    /// per-surface — indeed per-vertex — opacity with no dynamic-offset uniform
    /// machinery, and it keeps `ModelUniform` at its two `mat4x4<f32>` (the
    /// `CameraUniform` padding bug of issue #269 was exactly what a bare
    /// trailing `f32` on a uniform invites). See
    /// `doc/design_isosurface_node.md` §Alpha is a vertex attribute.
    pub alpha: f32,
}

impl Vertex {
    /// An **opaque** vertex: `alpha == 1.0`. Every pre-existing producer goes
    /// through here and is unaffected by the alpha channel.
    pub fn new(position: &Vec3, normal: &Vec3, material: &Material) -> Self {
        Self {
            position: [position.x, position.y, position.z],
            normal: [normal.x, normal.y, normal.z],
            albedo: [material.albedo.x, material.albedo.y, material.albedo.z],
            roughness: material.roughness,
            metallic: material.metallic,
            alpha: 1.0,
        }
    }

    /// Same, with an explicit opacity. Used by the isosurface tessellator to
    /// bake one surface's alpha onto every vertex it contributes to the merged
    /// transparent mesh.
    pub fn new_translucent(
        position: &Vec3,
        normal: &Vec3,
        material: &Material,
        alpha: f32,
    ) -> Self {
        Self {
            alpha,
            ..Self::new(position, normal, material)
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
                // alpha
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 11]>() as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/*
 * A Triangle mesh in CPU memory.
 */
#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
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
        length
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

    /// Returns the total memory usage in bytes for vertices and indices vectors
    pub fn memory_usage_bytes(&self) -> usize {
        let vertices_bytes = self.vertices.len() * std::mem::size_of::<Vertex>();
        let indices_bytes = self.indices.len() * std::mem::size_of::<u32>();
        vertices_bytes + indices_bytes
    }
}
