// csg_types.rs
// Central definition for CSG (Constructive Solid Geometry) types

/// Default CSG types with empty metadata.
/// This provides a consistent type alias that can be used throughout the project.
pub type CSGMesh = csgrs::mesh::Mesh<()>;
pub type CSGSketch = csgrs::sketch::Sketch<()>;