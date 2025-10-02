use glam::f64::DVec3;
use glam::f64::DVec2;
use crate::util::transform::Transform;

/*
 * geo_tree is a simple geometry expression tree implementation.
 * It can be implicitly evaluated or converted to polygon representation.
 * Geometry and Geometry2D nodes in an atomCAD node network output this representation.
 */
#[derive(Clone)]
 pub enum GeoNode {
  HalfSpace {
    normal: DVec3,
    center: DVec3,
  },
  HalfPlane {
    point1: DVec2,
    point2: DVec2,
  },
  Circle {
    center: DVec2,
    radius: f64,  
  },
  Sphere {
    center: DVec3,
    radius: f64,
  },
  Polygon {
    vertices: Vec<DVec2>,
  },
  Extrude {
    height: f64,
    direction: DVec3,
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

