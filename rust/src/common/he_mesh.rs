use glam::f64::DVec3;
use std::collections::HashMap;
use std::hash::Hash;

/// Strongly‐typed index handles
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VertexId(pub usize);
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct HalfEdgeId(pub usize);
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct FaceId(pub usize);

/// The mesh container
pub struct HEMesh {
    pub vertices:   Vec<Vertex>,
    pub half_edges: Vec<HalfEdge>,
    pub faces:      Vec<Face>,
    /// Maps vertex pairs (origin, target) to the half-edge that connects them
    /// This enables O(1) lookup of half-edges and their twins
    pub edge_map: HashMap<(VertexId, VertexId), HalfEdgeId>,
    /// Tracks whether face normals have been computed
    pub face_normals_computed: bool,
}

/// One record per mesh‐vertex.
pub struct Vertex {
    /// An arbitrary outgoing half‐edge from this vertex.
    /// Use this to walk incident faces/edges.
    pub half_edge: Option<HalfEdgeId>,
    pub position: DVec3,
}

/// The directed edge record.
pub struct HalfEdge {
    /// The vertex this half‐edge points _from_.
    pub origin: VertexId,
    /// The face to the left of this half‐edge.
    pub face:   FaceId,
    /// Next CCW half‐edge around the same face.
    pub next:   HalfEdgeId,
    /// The opposite half‐edge (same undirected edge, opposite direction).
    pub twin:   HalfEdgeId,
    /// Flag indicating whether this edge is sharp (used for smooth shading)
    pub is_sharp: bool,
    // … optionally user‐data, edge‐properties, flags, etc …
}

/// One record per polygonal face.
pub struct Face {
    /// An arbitrary half‐edge on this face.
    /// To list all vertices of the face, follow `.next` until you cycle.
    pub half_edge: HalfEdgeId,
    pub normal: DVec3, // Computed by HEMesh::compute_face_normals()
}

impl HEMesh {
    /// Creates a new empty HEMesh
    pub fn new() -> Self {
        HEMesh {
            vertices: Vec::new(),
            half_edges: Vec::new(),
            faces: Vec::new(),
            edge_map: HashMap::new(),
            face_normals_computed: false,
        }
    }

    /// Adds a vertex to the mesh and returns its ID
    pub fn add_vertex(&mut self, position: DVec3) -> VertexId {
        let vertex_id = VertexId(self.vertices.len());
        let vertex = Vertex {
            half_edge: None,
            position,
        };
        self.vertices.push(vertex);
        vertex_id
    }

    /// Adds a quad face to the mesh and returns its ID
    /// 
    /// The vertices should be provided in CCW order
    pub fn add_quad(&mut self, v0: VertexId, v1: VertexId, v2: VertexId, v3: VertexId) -> FaceId {
        let face_id = FaceId(self.faces.len());
        
        // Create the four half-edges for this quad
        let he0_id = HalfEdgeId(self.half_edges.len());
        let he1_id = HalfEdgeId(self.half_edges.len() + 1);
        let he2_id = HalfEdgeId(self.half_edges.len() + 2);
        let he3_id = HalfEdgeId(self.half_edges.len() + 3);
        
        // Create half-edges
        let he0 = HalfEdge {
            origin: v0,
            face: face_id,
            next: he1_id,
            twin: HalfEdgeId(0), // Temporary value, will be updated later
            is_sharp: false,
        };
        
        let he1 = HalfEdge {
            origin: v1,
            face: face_id,
            next: he2_id,
            twin: HalfEdgeId(0), // Temporary value, will be updated later
            is_sharp: false,
        };
        
        let he2 = HalfEdge {
            origin: v2,
            face: face_id,
            next: he3_id,
            twin: HalfEdgeId(0), // Temporary value, will be updated later
            is_sharp: false,
        };
        
        let he3 = HalfEdge {
            origin: v3,
            face: face_id,
            next: he0_id,
            twin: HalfEdgeId(0), // Temporary value, will be updated later
            is_sharp: false,
        };
        
        // Add half-edges to the mesh
        self.half_edges.push(he0);
        self.half_edges.push(he1);
        self.half_edges.push(he2);
        self.half_edges.push(he3);
        
        // Create face with reference to one of its half-edges
        let face = Face {
            half_edge: he0_id,
            normal: DVec3::ZERO, // Will be computed later
        };
        
        // Add face to the mesh
        self.faces.push(face);
        
        // When adding a quad, face normals need to be recomputed
        self.face_normals_computed = false;
        
        // Update vertex references to the half-edges
        self.vertices[v0.0].half_edge = Some(he0_id);
        self.vertices[v1.0].half_edge = Some(he1_id);
        self.vertices[v2.0].half_edge = Some(he2_id);
        self.vertices[v3.0].half_edge = Some(he3_id);
        
        // Find or create twins for each half-edge
        let vertex_pairs = [(v0, v1), (v1, v2), (v2, v3), (v3, v0)];
        let half_edge_ids = [he0_id, he1_id, he2_id, he3_id];
        
        for i in 0..4 {
            let (from, to) = vertex_pairs[i];
            let he_id = half_edge_ids[i];
            
            // Store the half-edge in the map
            self.edge_map.insert((from, to), he_id);
            
            // Check if the twin already exists in the map
            if let Some(&twin_id) = self.edge_map.get(&(to, from)) {
                // Update twin references in both half-edges
                self.half_edges[he_id.0].twin = twin_id;
                self.half_edges[twin_id.0].twin = he_id;
            }
        }
        
        face_id
    }
    
    /// Computes normals for each face in the mesh, handling non-coplanar faces
    /// by averaging triangle normals within the face
    pub fn compute_face_normals(&mut self) {

        for face_id in 0..self.faces.len() {
            // Get the half-edge associated with this face
            let start_he_id = self.faces[face_id].half_edge;
            
            // First, collect all vertex positions for this face
            let mut vertices = Vec::new();
            let mut current_he_id = start_he_id;
            
            // We'll use a safety counter to prevent infinite loops
            let max_iterations = self.half_edges.len();
            let mut iterations = 0;
            
            // Collect vertices by following the half-edges around the face
            loop {
                let origin = self.half_edges[current_he_id.0].origin;
                vertices.push(self.vertices[origin.0].position);
                
                current_he_id = self.half_edges[current_he_id.0].next;
                iterations += 1;
                
                if current_he_id == start_he_id || iterations >= max_iterations {
                    break;
                }
            }
            
            // We need at least 3 vertices to compute a normal
            if vertices.len() < 3 {
                self.faces[face_id].normal = DVec3::new(0.0, 0.0, 1.0);
                continue;
            }
            
            // For non-coplanar faces, we'll use a weighted average of triangle normals
            // using the first vertex as a fan pivot
            let v0 = vertices[0];
            let mut normal_sum = DVec3::ZERO;
            let mut total_weight = 0.0;
            
            // Triangulate the face as a fan from the first vertex
            // and accumulate weighted normals
            for i in 1..vertices.len() - 1 {
                let v1 = vertices[i];
                let v2 = vertices[i + 1];
                
                // Compute edges for this triangle
                let edge1 = v1 - v0;
                let edge2 = v2 - v0;
                
                // Cross product gives normal * 2 * area
                let normal = edge1.cross(edge2);
                let area = normal.length() * 0.5;
                
                // Add this triangle's contribution (weighted by area)
                if area > 1e-10 {
                    normal_sum += normal;
                    total_weight += area;
                }
            }
            
            // Normalize and store the result
            if total_weight > 0.0 && normal_sum.length_squared() > 0.0 {
                self.faces[face_id].normal = normal_sum.normalize();
            } else {
                // Default normal if the face is degenerate
                self.faces[face_id].normal = DVec3::new(0.0, 0.0, 1.0);
            }
        }
        
        // Set flag to indicate normals have been computed
        self.face_normals_computed = true;
    }
    
    /// Scales the entire mesh by the provided scale factor
    pub fn scale(&mut self, scale: f64) {
        for vertex in &mut self.vertices {
            vertex.position *= scale;
        }
        // Uniform scaling doesn't affect normals, so we don't need to recompute them
    }
    
    /// Detects sharp edges based on the angle between adjacent face normals
    /// 
    /// # Arguments
    /// * `angle_threshold_degrees` - The minimum angle (in degrees) between face normals
    ///                               for an edge to be considered sharp
    pub fn detect_sharp_edges(&mut self, angle_threshold_degrees: f64) {
        // Ensure face normals are computed
        if !self.face_normals_computed {
            self.compute_face_normals();
        }
        
        // Convert threshold to radians and calculate the cosine threshold
        // (We'll compare cosines directly to avoid expensive acos operations)
        let angle_threshold_radians = angle_threshold_degrees.to_radians();
        let cos_threshold = angle_threshold_radians.cos();
        
        // Process each half-edge
        for he_id in 0..self.half_edges.len() {
            let he_id = HalfEdgeId(he_id);
            let twin_id = self.half_edges[he_id.0].twin;
            
            // Skip if this edge has already been processed
            if he_id.0 < twin_id.0 {
                // Get face normals on both sides of the edge
                let face_id = self.half_edges[he_id.0].face;
                let twin_face_id = self.half_edges[twin_id.0].face;
                
                let face_normal = self.faces[face_id.0].normal;
                let twin_face_normal = self.faces[twin_face_id.0].normal;
                
                // Calculate the cosine of the angle between the normals
                let cos_angle = face_normal.dot(twin_face_normal);
                
                // If the cosine is less than the threshold, the angle is greater than the threshold
                // (Note: cosine decreases as angle increases)
                let is_sharp = cos_angle < cos_threshold;
                
                // Mark both half-edges with the result
                self.half_edges[he_id.0].is_sharp = is_sharp;
                self.half_edges[twin_id.0].is_sharp = is_sharp;
            }
        }
    }
}
