use crate::util::memory_size_estimator::MemorySizeEstimator;
use crate::util::transform::Transform;
use blake3;
use glam::f64::DMat2;
use glam::f64::DMat3;
use glam::f64::DVec2;
use glam::f64::DVec3;
use std::fmt;

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
    Ellipse {
        center: DVec2, // real-space center = L₂·c₀
        basis: DMat2,  // columns = r·a₂, r·b₂ (maps the unit disk → the ellipse)
        // Derived, precomputed by the constructor; EXCLUDED from hashing.
        inv_basis: DMat2,     // basis.inverse() (unused when degenerate)
        lipschitz_scale: f64, // σ_min(basis); 0.0 marks a degenerate (empty) ellipse
    },
    Sphere {
        center: DVec3,
        radius: f64,
    },
    Ellipsoid {
        center: DVec3, // real-space center = L·c₀
        basis: DMat3,  // columns = r·a, r·b, r·c (maps the unit ball → the ellipsoid)
        // Derived, precomputed by the constructor; EXCLUDED from hashing.
        inv_basis: DMat3,     // basis.inverse() (unused when degenerate)
        lipschitz_scale: f64, // σ_min(basis); 0.0 marks a degenerate (empty) ellipsoid
    },
    Polygon {
        vertices: Vec<DVec2>,
    },
    Extrude {
        height: f64,
        direction: DVec3,
        shape: Box<GeoNode>,
        plane_to_world_transform: Transform,
        infinite: bool,
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
        sub: Box<GeoNode>,
    },
    Difference3D {
        base: Box<GeoNode>,
        sub: Box<GeoNode>,
    },
}

pub mod batched_implicit_evaluator;
pub mod csg_cache;
mod csg_conversion;
pub mod csg_types;
pub mod csg_utils;
mod implicit_eval;
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
                format!(
                    "{}HalfSpace(normal: {}, center: {})",
                    prefix,
                    format_vec3(normal),
                    format_vec3(center)
                )
            }
            GeoNodeKind::HalfPlane { point1, point2 } => {
                format!(
                    "{}HalfPlane(p1: {}, p2: {})",
                    prefix,
                    format_vec2(point1),
                    format_vec2(point2)
                )
            }
            GeoNodeKind::Circle { center, radius } => {
                format!(
                    "{}Circle(center: {}, radius: {})",
                    prefix,
                    format_vec2(center),
                    format_f64(radius)
                )
            }
            GeoNodeKind::Ellipse { center, basis, .. } => {
                format!(
                    "{}Ellipse(center: {}, basis: [{}, {}])",
                    prefix,
                    format_vec2(center),
                    format_vec2(&basis.x_axis),
                    format_vec2(&basis.y_axis)
                )
            }
            GeoNodeKind::Sphere { center, radius } => {
                format!(
                    "{}Sphere(center: {}, radius: {})",
                    prefix,
                    format_vec3(center),
                    format_f64(radius)
                )
            }
            GeoNodeKind::Ellipsoid { center, basis, .. } => {
                format!(
                    "{}Ellipsoid(center: {}, basis: [{}, {}, {}])",
                    prefix,
                    format_vec3(center),
                    format_vec3(&basis.x_axis),
                    format_vec3(&basis.y_axis),
                    format_vec3(&basis.z_axis)
                )
            }
            GeoNodeKind::Polygon { vertices } => {
                let mut result = format!("{}Polygon({} vertices)", prefix, vertices.len());
                for (i, vertex) in vertices.iter().enumerate() {
                    result.push_str(&format!("\n{}  [{}]: {}", prefix, i, format_vec2(vertex)));
                }
                result
            }
            GeoNodeKind::Extrude {
                height,
                direction,
                shape,
                plane_to_world_transform,
                infinite,
            } => {
                format!(
                    "{}Extrude(height: {}, direction: {}, transform: {}, infinite: {})\n{}",
                    prefix,
                    format_f64(height),
                    format_vec3(direction),
                    format_transform(plane_to_world_transform),
                    infinite,
                    shape.display_with_indent(indent + 1)
                )
            }
            GeoNodeKind::Transform { transform, shape } => {
                format!(
                    "{}Transform({})\n{}",
                    prefix,
                    format_transform(transform),
                    shape.display_with_indent(indent + 1)
                )
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
                format!(
                    "{}Difference2D\n{}base:\n{}\n{}sub:\n{}",
                    prefix,
                    child_prefix,
                    base.display_with_indent(indent + 2),
                    child_prefix,
                    sub.display_with_indent(indent + 2)
                )
            }
            GeoNodeKind::Difference3D { base, sub } => {
                format!(
                    "{}Difference3D\n{}base:\n{}\n{}sub:\n{}",
                    prefix,
                    child_prefix,
                    base.display_with_indent(indent + 2),
                    child_prefix,
                    sub.display_with_indent(indent + 2)
                )
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

    /// Construct the lattice image of a circle: a disk in fractional (lattice)
    /// coordinates mapped to real space through `basis` (columns = r·a₂, r·b₂),
    /// centered at `center` (= L₂·c₀). This is an ellipse in general — the 2D
    /// analog of [`GeoNode::ellipsoid`], with the same two normalizations:
    /// 1. **Circular-basis fast path** — if the basis columns are orthogonal with
    ///    equal lengths, the shape is a true Euclidean circle and a plain
    ///    `GeoNodeKind::Circle` is returned (square effective cells become
    ///    byte-identical to a directly constructed circle; also catches rotated
    ///    orthonormal bases).
    /// 2. **Degenerate basis** — if `|det(basis)|` is ~0 the ellipse is empty;
    ///    a degenerate marker (`lipschitz_scale = 0.0`) is stored and eval returns
    ///    `f64::MAX` everywhere (never panics).
    pub fn ellipse(center: DVec2, basis: DMat2) -> Self {
        // 1. Circular-basis fast path (the square-cell-regression guard).
        if let Some(radius) = circular_radius_2d(&basis) {
            return Self::circle(center, radius);
        }

        // 2. Degenerate basis → empty shape.
        if basis.determinant().abs() < 1e-12 {
            return Self::ellipse_from_parts(center, basis, DMat2::ZERO, 0.0);
        }

        // 3. General ellipse.
        let inv_basis = basis.inverse();
        // σ_min(basis) = √λ_min(basisᵀ·basis); the tightest single-scalar rescaling
        // that makes the SDF exactly 1-Lipschitz (see implicit_eval.rs).
        let gram = basis.transpose() * basis;
        let lipschitz_scale = min_eigenvalue_symmetric_2x2(&gram).max(0.0).sqrt();
        Self::ellipse_from_parts(center, basis, inv_basis, lipschitz_scale)
    }

    /// Assembles an `Ellipse` node, hashing **only** `center` + `basis`
    /// (tag `0x0F`); the derived `inv_basis` / `lipschitz_scale` must not affect
    /// identity.
    fn ellipse_from_parts(
        center: DVec2,
        basis: DMat2,
        inv_basis: DMat2,
        lipschitz_scale: f64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x0F]); // variant tag
        hasher.update(&center.x.to_le_bytes());
        hasher.update(&center.y.to_le_bytes());
        for col in [basis.x_axis, basis.y_axis] {
            hasher.update(&col.x.to_le_bytes());
            hasher.update(&col.y.to_le_bytes());
        }

        Self {
            kind: GeoNodeKind::Ellipse {
                center,
                basis,
                inv_basis,
                lipschitz_scale,
            },
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

    /// Construct the lattice image of a sphere: a ball in fractional (lattice)
    /// coordinates mapped to real space through `basis` (columns = r·a, r·b, r·c),
    /// centered at `center` (= L·c₀). This is an ellipsoid in general.
    ///
    /// Two normalizations run, in order:
    /// 1. **Spherical-basis fast path** — if the basis columns are pairwise
    ///    orthogonal with equal lengths, the shape is a true Euclidean sphere and
    ///    a plain `GeoNodeKind::Sphere` is returned instead. This makes
    ///    (approximately) cubic cells byte-identical to a directly constructed
    ///    sphere (same SDF/CSG arms, same hash), and also catches
    ///    rotated-orthonormal bases.
    /// 2. **Degenerate basis** — if `|det(basis)|` is ~0 the ellipsoid is empty;
    ///    a degenerate marker (`lipschitz_scale = 0.0`) is stored and eval returns
    ///    `f64::MAX` everywhere (never panics).
    pub fn ellipsoid(center: DVec3, basis: DMat3) -> Self {
        // 1. Spherical-basis fast path (the cubic-regression guard).
        if let Some(radius) = spherical_radius_3d(&basis) {
            return Self::sphere(center, radius);
        }

        // 2. Degenerate basis → empty shape.
        if basis.determinant().abs() < 1e-12 {
            return Self::ellipsoid_from_parts(center, basis, DMat3::ZERO, 0.0);
        }

        // 3. General ellipsoid.
        let inv_basis = basis.inverse();
        // σ_min(basis) = √λ_min(basisᵀ·basis); this is the tightest single-scalar
        // rescaling that makes the SDF exactly 1-Lipschitz (see implicit_eval.rs).
        let gram = basis.transpose() * basis;
        let lipschitz_scale = min_eigenvalue_symmetric_3x3(&gram).max(0.0).sqrt();
        Self::ellipsoid_from_parts(center, basis, inv_basis, lipschitz_scale)
    }

    /// Assembles an `Ellipsoid` node, hashing **only** `center` + `basis`
    /// (tag `0x0E`); the derived `inv_basis` / `lipschitz_scale` must not affect
    /// identity.
    fn ellipsoid_from_parts(
        center: DVec3,
        basis: DMat3,
        inv_basis: DMat3,
        lipschitz_scale: f64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x0E]); // variant tag
        hasher.update(&center.x.to_le_bytes());
        hasher.update(&center.y.to_le_bytes());
        hasher.update(&center.z.to_le_bytes());
        for col in [basis.x_axis, basis.y_axis, basis.z_axis] {
            hasher.update(&col.x.to_le_bytes());
            hasher.update(&col.y.to_le_bytes());
            hasher.update(&col.z.to_le_bytes());
        }

        Self {
            kind: GeoNodeKind::Ellipsoid {
                center,
                basis,
                inv_basis,
                lipschitz_scale,
            },
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

    pub fn extrude(
        height: f64,
        direction: DVec3,
        shape: Box<GeoNode>,
        plane_to_world_transform: Transform,
        infinite: bool,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[0x06]); // variant tag
        hasher.update(&height.to_le_bytes());
        hasher.update(&[infinite as u8]);
        hasher.update(&direction.x.to_le_bytes());
        hasher.update(&direction.y.to_le_bytes());
        hasher.update(&direction.z.to_le_bytes());
        hasher.update(shape.hash.as_bytes());
        // Hash plane_to_world_transform
        hasher.update(&plane_to_world_transform.translation.x.to_le_bytes());
        hasher.update(&plane_to_world_transform.translation.y.to_le_bytes());
        hasher.update(&plane_to_world_transform.translation.z.to_le_bytes());
        hasher.update(&plane_to_world_transform.rotation.x.to_le_bytes());
        hasher.update(&plane_to_world_transform.rotation.y.to_le_bytes());
        hasher.update(&plane_to_world_transform.rotation.z.to_le_bytes());
        hasher.update(&plane_to_world_transform.rotation.w.to_le_bytes());

        Self {
            kind: GeoNodeKind::Extrude {
                height,
                direction,
                shape,
                plane_to_world_transform,
                infinite,
            },
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

/// If the basis columns are pairwise orthogonal with (approximately) equal
/// lengths, the mapped ball is a true Euclidean sphere; returns its radius (the
/// common column length). Otherwise `None`. Tolerance `1e-9` (relative); a zero
/// or near-zero column length disqualifies the basis (it is not spherical — it
/// is degenerate, handled separately).
fn spherical_radius_3d(basis: &DMat3) -> Option<f64> {
    const TOL: f64 = 1e-9;
    let c0 = basis.x_axis;
    let c1 = basis.y_axis;
    let c2 = basis.z_axis;
    let l0 = c0.length();
    let l1 = c1.length();
    let l2 = c2.length();
    if l0 <= 0.0 || l1 <= 0.0 || l2 <= 0.0 {
        return None;
    }
    // Pairwise orthogonality: |cᵢ·cⱼ| ≤ tol·|cᵢ|·|cⱼ|.
    if c0.dot(c1).abs() > TOL * l0 * l1
        || c0.dot(c2).abs() > TOL * l0 * l2
        || c1.dot(c2).abs() > TOL * l1 * l2
    {
        return None;
    }
    // Equal lengths: ||cᵢ| − |cⱼ|| ≤ tol·max_k|cₖ|.
    let max_l = l0.max(l1).max(l2);
    if (l0 - l1).abs() > TOL * max_l
        || (l0 - l2).abs() > TOL * max_l
        || (l1 - l2).abs() > TOL * max_l
    {
        return None;
    }
    Some(l0)
}

/// 2D analog of [`spherical_radius_3d`]: if the two basis columns are orthogonal
/// with (approximately) equal lengths, the mapped disk is a true Euclidean circle;
/// returns its radius (the common column length). Otherwise `None`. A zero or
/// near-zero column length disqualifies the basis (degenerate, handled separately).
fn circular_radius_2d(basis: &DMat2) -> Option<f64> {
    const TOL: f64 = 1e-9;
    let c0 = basis.x_axis;
    let c1 = basis.y_axis;
    let l0 = c0.length();
    let l1 = c1.length();
    if l0 <= 0.0 || l1 <= 0.0 {
        return None;
    }
    // Orthogonality: |c₀·c₁| ≤ tol·|c₀|·|c₁|.
    if c0.dot(c1).abs() > TOL * l0 * l1 {
        return None;
    }
    // Equal lengths: ||c₀| − |c₁|| ≤ tol·max(|c₀|, |c₁|).
    if (l0 - l1).abs() > TOL * l0.max(l1) {
        return None;
    }
    Some(l0)
}

/// Smallest eigenvalue of a symmetric 2×2 matrix `[[a, b], [b, c]]` via the
/// quadratic formula: `λ = (a+c)/2 ± √(((a−c)/2)² + b²)`. Used to obtain `σ_min`
/// of a 2D basis as `√λ_min(basisᵀ·basis)`. `m` is assumed symmetric.
fn min_eigenvalue_symmetric_2x2(m: &DMat2) -> f64 {
    // Column-major glam storage: x_axis is column 0, y_axis is column 1.
    let a = m.x_axis.x;
    let c = m.y_axis.y;
    let b = m.y_axis.x; // (row 0, col 1)
    let mean = (a + c) / 2.0;
    let half_diff = (a - c) / 2.0;
    let disc = (half_diff * half_diff + b * b).max(0.0).sqrt();
    mean - disc
}

/// Smallest eigenvalue of a symmetric 3×3 matrix via the standard closed-form
/// trigonometric solution (Smith 1961). Used to obtain `σ_min` of a basis as
/// `√λ_min(basisᵀ·basis)` without an SVD library. `m` is assumed symmetric.
fn min_eigenvalue_symmetric_3x3(m: &DMat3) -> f64 {
    // Column-major glam storage: x_axis is column 0, etc.
    let a11 = m.x_axis.x;
    let a22 = m.y_axis.y;
    let a33 = m.z_axis.z;
    let a12 = m.y_axis.x; // (row 0, col 1)
    let a13 = m.z_axis.x; // (row 0, col 2)
    let a23 = m.z_axis.y; // (row 1, col 2)

    let p1 = a12 * a12 + a13 * a13 + a23 * a23;
    if p1 == 0.0 {
        // Already diagonal.
        return a11.min(a22).min(a33);
    }

    let q = (a11 + a22 + a33) / 3.0;
    let p2 = (a11 - q) * (a11 - q) + (a22 - q) * (a22 - q) + (a33 - q) * (a33 - q) + 2.0 * p1;
    let p = (p2 / 6.0).sqrt();

    // B = (1/p)·(A − q·I); r = det(B)/2 ∈ [−1, 1].
    let b11 = (a11 - q) / p;
    let b22 = (a22 - q) / p;
    let b33 = (a33 - q) / p;
    let b12 = a12 / p;
    let b13 = a13 / p;
    let b23 = a23 / p;
    let det_b = b11 * (b22 * b33 - b23 * b23) - b12 * (b12 * b33 - b23 * b13)
        + b13 * (b12 * b23 - b22 * b13);
    let r = (det_b / 2.0).clamp(-1.0, 1.0);

    let phi = r.acos() / 3.0;
    // Smallest of the three eigenvalues q + 2p·cos(φ + 2πk/3).
    q + 2.0 * p * (phi + 2.0 * std::f64::consts::PI / 3.0).cos()
}

// Helper functions for formatting
fn format_vec2(v: &DVec2) -> String {
    format!("({}, {})", format_f64(&v.x), format_f64(&v.y))
}

fn format_vec3(v: &DVec3) -> String {
    format!(
        "({}, {}, {})",
        format_f64(&v.x),
        format_f64(&v.y),
        format_f64(&v.z)
    )
}

fn format_f64(f: &f64) -> String {
    if f.fract() == 0.0 {
        format!("{}", *f as i64)
    } else {
        format!("{:.2}", f)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
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
            GeoNodeKind::Ellipse { .. } => {
                std::mem::size_of::<DVec2>()
                    + 2 * std::mem::size_of::<DMat2>()
                    + std::mem::size_of::<f64>()
            }
            GeoNodeKind::Sphere { .. } => std::mem::size_of::<DVec3>() + std::mem::size_of::<f64>(),
            GeoNodeKind::Ellipsoid { .. } => {
                std::mem::size_of::<DVec3>()
                    + 2 * std::mem::size_of::<DMat3>()
                    + std::mem::size_of::<f64>()
            }

            // Polygon - has a Vec of vertices
            GeoNodeKind::Polygon { vertices } => {
                std::mem::size_of::<Vec<DVec2>>()
                    + vertices.capacity() * std::mem::size_of::<DVec2>()
            }

            // Single child nodes - recursive
            GeoNodeKind::Extrude { shape, .. } => {
                std::mem::size_of::<f64>()
                    + std::mem::size_of::<DVec3>()
                    + std::mem::size_of::<Box<GeoNode>>()
                    + shape.estimate_memory_bytes()
            }
            GeoNodeKind::Transform { shape, .. } => {
                std::mem::size_of::<Transform>()
                    + std::mem::size_of::<Box<GeoNode>>()
                    + shape.estimate_memory_bytes()
            }

            // Multiple children nodes - recursive
            GeoNodeKind::Union2D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes
                        .iter()
                        .map(|s| s.estimate_memory_bytes())
                        .sum::<usize>()
            }
            GeoNodeKind::Union3D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes
                        .iter()
                        .map(|s| s.estimate_memory_bytes())
                        .sum::<usize>()
            }
            GeoNodeKind::Intersection2D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes
                        .iter()
                        .map(|s| s.estimate_memory_bytes())
                        .sum::<usize>()
            }
            GeoNodeKind::Intersection3D { shapes } => {
                std::mem::size_of::<Vec<GeoNode>>()
                    + shapes
                        .iter()
                        .map(|s| s.estimate_memory_bytes())
                        .sum::<usize>()
            }

            // Two children nodes - recursive
            GeoNodeKind::Difference2D { base, sub } => {
                std::mem::size_of::<Box<GeoNode>>() * 2
                    + base.estimate_memory_bytes()
                    + sub.estimate_memory_bytes()
            }
            GeoNodeKind::Difference3D { base, sub } => {
                std::mem::size_of::<Box<GeoNode>>() * 2
                    + base.estimate_memory_bytes()
                    + sub.estimate_memory_bytes()
            }
        };

        base_size + kind_size
    }
}
