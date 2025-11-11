use glam::f64::DVec3;
use glam::f64::DVec2;
use crate::util::transform::Transform;
use std::fmt;

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
    // inside is to the left of the line defined by point1 -> point2
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
pub mod batched_implicit_evaluator;

impl fmt::Display for GeoNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_with_indent(0))
    }
}

impl GeoNode {
    fn display_with_indent(&self, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        let child_prefix = "  ".repeat(indent + 1);
        
        match self {
            GeoNode::HalfSpace { normal, center } => {
                format!("{}HalfSpace(normal: {}, center: {})", 
                    prefix, format_vec3(normal), format_vec3(center))
            }
            GeoNode::HalfPlane { point1, point2 } => {
                format!("{}HalfPlane(p1: {}, p2: {})", 
                    prefix, format_vec2(point1), format_vec2(point2))
            }
            GeoNode::Circle { center, radius } => {
                format!("{}Circle(center: {}, radius: {})", 
                    prefix, format_vec2(center), format_f64(radius))
            }
            GeoNode::Sphere { center, radius } => {
                format!("{}Sphere(center: {}, radius: {})", 
                    prefix, format_vec3(center), format_f64(radius))
            }
            GeoNode::Polygon { vertices } => {
                let mut result = format!("{}Polygon({} vertices)", prefix, vertices.len());
                for (i, vertex) in vertices.iter().enumerate() {
                    result.push_str(&format!("\n{}  [{}]: {}", prefix, i, format_vec2(vertex)));
                }
                result
            }
            GeoNode::Extrude { height, direction, shape } => {
                format!("{}Extrude(height: {}, direction: {})\n{}", 
                    prefix, format_f64(height), format_vec3(direction),
                    shape.display_with_indent(indent + 1))
            }
            GeoNode::Transform { transform, shape } => {
                format!("{}Transform({})\n{}", 
                    prefix, format_transform(transform),
                    shape.display_with_indent(indent + 1))
            }
            GeoNode::Union2D { shapes } => {
                let mut result = format!("{}Union2D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNode::Union3D { shapes } => {
                let mut result = format!("{}Union3D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNode::Intersection2D { shapes } => {
                let mut result = format!("{}Intersection2D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNode::Intersection3D { shapes } => {
                let mut result = format!("{}Intersection3D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNode::Difference2D { base, sub } => {
                format!("{}Difference2D\n{}base:\n{}\n{}sub:\n{}", 
                    prefix, child_prefix, base.display_with_indent(indent + 2),
                    child_prefix, sub.display_with_indent(indent + 2))
            }
            GeoNode::Difference3D { base, sub } => {
                format!("{}Difference3D\n{}base:\n{}\n{}sub:\n{}", 
                    prefix, child_prefix, base.display_with_indent(indent + 2),
                    child_prefix, sub.display_with_indent(indent + 2))
            }
        }
    }
}

// Helper functions for formatting
fn format_vec2(v: &DVec2) -> String {
    format!("({}, {})", format_f64(&v.x), format_f64(&v.y))
}

fn format_vec3(v: &DVec3) -> String {
    format!("({}, {}, {})", format_f64(&v.x), format_f64(&v.y), format_f64(&v.z))
}

fn format_f64(f: &f64) -> String {
    if f.fract() == 0.0 {
        format!("{}", *f as i64)
    } else {
        format!("{:.2}", f).trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn format_transform(transform: &Transform) -> String {
    // Simplified transform display - you might want to expand this based on Transform's structure
    format!("translation: {}", format_vec3(&transform.translation))
}

