use crate::util::memory_size_estimator::MemorySizeEstimator;
use glam::DVec3;
use std::collections::{HashMap, HashSet};

/// Represents a face in the PolyMesh
pub struct Face {
    /// Indices of the vertices that form this face (CCW order)
    pub vertices: Vec<u32>,
    /// Normal vector for this face
    pub normal: DVec3,
    /// Smoothing group ID for this face (None if not assigned to a smoothing group)
    pub smoothing_group_id: Option<u32>,
    /// Whether this face is highlighted (e.g., for user interaction)
    pub highlighted: bool,
}

/// Represents a vertex in the PolyMesh with position and adjacency information
pub struct Vertex {
    /// Position of the vertex
    pub position: DVec3,
    /// Indices of faces that use this vertex
    pub face_indices: Vec<u32>,
}

/// Represents an edge in the PolyMesh with information about adjacent faces and sharpness
pub struct Edge {
    /// Indices of faces that share this edge
    pub face_indices: HashSet<u32>,
    /// Flag indicating whether this edge is sharp (will be used for smoothing group detection)
    pub is_sharp: bool,
}

/// A specialized polygon mesh representation that enables O(1) access to faces adjacent to a vertex
/// This is an intermediate representation for mesh processing and analysis, particularly useful
/// for detecting sharp features and optimizing vertex positions
pub struct PolyMesh {
    pub open: bool,

    pub hatched: bool,

    /// Vertices of the mesh with their positions and adjacency information
    pub vertices: Vec<Vertex>,
    /// Faces of the mesh
    pub faces: Vec<Face>,
    /// Edges of the mesh, keyed by vertex index pairs (always ordered so first index < second index)
    pub edges: HashMap<(u32, u32), Edge>,
    /// Flag indicating whether face normals are currently valid
    face_normals_valid: bool,
}

impl PolyMesh {
    /// Creates a new empty PolyMesh
    pub fn new(open: bool, hatched: bool) -> Self {
        PolyMesh {
            open,
            hatched,
            vertices: Vec::new(),
            faces: Vec::new(),
            edges: HashMap::new(),
            face_normals_valid: false,
        }
    }

    /// Adds a vertex to the mesh and returns its index
    pub fn add_vertex(&mut self, position: DVec3) -> u32 {
        let vertex_index = self.vertices.len() as u32;
        let vertex = Vertex {
            position,
            face_indices: Vec::new(),
        };
        self.vertices.push(vertex);
        vertex_index
    }

    /// Gets the edge between two vertices, or None if the edge doesn't exist
    fn get_edge(&self, v1: u32, v2: u32) -> Option<&Edge> {
        // Ensure v1 < v2 for consistent key ordering
        let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
        self.edges.get(&key)
    }

    /// Gets the edge between two vertices, creating it if it doesn't exist
    fn get_or_create_edge(&mut self, v1: u32, v2: u32) -> &mut Edge {
        // Ensure v1 < v2 for consistent key ordering
        let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };

        // Create the edge if it doesn't exist
        self.edges.entry(key).or_insert_with(|| Edge {
            face_indices: HashSet::new(),
            is_sharp: false,
        })
    }

    /// Adds a face to the mesh and updates the vertex face adjacency and edge relationships
    /// Takes a vector of vertex indices that define the face
    pub fn add_face(&mut self, vertex_indices: Vec<u32>) -> u32 {
        // Require at least 3 vertices for a face
        assert!(
            vertex_indices.len() >= 3,
            "A face must have at least 3 vertices"
        );

        // Create the face with a default normal (will be computed later)
        let face = Face {
            vertices: vertex_indices.clone(),
            normal: DVec3::ZERO,
            smoothing_group_id: None,
            highlighted: false,
        };

        let face_index = self.faces.len() as u32;
        self.faces.push(face);

        // Update the face indices for each vertex
        for &vertex_index in &vertex_indices {
            // Ensure vertex_index is valid
            if (vertex_index as usize) < self.vertices.len() {
                self.vertices[vertex_index as usize]
                    .face_indices
                    .push(face_index);
            }
        }

        // Update the edges - connect each edge of the face
        let num_vertices = vertex_indices.len();
        for i in 0..num_vertices {
            let j = (i + 1) % num_vertices; // Next vertex in the face
            let edge = self.get_or_create_edge(vertex_indices[i], vertex_indices[j]);
            edge.face_indices.insert(face_index);
        }

        // Adding a face invalidates normals
        self.face_normals_valid = false;

        face_index
    }

    pub fn scale(&mut self, scale: f64) {
        for vertex in &mut self.vertices {
            vertex.position *= scale;
        }
    }

    /// Computes the normal for each face in the mesh
    pub fn compute_face_normals(&mut self) {
        for face in &mut self.faces {
            // Get positions from the vertices
            let v0 = self.vertices[face.vertices[0] as usize].position;
            let v1 = self.vertices[face.vertices[1] as usize].position;
            let v2 = self.vertices[face.vertices[2] as usize].position;

            // Compute two edges of the face
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;

            // Compute normal using cross product
            let normal = edge1.cross(edge2);

            // Normalize if not zero
            if normal.length_squared() > 0.0 {
                face.normal = normal.normalize();
            } else {
                // Default normal if the face is degenerate
                face.normal = DVec3::new(0.0, 0.0, 1.0);
            }
        }

        // Mark normals as valid
        self.face_normals_valid = true;
    }

    /// Detects sharp edges in the mesh based on the angle between adjacent faces
    /// An edge is considered sharp if:
    /// 1. It has less than or more than 2 faces attached to it, or
    /// 2. The angle between the normals of the 2 attached faces exceeds the threshold
    ///
    /// If create_smoothing_groups is true, smoothing groups will be created based on the detected sharp edges
    pub fn detect_sharp_edges(
        &mut self,
        angle_threshold_degrees: f64,
        create_smoothing_groups: bool,
    ) {
        // Ensure we have valid normals
        if !self.face_normals_valid {
            self.compute_face_normals();
        }

        // Convert threshold to radians and calculate its cosine
        let angle_threshold_radians = angle_threshold_degrees.to_radians();
        let cos_threshold = angle_threshold_radians.cos();

        // Check each edge
        for (_, edge) in self.edges.iter_mut() {
            // Clear any previous sharpness flag
            edge.is_sharp = false;

            // Case 1: Non-manifold edge (not exactly 2 faces)
            if edge.face_indices.len() == 1 {
                edge.is_sharp = true;
                continue;
            }

            if edge.face_indices.len() > 2 {
                edge.is_sharp = true;
                continue;
            }

            // Case 2: Check angle between the two faces' normals
            let face_indices: Vec<u32> = edge.face_indices.iter().copied().collect();
            let normal1 = self.faces[face_indices[0] as usize].normal;
            let normal2 = self.faces[face_indices[1] as usize].normal;

            // Calculate dot product between normalized normals
            let dot_product = normal1.dot(normal2);

            // Handle floating point precision issues
            let dot_product = dot_product.clamp(-1.0, 1.0);

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
        for face in &mut self.faces {
            face.smoothing_group_id = None;
        }

        // Current smoothing group ID counter
        let mut next_group_id: u32 = 1;

        // Process all faces
        for face_idx in 0..self.faces.len() {
            let face_id = face_idx as u32;

            // Skip faces that already have a smoothing group assigned
            if self.faces[face_idx].smoothing_group_id.is_some() {
                continue;
            }

            // Assign a new smoothing group ID to this face and flood fill
            self.faces[face_idx].smoothing_group_id = Some(next_group_id);
            self.flood_fill_smoothing_group(face_id, next_group_id);

            // Increment for the next smoothing group
            next_group_id += 1;
        }
    }

    /// Performs a flood-fill starting from the given face to assign smoothing group IDs
    /// Propagates the given smoothing_group_id to all faces connected by non-sharp edges
    fn flood_fill_smoothing_group(&mut self, start_face_id: u32, smoothing_group_id: u32) {
        // Use a stack for depth-first traversal
        let mut stack = vec![start_face_id];

        // Flood-fill algorithm to propagate this smoothing group ID
        while let Some(current_face_id) = stack.pop() {
            // Collect adjacent faces that need to be processed
            let mut adjacent_faces_to_process = Vec::new();

            // Copy the vertices
            let vertices = &self.faces[current_face_id as usize].vertices;

            // First phase: Find all adjacent faces through non-sharp edges (without modifying anything)
            for i in 0..vertices.len() {
                let v1 = vertices[i];
                let v2 = vertices[(i + 1) % vertices.len()]; // Next vertex in the face

                // Get the edge and collect adjacent faces
                let edge = match self.get_edge(v1, v2) {
                    Some(e) => e,
                    None => continue, // Skip if edge doesn't exist
                };

                // Skip sharp edges
                if edge.is_sharp {
                    continue;
                }

                // Find adjacent faces through this non-sharp edge
                for &adjacent_face_id in &edge.face_indices {
                    // Skip the current face and faces that already have this smoothing group
                    if adjacent_face_id == current_face_id
                        || self.faces[adjacent_face_id as usize].smoothing_group_id
                            == Some(smoothing_group_id)
                    {
                        continue;
                    }

                    // Collect for processing
                    adjacent_faces_to_process.push(adjacent_face_id);
                }
            }

            // Second phase: Process all collected faces (now we can modify self.faces)
            for adjacent_face_id in adjacent_faces_to_process {
                // Assign the smoothing group to this adjacent face
                self.faces[adjacent_face_id as usize].smoothing_group_id = Some(smoothing_group_id);

                // Add it to the stack for further processing
                stack.push(adjacent_face_id);
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

    /// Gets a slice of face indices that use this vertex
    pub fn get_vertex_face_indices(&self, vertex_index: u32) -> &[u32] {
        if (vertex_index as usize) < self.vertices.len() {
            &self.vertices[vertex_index as usize].face_indices
        } else {
            &[]
        }
    }

    /// Helper method to add a quad face (4 vertices) for backward compatibility
    pub fn add_quad(&mut self, v0: u32, v1: u32, v2: u32, v3: u32) -> u32 {
        self.add_face(vec![v0, v1, v2, v3])
    }
}

// Memory size estimation implementations

impl MemorySizeEstimator for PolyMesh {
    fn estimate_memory_bytes(&self) -> usize {
        let base_size = std::mem::size_of::<PolyMesh>();

        // Accurately estimate vertices Vec by traversing
        let vertices_size = self
            .vertices
            .iter()
            .map(|v| {
                std::mem::size_of::<Vertex>()
                    + v.face_indices.capacity() * std::mem::size_of::<u32>()
            })
            .sum::<usize>();

        // Accurately estimate faces Vec by traversing
        let faces_size = self
            .faces
            .iter()
            .map(|f| {
                std::mem::size_of::<Face>() + f.vertices.capacity() * std::mem::size_of::<u32>()
            })
            .sum::<usize>();

        // Accurately estimate edges HashMap by traversing
        let edges_size = self
            .edges
            .values()
            .map(|edge| {
                std::mem::size_of::<(u32, u32)>()
                    + std::mem::size_of::<Edge>()
                    + edge.face_indices.capacity() * std::mem::size_of::<u32>()
            })
            .sum::<usize>();

        base_size + vertices_size + faces_size + edges_size
    }
}
