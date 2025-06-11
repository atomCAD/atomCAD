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

    /// Converts this QuadMesh into an existing Mesh with smooth normals (averaged from adjacent faces)
    /// 
    /// # Arguments
    /// * `mesh` - The target mesh to add vertices and faces to
    /// * `material` - The material to apply to the mesh vertices
    fn convert_into_mesh_smooth(&self, mesh: &mut Mesh, material: &Material) {
        // First calculate normal for each vertex by averaging adjacent quad normals
        let mut vertex_normals: Vec<Vec3> = vec![Vec3::ZERO; self.vertices.len()];
        
        for (vertex_idx, vertex) in self.vertices.iter().enumerate() {
            let mut normal_sum = DVec3::ZERO;
            
            // Sum up the normals of all quads that use this vertex
            for &quad_idx in &vertex.quad_indices {
                normal_sum += self.quads[quad_idx as usize].normal;
            }
            
            // Normalize the result if not zero
            if normal_sum.length_squared() > 0.0 {
                normal_sum = normal_sum.normalize();
            }
            
            // Convert from DVec3 to Vec3 for the renderer
            vertex_normals[vertex_idx] = self.dvec3_to_vec3(&normal_sum);
        }
        
        // Add all vertices to the mesh
        let vertex_indices: Vec<u32> = self.vertices.iter().enumerate().map(|(idx, vertex)| {
            let position = self.dvec3_to_vec3(&vertex.position);
            let normal = vertex_normals[idx];
            mesh.add_vertex(Vertex::new(&position, &normal, material))
        }).collect();
        
        // Add all quads (as two triangles) to the mesh
        for quad in &self.quads {
            mesh.add_quad(
                vertex_indices[quad.vertices[0] as usize],
                vertex_indices[quad.vertices[1] as usize],
                vertex_indices[quad.vertices[2] as usize],
                vertex_indices[quad.vertices[3] as usize]
            );
        }
    }
    
    /// Converts this QuadMesh into an existing Mesh with sharp edges (no normal averaging)
    /// 
    /// # Arguments
    /// * `mesh` - The target mesh to add vertices and faces to
    /// * `material` - The material to apply to the mesh vertices
    fn convert_into_mesh_sharp(&self, mesh: &mut Mesh, material: &Material) {
        // Process each quad
        
        // Sharp version: duplicate vertices for each quad
        for quad in &self.quads {
            // Create a normal for this quad's vertices
            let normal = self.dvec3_to_vec3(&quad.normal);
            
            // Create four unique vertices for this quad, all with the same normal
            let v0_idx = mesh.add_vertex(Vertex::new(
                &self.dvec3_to_vec3(&self.vertices[quad.vertices[0] as usize].position),
                &normal,
                material
            ));
            
            let v1_idx = mesh.add_vertex(Vertex::new(
                &self.dvec3_to_vec3(&self.vertices[quad.vertices[1] as usize].position),
                &normal,
                material
            ));
            
            let v2_idx = mesh.add_vertex(Vertex::new(
                &self.dvec3_to_vec3(&self.vertices[quad.vertices[2] as usize].position),
                &normal,
                material
            ));
            
            let v3_idx = mesh.add_vertex(Vertex::new(
                &self.dvec3_to_vec3(&self.vertices[quad.vertices[3] as usize].position),
                &normal,
                material
            ));
            
            // Add the quad (as two triangles) to the mesh
            mesh.add_quad(v0_idx, v1_idx, v2_idx, v3_idx);

        }
    }

    /// Converts this QuadMesh into an existing Mesh
    /// 
    /// # Arguments
    /// * `mesh` - The target mesh to add vertices and faces to
    /// * `smooth` - If true, vertex normals are averaged from adjacent face normals.
    ///              If false, each quad gets its own set of vertices with the quad's normal.
    /// * `material` - The material to apply to the mesh vertices
    pub fn convert_into_mesh_simple(&self, mesh: &mut Mesh, smooth: bool, material: &Material) {
        if smooth {
            self.convert_into_mesh_smooth(mesh, material);
        } else {
            self.convert_into_mesh_sharp(mesh, material);
        }
    }
    
    /// Creates a new rendering Mesh from this QuadMesh
    /// 
    /// # Arguments
    /// * `smooth` - If true, vertex normals are averaged from adjacent face normals.
    ///              If false, each quad gets its own set of vertices with the quad's normal.
    /// * `material` - The material to apply to the mesh vertices
    /// 
    /// # Returns
    /// A new Mesh suitable for rendering
    pub fn create_mesh_simple(&self, smooth: bool, material: &Material) -> Mesh {
        let mut mesh = Mesh::new();
        self.convert_into_mesh_simple(&mut mesh, smooth, material);
        mesh
    }
}
