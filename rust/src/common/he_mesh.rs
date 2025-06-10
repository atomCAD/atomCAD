use glam::f64::DVec3;

/// Strongly‐typed index handles
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct VertexId(pub usize);
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct HalfEdgeId(pub usize);
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FaceId(pub usize);

/// The mesh container
pub struct HEMesh {
    pub vertices:   Vec<Vertex>,
    pub half_edges: Vec<HalfEdge>,
    pub faces:      Vec<Face>,
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
    // … optionally user‐data, edge‐properties, flags, etc …
}

/// One record per polygonal face.
pub struct Face {
    /// An arbitrary half‐edge on this face.
    /// To list all vertices of the face, follow `.next` until you cycle.
    pub half_edge: HalfEdgeId,
    pub normal: DVec3, // Computed by HEMesh::compute_face_normals()
}