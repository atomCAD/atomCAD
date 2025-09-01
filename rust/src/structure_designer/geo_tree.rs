use glam::i32::IVec3;
use glam::i32::IVec2;

/*
 * geo_tree is a simple geometry expression tree implementation.
 * It can be implicitly evaluated or converted to polygon representation.
 * Geometry and Geometry2D nodes in an atomCAD node network output this representation.
 */
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
  Polygon {
    vertices: Vec<IVec2>,
  },
  Extrude {
    z_start: i32,
    z_end: i32,
    shape: Box<GeoNode>,
  },
  Transform {
    translation: IVec3,
    rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees.
    shape: Box<GeoNode>,
  },
  Union {
    shapes: Vec<GeoNode>,
  },
  Intersection {
    shapes: Vec<GeoNode>,
  },
  Difference {
    base: Box<GeoNode>,
    sub: Box<GeoNode>
  },
}
