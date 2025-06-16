// csg_types.rs
// Central definition for CSG (Constructive Solid Geometry) type

/// Default CSG type with empty metadata.
/// This provides a consistent type alias that can be used throughout the project.
pub type CSG = csgrs::csg::CSG<()>;
