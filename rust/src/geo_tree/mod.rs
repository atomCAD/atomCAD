use glam::f64::DVec3;
use glam::f64::DVec2;
use crate::util::transform::Transform;
use std::fmt;
use blake3;
use crate::util::memory_size_estimator::MemorySizeEstimator;

/*
 * geo_tree is a simple geometry expression tree implementation.
 * It can be implicitly evaluated or converted to polygon representation.
 * Geometry and Geometry2D nodes in an atomCAD node network output this representation.
 */
#[derive(Clone)]
pub struct GeoNode {
    kind: GeoNodeKind,
    hash: blake3::Hash,
}

#[derive(Clone)]
enum GeoNodeKind {
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
pub mod csg_cache;
pub mod csg_types;
pub mod csg_utils;
pub mod implicit_geometry;

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

    // Public accessor for the precomputed hash
    pub fn hash(&self) -> &blake3::Hash {
        &self.hash
    }

    // Constructor methods for all GeoNode variants
    // Each computes the hash at construction time

    pub fn half_space(normal: DVec3, center: DVec3) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x01]); // variant tag
        hasher.update(&normal.x.to_le_bytes());
        hasher.update(&normal.y.to_le_bytes());
        hasher.update(&normal.z.to_le_bytes());
        hasher.update(&center.x.to_le_bytes());
        hasher.update(&center.y.to_le_bytes());
        hasher.update(&center.z.to_le_bytes());
        
        Self {
            kind: GeoNodeKind::HalfSpace { normal, center },
            hash: hasher.finalize(),
        }
    }

    pub fn half_plane(point1: DVec2, point2: DVec2) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x02]); // variant tag
        hasher.update(&point1.x.to_le_bytes());
        hasher.update(&point1.y.to_le_bytes());
        hasher.update(&point2.x.to_le_bytes());
        hasher.update(&point2.y.to_le_bytes());
        
        Self {
            kind: GeoNodeKind::HalfPlane { point1, point2 },
            hash: hasher.finalize(),
        }
    }

    pub fn circle(center: DVec2, radius: f64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x03]); // variant tag
        hasher.update(&center.x.to_le_bytes());
        hasher.update(&center.y.to_le_bytes());
        hasher.update(&radius.to_le_bytes());
        
        Self {
            kind: GeoNodeKind::Circle { center, radius },
            hash: hasher.finalize(),
        }
    }

    pub fn sphere(center: DVec3, radius: f64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x04]); // variant tag
        hasher.update(&center.x.to_le_bytes());
        hasher.update(&center.y.to_le_bytes());
        hasher.update(&center.z.to_le_bytes());
        hasher.update(&radius.to_le_bytes());
        
        Self {
            kind: GeoNodeKind::Sphere { center, radius },
            hash: hasher.finalize(),
        }
    }

    pub fn polygon(vertices: Vec<DVec2>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x05]); // variant tag
        hasher.update(&(vertices.len() as u32).to_le_bytes());
        for v in &vertices {
            hasher.update(&v.x.to_le_bytes());
            hasher.update(&v.y.to_le_bytes());
        }
        
        Self {
            kind: GeoNodeKind::Polygon { vertices },
            hash: hasher.finalize(),
        }
    }

    pub fn extrude(height: f64, direction: DVec3, shape: Box<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x06]); // variant tag
        hasher.update(&height.to_le_bytes());
        hasher.update(&direction.x.to_le_bytes());
        hasher.update(&direction.y.to_le_bytes());
        hasher.update(&direction.z.to_le_bytes());
        hasher.update(shape.hash.as_bytes());
        
        Self {
            kind: GeoNodeKind::Extrude { height, direction, shape },
            hash: hasher.finalize(),
        }
    }

    pub fn transform(transform: Transform, shape: Box<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x07]); // variant tag
        // Hash transform components
        hasher.update(&transform.translation.x.to_le_bytes());
        hasher.update(&transform.translation.y.to_le_bytes());
        hasher.update(&transform.translation.z.to_le_bytes());
        hasher.update(&transform.rotation.x.to_le_bytes());
        hasher.update(&transform.rotation.y.to_le_bytes());
        hasher.update(&transform.rotation.z.to_le_bytes());
        hasher.update(&transform.rotation.w.to_le_bytes());
        hasher.update(shape.hash.as_bytes());
        
        Self {
            kind: GeoNodeKind::Transform { transform, shape },
            hash: hasher.finalize(),
        }
    }

    pub fn union_2d(shapes: Vec<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x08]); // variant tag
        hasher.update(&(shapes.len() as u32).to_le_bytes());
        for shape in &shapes {
            hasher.update(shape.hash.as_bytes());
        }
        
        Self {
            kind: GeoNodeKind::Union2D { shapes },
            hash: hasher.finalize(),
        }
    }

    pub fn union_3d(shapes: Vec<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x09]); // variant tag
        hasher.update(&(shapes.len() as u32).to_le_bytes());
        for shape in &shapes {
            hasher.update(shape.hash.as_bytes());
        }
        
        Self {
            kind: GeoNodeKind::Union3D { shapes },
            hash: hasher.finalize(),
        }
    }

    pub fn intersection_2d(shapes: Vec<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x0A]); // variant tag
        hasher.update(&(shapes.len() as u32).to_le_bytes());
        for shape in &shapes {
            hasher.update(shape.hash.as_bytes());
        }
        
        Self {
            kind: GeoNodeKind::Intersection2D { shapes },
            hash: hasher.finalize(),
        }
    }

    pub fn intersection_3d(shapes: Vec<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x0B]); // variant tag
        hasher.update(&(shapes.len() as u32).to_le_bytes());
        for shape in &shapes {
            hasher.update(shape.hash.as_bytes());
        }
        
        Self {
            kind: GeoNodeKind::Intersection3D { shapes },
            hash: hasher.finalize(),
        }
    }

    pub fn difference_2d(base: Box<GeoNode>, sub: Box<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x0C]); // variant tag
        hasher.update(base.hash.as_bytes());
        hasher.update(sub.hash.as_bytes());
        
        Self {
            kind: GeoNodeKind::Difference2D { base, sub },
            hash: hasher.finalize(),
        }
    }

    pub fn difference_3d(base: Box<GeoNode>, sub: Box<GeoNode>) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x0D]); // variant tag
        hasher.update(base.hash.as_bytes());
        hasher.update(sub.hash.as_bytes());
        
        Self {
            kind: GeoNodeKind::Difference3D { base, sub },
            hash: hasher.finalize(),
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

// Memory size estimation implementation

impl MemorySizeEstimator for GeoNode {
    fn estimate_memory_bytes(&self) -> usize {
        let base_size = std::mem::size_of::<GeoNode>();
        
        // Recursively estimate the size of the GeoNodeKind
        let kind_size = match &self.kind {
            // Leaf nodes - just their stack size
            GeoNodeKind::HalfSpace { .. } => std::mem::size_of::<DVec3>() * 2,
            GeoNodeKind::HalfPlane { .. } => std::mem::size_of::<DVec2>() * 2,
            GeoNodeKind::Circle { .. } => std::mem::size_of::<DVec2>() + std::mem::size_of::<f64>(),
            GeoNodeKind::Sphere { .. } => std::mem::size_of::<DVec3>() + std::mem::size_of::<f64>(),
            
            // Polygon - has a Vec of vertices
            GeoNodeKind::Polygon { vertices } => {
                std::mem::size_of::<Vec<DVec2>>() + vertices.capacity() * std::mem::size_of::<DVec2>()
            },
            
            // Single child nodes - recursive
            GeoNodeKind::Extrude { shape, .. } => {
                std::mem::size_of::<f64>() 
                    + std::mem::size_of::<DVec3>() 
                    + std::mem::size_of::<Box<GeoNode>>()
                    + shape.estimate_memory_bytes()
            },
            GeoNodeKind::Transform { shape, .. } => {
                std::mem::size_of::<Transform>() 
                    + std::mem::size_of::<Box<GeoNode>>()
                    + shape.estimate_memory_bytes()
            },
            
            // Multiple children nodes - recursive
            GeoNodeKind::Union2D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes.iter().map(|s| s.estimate_memory_bytes()).sum::<usize>()
            },
            GeoNodeKind::Union3D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes.iter().map(|s| s.estimate_memory_bytes()).sum::<usize>()
            },
            GeoNodeKind::Intersection2D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes.iter().map(|s| s.estimate_memory_bytes()).sum::<usize>()
            },
            GeoNodeKind::Intersection3D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes.iter().map(|s| s.estimate_memory_bytes()).sum::<usize>()
            },
            
            // Two children nodes - recursive
            GeoNodeKind::Difference2D { base, sub } => {
                std::mem::size_of::<Box<GeoNode>>() * 2
                    + base.estimate_memory_bytes()
                    + sub.estimate_memory_bytes()
            },
            GeoNodeKind::Difference3D { base, sub } => {
                std::mem::size_of::<Box<GeoNode>>() * 2
                    + base.estimate_memory_bytes()
                    + sub.estimate_memory_bytes()
            },
        };
        
        base_size + kind_size
    }
}

















