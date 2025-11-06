use glam::Vec3;
use crate::renderer::mesh::{Mesh, Material, Vertex};

/// A vertex that can be marked as occluded during tessellation
#[derive(Clone, Debug)]
pub struct OccludableVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub occluded: bool,
}

impl OccludableVertex {
    pub fn new(position: Vec3, normal: Vec3, occluded: bool) -> Self {
        Self {
            position,
            normal,
            occluded,
        }
    }
}

/// A triangle represented by three vertex indices
#[derive(Clone, Debug)]
pub struct Triangle {
    pub v0: u32,
    pub v1: u32,
    pub v2: u32,
    pub center_occluded: bool,
}

impl Triangle {
    pub fn new(v0: u32, v1: u32, v2: u32, center_occluded: bool) -> Self {
        Self { v0, v1, v2, center_occluded }
    }
}

/// A mesh structure that supports occlusion marking before final tessellation
pub struct OccludableMesh {
    pub vertices: Vec<OccludableVertex>,
    pub triangles: Vec<Triangle>,
}

impl OccludableMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            triangles: Vec::new(),
        }
    }

    /// Add a vertex and return its index
    pub fn add_vertex(&mut self, vertex: OccludableVertex) -> u32 {
        let index = self.vertices.len() as u32;
        self.vertices.push(vertex);
        index
    }

    /// Add a triangle using three vertex indices and center occlusion status
    pub fn add_triangle(&mut self, v0: u32, v1: u32, v2: u32, center_occluded: bool) {
        self.triangles.push(Triangle::new(v0, v1, v2, center_occluded));
    }

    /// Add a quad using four vertex indices (creates two triangles)
    /// Both triangles will have the same center occlusion status
    pub fn add_quad(&mut self, v0: u32, v1: u32, v2: u32, v3: u32, center_occluded: bool) {
        self.add_triangle(v0, v1, v2, center_occluded);
        self.add_triangle(v2, v3, v0, center_occluded);
    }

    /// Get the number of vertices
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get the number of triangles
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Get the total number of indices (triangles * 3)
    pub fn index_count(&self) -> usize {
        self.triangles.len() * 3
    }

    /// Clear all vertices and triangles
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.triangles.clear();
    }

    /// Add this occludable mesh to the output mesh, handling occlusion compression
    pub fn add_to_mesh(&self, output_mesh: &mut Mesh, material: &Material) {
        // Step 1: Set up boolean Vec to mark which vertices are needed
        let mut vertex_needed = vec![false; self.vertices.len()];
        
        // Step 2: Go through triangles and determine which to keep
        let mut compressed_triangles = Vec::new();
        
        for triangle in &self.triangles {
            // Check if triangle has any non-occluded point (vertices + center)
            let v0_occluded = self.vertices[triangle.v0 as usize].occluded;
            let v1_occluded = self.vertices[triangle.v1 as usize].occluded;
            let v2_occluded = self.vertices[triangle.v2 as usize].occluded;
            let center_occluded = triangle.center_occluded;
            
            // Keep triangle if ANY vertex (including center) is NOT occluded
            let should_keep = !v0_occluded || !v1_occluded || !v2_occluded || !center_occluded;
            
            if should_keep {
                // Mark all vertices of this triangle as needed
                vertex_needed[triangle.v0 as usize] = true;
                vertex_needed[triangle.v1 as usize] = true;
                vertex_needed[triangle.v2 as usize] = true;
                
                // Keep this triangle
                compressed_triangles.push(triangle.clone());
            }
        }
        
        // Step 3: Create compressed vertex vector and index mapping
        let (compressed_vertices, index_mapping) = self.compress_vertices(&vertex_needed);
        
        // Step 4: Update triangle indices to use compressed vertex indices
        let mut final_triangles = Vec::new();
        for triangle in compressed_triangles {
            let new_v0 = index_mapping[triangle.v0 as usize];
            let new_v1 = index_mapping[triangle.v1 as usize];
            let new_v2 = index_mapping[triangle.v2 as usize];
            
            final_triangles.push(Triangle::new(new_v0, new_v1, new_v2, triangle.center_occluded));
        }
        
        // Step 5: Add compressed vertices and triangles to output mesh
        self.add_compressed_to_mesh(output_mesh, material, &compressed_vertices, &final_triangles);
    }

    /// Helper method to compress vertices and create index mapping
    fn compress_vertices(&self, vertex_needed: &[bool]) -> (Vec<&OccludableVertex>, Vec<u32>) {
        let mut compressed_vertices = Vec::new();
        let mut index_mapping = vec![0u32; self.vertices.len()];
        
        for (old_index, &needed) in vertex_needed.iter().enumerate() {
            if needed {
                let new_index = compressed_vertices.len() as u32;
                index_mapping[old_index] = new_index;
                compressed_vertices.push(&self.vertices[old_index]);
            }
        }
        
        (compressed_vertices, index_mapping)
    }

    /// Helper method to add compressed data to the output mesh
    fn add_compressed_to_mesh(
        &self,
        output_mesh: &mut Mesh,
        material: &Material,
        compressed_vertices: &[&OccludableVertex],
        final_triangles: &[Triangle],
    ) {
        // Add all compressed vertices to the output mesh
        let vertex_start_index = output_mesh.vertices.len() as u32;
        
        for occludable_vertex in compressed_vertices {
            let vertex = Vertex::new(
                &occludable_vertex.position,
                &occludable_vertex.normal,
                material,
            );
            output_mesh.add_vertex(vertex);
        }
        
        // Add all triangles to the output mesh
        for triangle in final_triangles {
            output_mesh.add_triangle(
                vertex_start_index + triangle.v0,
                vertex_start_index + triangle.v1,
                vertex_start_index + triangle.v2,
            );
        }
    }
}
