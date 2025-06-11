use glam::DVec3;
use std::collections::{HashMap, HashSet};

/// Represents a quad face in the QuadMesh
pub struct Quad {
    /// Indices of the four vertices that form this quad (CCW order)
    pub vertices: [u32; 4],
    /// Normal vector for this quad
    pub normal: DVec3,
    /// Smoothing group ID for this quad (None if not assigned to a smoothing group)
    pub smoothing_group_id: Option<u32>,
}

/// Represents a vertex in the QuadMesh with position and adjacency information
pub struct QuadVertex {
    /// Position of the vertex
    pub position: DVec3,
    /// Indices of quads that use this vertex
    pub quad_indices: Vec<u32>,
}

/// Represents an edge in the QuadMesh with information about adjacent quads and sharpness
pub struct QuadEdge {
    /// Indices of quads that share this edge
    pub quad_indices: HashSet<u32>,
    /// Flag indicating whether this edge is sharp (will be used for smoothing group detection)
    pub is_sharp: bool,
}

/// A specialized quad mesh representation that enables O(1) access to faces adjacent to a vertex
/// This is an intermediate representation for mesh processing and analysis, particularly useful
/// for detecting sharp features and optimizing vertex positions
pub struct QuadMesh {
    /// Vertices of the mesh with their positions and adjacency information
    pub vertices: Vec<QuadVertex>,
    /// Quad faces of the mesh
    pub quads: Vec<Quad>,
    /// Edges of the mesh, keyed by vertex index pairs (always ordered so first index < second index)
    pub edges: HashMap<(u32, u32), QuadEdge>,
    /// Flag indicating whether quad normals are currently valid
    quad_normals_valid: bool,
}

impl QuadMesh {
    /// Creates a new empty QuadMesh
    pub fn new() -> Self {
        QuadMesh {
            vertices: Vec::new(),
            quads: Vec::new(),
            edges: HashMap::new(),
            quad_normals_valid: false,
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

    /// Gets the edge between two vertices, or None if the edge doesn't exist
    fn get_edge(&self, v1: u32, v2: u32) -> Option<&QuadEdge> {
        // Ensure v1 < v2 for consistent key ordering
        let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
        self.edges.get(&key)
    }

    /// Gets the edge between two vertices, creating it if it doesn't exist
    fn get_or_create_edge(&mut self, v1: u32, v2: u32) -> &mut QuadEdge {
        // Ensure v1 < v2 for consistent key ordering
        let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
        
        // Create the edge if it doesn't exist
        self.edges.entry(key).or_insert_with(|| QuadEdge {
            quad_indices: HashSet::new(),
            is_sharp: false,
        })
    }

    /// Adds a quad to the mesh and updates the vertex quad adjacency and edge relationships
    pub fn add_quad(&mut self, v0: u32, v1: u32, v2: u32, v3: u32) -> u32 {
        // Create the quad with a default normal (will be computed later)
        let quad = Quad {
            vertices: [v0, v1, v2, v3],
            normal: DVec3::ZERO,
            smoothing_group_id: None,
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
        
        // Update the edges - connect each edge of the quad
        let vertices = [v0, v1, v2, v3];
        for i in 0..4 {
            let j = (i + 1) % 4; // Next vertex in the quad
            let edge = self.get_or_create_edge(vertices[i], vertices[j]);
            edge.quad_indices.insert(quad_index);
        }
        
        // Adding a quad invalidates normals
        self.quad_normals_valid = false;
        
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
        
        // Mark normals as valid
        self.quad_normals_valid = true;
    }
    
    /// Detects sharp edges in the mesh based on the angle between adjacent quads
    /// An edge is considered sharp if:
    /// 1. It has less than or more than 2 quads attached to it, or
    /// 2. The angle between the normals of the 2 attached quads exceeds the threshold
    /// 
    /// If create_smoothing_groups is true, smoothing groups will be created based on the detected sharp edges
    pub fn detect_sharp_edges(&mut self, angle_threshold_degrees: f64, create_smoothing_groups: bool) {
        // Ensure we have valid normals
        if !self.quad_normals_valid {
            self.compute_quad_normals();
        }
        
        // Convert threshold to radians and calculate its cosine
        let angle_threshold_radians = angle_threshold_degrees.to_radians();
        let cos_threshold = angle_threshold_radians.cos();
        
        // Check each edge
        for (_, edge) in self.edges.iter_mut() {
            // Clear any previous sharpness flag
            edge.is_sharp = false;
            
            // Case 1: Non-manifold edge (not exactly 2 quads)
            if edge.quad_indices.len() != 2 {
                edge.is_sharp = true;
                continue;
            }
            
            // Case 2: Check angle between the two quads' normals
            let quad_indices: Vec<u32> = edge.quad_indices.iter().copied().collect();
            let normal1 = self.quads[quad_indices[0] as usize].normal;
            let normal2 = self.quads[quad_indices[1] as usize].normal;
            
            // Calculate dot product between normalized normals
            let dot_product = normal1.dot(normal2);
            
            // Handle floating point precision issues
            let dot_product = dot_product.max(-1.0).min(1.0);
            
            // Set edge as sharp if angle exceeds threshold
            if dot_product < cos_threshold {
                edge.is_sharp = true;
            }
        }
        
        // Optionally create smoothing groups based on the detected sharp edges
        if create_smoothing_groups {
            self.create_smoothing_groups();
        }
    }
    
    /// Creates smoothing groups based on previously detected sharp edges
    /// Faces connected by non-sharp edges will be assigned to the same smoothing group
    fn create_smoothing_groups(&mut self) {
        // Reset all smoothing group IDs
        for quad in &mut self.quads {
            quad.smoothing_group_id = None;
        }
        
        // Current smoothing group ID counter
        let mut next_group_id: u32 = 1;
        
        // Process all quads
        for quad_idx in 0..self.quads.len() {
            let quad_id = quad_idx as u32;
            
            // Skip quads that already have a smoothing group assigned
            if self.quads[quad_idx].smoothing_group_id.is_some() {
                continue;
            }
            
            // Assign a new smoothing group ID to this quad and flood fill
            self.quads[quad_idx].smoothing_group_id = Some(next_group_id);
            self.flood_fill_smoothing_group(quad_id, next_group_id);
            
            // Increment for the next smoothing group
            next_group_id += 1;
        }
    }
    
    /// Performs a flood-fill starting from the given quad to assign smoothing group IDs
    /// Propagates the given smoothing_group_id to all quads connected by non-sharp edges
    fn flood_fill_smoothing_group(&mut self, start_quad_id: u32, smoothing_group_id: u32) {
        // Use a stack for depth-first traversal
        let mut stack = vec![start_quad_id];
        
        // Flood-fill algorithm to propagate this smoothing group ID
        while let Some(current_quad_id) = stack.pop() {
            // Collect adjacent quads that need to be processed
            let mut adjacent_quads_to_process = Vec::new();
            
            // Copy the vertices
            let vertices = self.quads[current_quad_id as usize].vertices;
            
            // First phase: Find all adjacent quads through non-sharp edges (without modifying anything)
            for i in 0..4 {
                let v1 = vertices[i];
                let v2 = vertices[(i + 1) % 4]; // Next vertex in the quad
                
                // Get the edge and collect adjacent quads
                let edge = match self.get_edge(v1, v2) {
                    Some(e) => e,
                    None => continue,  // Skip if edge doesn't exist
                };
                
                // Skip sharp edges
                if edge.is_sharp {
                    continue;
                }
                
                // Find adjacent quads through this non-sharp edge
                for &adjacent_quad_id in &edge.quad_indices {
                    // Skip the current quad and quads that already have this smoothing group
                    if adjacent_quad_id == current_quad_id || 
                       self.quads[adjacent_quad_id as usize].smoothing_group_id == Some(smoothing_group_id) {
                        continue;
                    }
                    
                    // Collect for processing
                    adjacent_quads_to_process.push(adjacent_quad_id);
                }
            }
            
            // Second phase: Process all collected quads (now we can modify self.quads)
            for adjacent_quad_id in adjacent_quads_to_process {
                // Assign the smoothing group to this adjacent quad
                self.quads[adjacent_quad_id as usize].smoothing_group_id = Some(smoothing_group_id);
                
                // Add it to the stack for further processing
                stack.push(adjacent_quad_id);
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
}
