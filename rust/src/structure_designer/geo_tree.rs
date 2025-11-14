use glam::f64::DVec3;
use glam::f64::DVec2;
use crate::util::transform::Transform;
use std::fmt;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::structure_designer::geo_tree::geo_tree_cache::GeoTreeCacheInner;

/*
 * geo_tree is a simple geometry expression tree implementation.
 * It can be implicitly evaluated or converted to polygon representation.
 * Geometry and Geometry2D nodes in an atomCAD node network output this representation.
 */

pub mod geo_tree_cache;

/// Shared geometry node with identity and back-reference to the owning cache.
///
/// Children are shared via `Rc<GeoNode>` so that multiple parents can refer to
/// the same sub-expression. When the last `Rc` goes away, the node's `Drop`
/// implementation notifies the cache via `node_deleted(id)`.
pub struct GeoNode {
    pub id: u64,
    pub kind: GeoNodeKind,
    cache: Weak<RefCell<GeoTreeCacheInner>>, // used for deletion notifications
}

/// The actual geometry variants.
pub enum GeoNodeKind {
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
        shape: Rc<GeoNode>,
    },
    Transform {
        transform: Transform,
        shape: Rc<GeoNode>,
    },
    Union2D {
        shapes: Vec<Rc<GeoNode>>,
    },
    Union3D {
        shapes: Vec<Rc<GeoNode>>,
    },
    Intersection2D {
        shapes: Vec<Rc<GeoNode>>,
    },
    Intersection3D {
        shapes: Vec<Rc<GeoNode>>,
    },
    Difference2D {
        base: Rc<GeoNode>,
        sub: Rc<GeoNode>,
    },
    Difference3D {
        base: Rc<GeoNode>,
        sub: Rc<GeoNode>,
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
        
        match &self.kind {
            GeoNodeKind::HalfSpace { normal, center } => {
                format!("{}HalfSpace(normal: {}, center: {})", 
                    prefix, format_vec3(normal), format_vec3(center))
            }
            GeoNodeKind::HalfPlane { point1, point2 } => {
                format!("{}HalfPlane(p1: {}, p2: {})", 
                    prefix, format_vec2(point1), format_vec2(point2))
            }
            GeoNodeKind::Circle { center, radius } => {
                format!("{}Circle(center: {}, radius: {})", 
                    prefix, format_vec2(center), format_f64(radius))
            }
            GeoNodeKind::Sphere { center, radius } => {
                format!("{}Sphere(center: {}, radius: {})", 
                    prefix, format_vec3(center), format_f64(radius))
            }
            GeoNodeKind::Polygon { vertices } => {
                let mut result = format!("{}Polygon({} vertices)", prefix, vertices.len());
                for (i, vertex) in vertices.iter().enumerate() {
                    result.push_str(&format!("\n{}  [{}]: {}", prefix, i, format_vec2(vertex)));
                }
                result
            }
            GeoNodeKind::Extrude { height, direction, shape } => {
                format!("{}Extrude(height: {}, direction: {})\n{}", 
                    prefix, format_f64(height), format_vec3(direction),
                    shape.display_with_indent(indent + 1))
            }
            GeoNodeKind::Transform { transform, shape } => {
                format!("{}Transform({})\n{}", 
                    prefix, format_transform(transform),
                    shape.display_with_indent(indent + 1))
            }
            GeoNodeKind::Union2D { shapes } => {
                let mut result = format!("{}Union2D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNodeKind::Union3D { shapes } => {
                let mut result = format!("{}Union3D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNodeKind::Intersection2D { shapes } => {
                let mut result = format!("{}Intersection2D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNodeKind::Intersection3D { shapes } => {
                let mut result = format!("{}Intersection3D", prefix);
                for shape in shapes {
                    result.push_str(&format!("\n{}", shape.display_with_indent(indent + 1)));
                }
                result
            }
            GeoNodeKind::Difference2D { base, sub } => {
                format!("{}Difference2D\n{}base:\n{}\n{}sub:\n{}", 
                    prefix, child_prefix, base.display_with_indent(indent + 2),
                    child_prefix, sub.display_with_indent(indent + 2))
            }
            GeoNodeKind::Difference3D { base, sub } => {
                format!("{}Difference3D\n{}base:\n{}\n{}sub:\n{}", 
                    prefix, child_prefix, base.display_with_indent(indent + 2),
                    child_prefix, sub.display_with_indent(indent + 2))
            }
        }
    }
}

impl Drop for GeoNode {
    fn drop(&mut self) {
        if let Some(cache_rc) = self.cache.upgrade() {
            if let Ok(mut inner) = cache_rc.try_borrow_mut() {
                inner.node_deleted(self.id);
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

