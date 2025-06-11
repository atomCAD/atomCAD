use glam::DVec3;
use crate::renderer::mesh::{Mesh, Vertex, Material};
use glam::Vec3;

/// Represents a quad face in the QuadMesh
pub struct Quad {
    /// Indices of the four vertices that form this quad (CCW order)
    pub vertices: [u32; 4],
    /// Normal vector for this quad
    pub normal: DVec3,
}

/// Represents a vertex in the QuadMesh with position and adjacency information
pub struct QuadVertex {
    /// Position of the vertex
    pub position: DVec3,
    /// Indices of quads that use this vertex
    pub quad_indices: Vec<u32>,
}

/// A specialized quad mesh representation that enables O(1) access to faces adjacent to a vertex
/// This is an intermediate representation for mesh processing and analysis, particularly useful
/// for detecting sharp features and optimizing vertex positions
pub struct QuadMesh {
    /// Vertices of the mesh with their positions and adjacency information
    pub vertices: Vec<QuadVertex>,
    /// Quad faces of the mesh
    pub quads: Vec<Quad>,
}

impl QuadMesh {
    /// Creates a new empty QuadMesh
    pub fn new() -> Self {
        QuadMesh {
            vertices: Vec::new(),
            quads: Vec::new(),
        }
    }

    /// Adds a vertex to the mesh and returns its index
    pub fn add_vertex(&mut self, position: DVec3) -> u32 {
        let vertex_index = self.vertices.len() as u32;
        let vertex = QuadVertex {
            position,
            quad_indices: Vec::new(),
        };
        self.vertices.push(vertex);
        vertex_index
    }

    /// Adds a quad to the mesh and updates the vertex quad adjacency
    pub fn add_quad(&mut self, v0: u32, v1: u32, v2: u32, v3: u32) -> u32 {
        // Create the quad with a default normal (will be computed later)
        let quad = Quad {
            vertices: [v0, v1, v2, v3],
            normal: DVec3::ZERO,
        };
        
        let quad_index = self.quads.len() as u32;
        self.quads.push(quad);
        
        // Update the quad indices for each vertex
        for &vertex_index in &[v0, v1, v2, v3] {
            // Ensure vertex_index is valid
            if (vertex_index as usize) < self.vertices.len() {
                self.vertices[vertex_index as usize].quad_indices.push(quad_index);
            }
        }
        
        quad_index
    }

    pub fn scale(&mut self, scale: f64) {
        for vertex in &mut self.vertices {
            vertex.position *= scale;
        }
    }

    /// Computes the normal for each quad in the mesh
    pub fn compute_quad_normals(&mut self) {
        for quad in &mut self.quads {
            // Get positions from the vertices
            let v0 = self.vertices[quad.vertices[0] as usize].position;
            let v1 = self.vertices[quad.vertices[1] as usize].position;
            let v2 = self.vertices[quad.vertices[2] as usize].position;
            
            // Compute two edges of the quad
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            
            // Compute normal using cross product
            let normal = edge1.cross(edge2);
            
            // Normalize if not zero
            if normal.length_squared() > 0.0 {
                quad.normal = normal.normalize();
            } else {
                // Default normal if the quad is degenerate
                quad.normal = DVec3::new(0.0, 0.0, 1.0);
            }
        }
    }
    
    /// Returns a reference to the position of a vertex
    pub fn get_vertex_position(&self, index: u32) -> Option<&DVec3> {
        self.vertices.get(index as usize).map(|v| &v.position)
    }
    
    /// Sets the position of a vertex
    pub fn set_vertex_position(&mut self, index: u32, position: DVec3) {
        if (index as usize) < self.vertices.len() {
            self.vertices[index as usize].position = position;
        }
    }
    
    /// Gets a slice of quad indices that use this vertex
    pub fn get_vertex_quad_indices(&self, vertex_index: u32) -> &[u32] {
        if (vertex_index as usize) < self.vertices.len() {
            &self.vertices[vertex_index as usize].quad_indices
        } else {
            &[]
        }
    }
    
    /// Helper function to convert a DVec3 to Vec3 for rendering
    fn dvec3_to_vec3(&self, vec: &DVec3) -> Vec3 {
        Vec3::new(vec.x as f32, vec.y as f32, vec.z as f32)
    }
}
