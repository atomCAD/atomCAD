use glam::i32::IVec3;
use glam::i32::IVec2;
use crate::util::transform::Transform;

/*
 * geo_tree is a simple geometry expression tree implementation.
 * It can be implicitly evaluated or converted to polygon representation.
 * Geometry and Geometry2D nodes in an atomCAD node network output this representation.
 */
#[derive(Clone)]
 pub enum GeoNode {
  HalfSpace {
    miller_index: IVec3,
    center: IVec3,
    shift: i32,
  },
  HalfPlane {
    point1: IVec2,
    point2: IVec2,
  },
  Circle {
    center: IVec2,
    radius: i32,  
  },
  Sphere {
    center: IVec3,
    radius: i32,
  },
  Rect {
    min_corner: IVec2,
    extent: IVec2,
  },
  Cuboid {
    min_corner: IVec3,
    extent: IVec3,
  },
  Polygon {
    vertices: Vec<IVec2>,
  },
  Extrude {
    height: i32,
    shape: Box<GeoNode>,
  },
  Transform {
    transform: Transform,
    shape: Box<GeoNode>,
  },
  Union2D {
    shapes: Vec<GeoNode>,
  },
  Union3D {
    shapes: Vec<GeoNode>,
  },
  Intersection2D {
    shapes: Vec<GeoNode>,
  },
  Intersection3D {
    shapes: Vec<GeoNode>,
  },
  Difference2D {
    base: Box<GeoNode>,
    sub: Box<GeoNode>
  },
  Difference3D {
    base: Box<GeoNode>,
    sub: Box<GeoNode>
  },
}

mod csg_conversion;
mod implicit_eval;

