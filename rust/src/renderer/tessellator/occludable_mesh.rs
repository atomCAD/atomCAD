use glam::Vec3;
use crate::renderer::mesh::{Mesh, Material, Vertex};

/// A vertex that can be marked as occluded during tessellation
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
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

/// Maximum vertices for a sphere (36x18 divisions = ~1224 vertices max)
pub const MAX_VERTICES: usize = 2048;
/// Maximum triangles for a sphere (36x18 divisions = ~2448 triangles max)  
pub const MAX_TRIANGLES: usize = 4096;

/// A mesh structure that supports occlusion marking with pre-allocated buffers
pub struct OccludableMesh {
    // Pre-allocated buffers to avoid memory allocation during tessellation
    vertices: [OccludableVertex; MAX_VERTICES],
    triangles: [Triangle; MAX_TRIANGLES],
    
    // Current usage counters
    vertex_count: usize,
    triangle_count: usize,
}

impl OccludableMesh {
    pub fn new() -> Self {
        Self {
            // Initialize with default values
            vertices: [OccludableVertex::new(Vec3::ZERO, Vec3::Y, false); MAX_VERTICES],
            triangles: [Triangle::new(0, 0, 0, false); MAX_TRIANGLES],
            vertex_count: 0,
            triangle_count: 0,
        }
    }

    /// Reset the mesh for reuse (no memory allocation)
    pub fn reset(&mut self) {
        self.vertex_count = 0;
        self.triangle_count = 0;
    }

    /// Add a vertex and return its index
    pub fn add_vertex(&mut self, vertex: OccludableVertex) -> u32 {
        debug_assert!(self.vertex_count < MAX_VERTICES, "OccludableMesh vertex buffer overflow");
        
        let index = self.vertex_count as u32;
        self.vertices[self.vertex_count] = vertex;
        self.vertex_count += 1;
        index
    }

    /// Add a triangle using three vertex indices and center occlusion status
    pub fn add_triangle(&mut self, v0: u32, v1: u32, v2: u32, center_occluded: bool) {
        debug_assert!(self.triangle_count < MAX_TRIANGLES, "OccludableMesh triangle buffer overflow");
        
        self.triangles[self.triangle_count] = Triangle::new(v0, v1, v2, center_occluded);
        self.triangle_count += 1;
    }

    /// Add a quad using four vertex indices (creates two triangles)
    /// Both triangles will have the same center occlusion status
    pub fn add_quad(&mut self, v0: u32, v1: u32, v2: u32, v3: u32, center_occluded: bool) {
        self.add_triangle(v0, v1, v2, center_occluded);
        self.add_triangle(v2, v3, v0, center_occluded);
    }

    /// Get the number of vertices currently in use
    pub fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    /// Get the number of triangles currently in use
    pub fn triangle_count(&self) -> usize {
        self.triangle_count
    }

    /// Get the total number of indices (triangles * 3)
    pub fn index_count(&self) -> usize {
        self.triangle_count * 3
    }

    /// Get slice of vertices currently in use
    pub fn vertices(&self) -> &[OccludableVertex] {
        &self.vertices[..self.vertex_count]
    }

    /// Get slice of triangles currently in use
    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles[..self.triangle_count]
    }

    /// Get mutable slice of vertices currently in use
    pub fn vertices_mut(&mut self) -> &mut [OccludableVertex] {
        &mut self.vertices[..self.vertex_count]
    }

    /// Get mutable slice of triangles currently in use
    pub fn triangles_mut(&mut self) -> &mut [Triangle] {
        &mut self.triangles[..self.triangle_count]
    }

    /// Add this occludable mesh to the output mesh, handling occlusion compression
    /// Zero-allocation implementation using dual-index compression
    pub fn add_to_mesh(&self, output_mesh: &mut Mesh, material: &Material) {
        // Pre-allocate index mapping array (reuse the vertices array space conceptually)
        let mut index_mapping = [0u32; MAX_VERTICES];
        let mut vertex_needed = [false; MAX_VERTICES];
        
        // Step 1: Mark which vertices are needed by scanning triangles
        let mut kept_triangle_count = 0;
        for i in 0..self.triangle_count {
            let triangle = &self.triangles[i];
            
            // Check if triangle has any non-occluded point (vertices + center)
            let v0_occluded = self.vertices[triangle.v0 as usize].occluded;
            let v1_occluded = self.vertices[triangle.v1 as usize].occluded;
            let v2_occluded = self.vertices[triangle.v2 as usize].occluded;
            let center_occluded = triangle.center_occluded;
            
            // Keep triangle if ANY vertex (including center) is NOT occluded
            let should_keep = !v0_occluded || !v1_occluded || !v2_occluded || !center_occluded;
            
            if should_keep {
                // Mark vertices as needed
                vertex_needed[triangle.v0 as usize] = true;
                vertex_needed[triangle.v1 as usize] = true;
                vertex_needed[triangle.v2 as usize] = true;
                kept_triangle_count += 1;
            }
        }
        
        // Step 2: Build index mapping and add vertices to output mesh
        let vertex_start_index = output_mesh.vertices.len() as u32;
        let mut compressed_vertex_count = 0u32;
        
        for old_index in 0..self.vertex_count {
            if vertex_needed[old_index] {
                index_mapping[old_index] = compressed_vertex_count;
                
                // Add vertex directly to output mesh
                let occludable_vertex = &self.vertices[old_index];
                let vertex = Vertex::new(
                    &occludable_vertex.position,
                    &occludable_vertex.normal,
                    material,
                );
                output_mesh.add_vertex(vertex);
                
                compressed_vertex_count += 1;
            }
        }
        
        // Step 3: Add triangles with remapped indices directly to output mesh
        for i in 0..self.triangle_count {
            let triangle = &self.triangles[i];
            
            // Check if triangle should be kept (same logic as Step 1)
            let v0_occluded = self.vertices[triangle.v0 as usize].occluded;
            let v1_occluded = self.vertices[triangle.v1 as usize].occluded;
            let v2_occluded = self.vertices[triangle.v2 as usize].occluded;
            let center_occluded = triangle.center_occluded;
            
            let should_keep = !v0_occluded || !v1_occluded || !v2_occluded || !center_occluded;
            
            if should_keep {
                // Add triangle with remapped indices
                let new_v0 = vertex_start_index + index_mapping[triangle.v0 as usize];
                let new_v1 = vertex_start_index + index_mapping[triangle.v1 as usize];
                let new_v2 = vertex_start_index + index_mapping[triangle.v2 as usize];
                
                output_mesh.add_triangle(new_v0, new_v1, new_v2);
            }
        }
    }

}
















