//! Tests for `GeoNodeKind::Ellipsoid` (Phase 1) and `GeoNodeKind::Ellipse`
//! (Phase 2) of the lattice-covariant sphere/circle design,
//! `doc/design_lattice_covariant_sphere_circle.md`.
//!
//! `GeoNodeKind` is private, so the variant is introspected through the public
//! `Display` impl (each arm's string starts with the variant name) and through
//! `hash()` (the spherical/circular-basis fast path snaps to `Sphere`/`Circle`,
//! giving hash equality with a directly constructed sphere/circle).

use glam::f64::{DMat2, DMat3, DVec2, DVec3};
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::geo_tree::implicit_geometry::{
    BATCH_SIZE, ImplicitGeometry2D, ImplicitGeometry3D,
};

/// A generic skewed (non-orthogonal, unequal-length) basis used across tests.
fn skewed_basis() -> DMat3 {
    DMat3::from_cols(
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(1.0, 3.0, 0.0),
        DVec3::new(0.5, 0.5, 5.0),
    )
}

/// Exact membership test: `x ∈ E ⇔ |inv_basis·(x − center)| ≤ 1`. The test
/// recomputes `inv_basis` itself (the node's copy is private).
fn membership(center: DVec3, basis: DMat3, x: DVec3) -> f64 {
    let inv = basis.inverse();
    (inv * (x - center)).length() - 1.0
}

fn variant_name(node: &GeoNode) -> String {
    format!("{}", node)
}

// =============================================================================
// Sign & zero set
// =============================================================================

#[test]
fn test_ellipsoid_sign_and_zero_set() {
    let center = DVec3::new(2.0, -1.0, 3.0);
    let basis = skewed_basis();
    let node = GeoNode::ellipsoid(center, basis);
    assert!(
        variant_name(&node).starts_with("Ellipsoid"),
        "skewed basis must stay an Ellipsoid, got: {}",
        variant_name(&node)
    );

    // Center is strictly inside.
    assert!(
        node.implicit_eval_3d(&center) < 0.0,
        "center should be inside (negative SDF)"
    );

    let unit_dirs = [
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(1.0, 1.0, 1.0).normalize(),
        DVec3::new(-1.0, 2.0, -0.5).normalize(),
    ];

    for u in unit_dirs {
        // center + basis·u lies exactly on the surface (|q| = 1).
        let on_surface = center + basis * u;
        assert!(
            node.implicit_eval_3d(&on_surface).abs() < 1e-9,
            "center + basis·u should be on the surface (SDF ≈ 0)"
        );

        // center + basis·(2u) lies outside (|q| = 2).
        let outside = center + basis * (2.0 * u);
        assert!(
            node.implicit_eval_3d(&outside) > 0.0,
            "center + basis·(2u) should be outside (positive SDF)"
        );
    }
}

// =============================================================================
// Conservativeness (exact sign, underestimated magnitude)
// =============================================================================

#[test]
fn test_ellipsoid_conservative_and_correct_sign() {
    let center = DVec3::new(1.0, 2.0, -1.0);
    let basis = skewed_basis();
    let node = GeoNode::ellipsoid(center, basis);

    // Dense sampling of the surface: y_j = center + basis·u_j over a grid of
    // unit directions u_j.
    let mut surface = Vec::new();
    let n_theta = 40;
    let n_phi = 80;
    for i in 0..n_theta {
        let theta = std::f64::consts::PI * (i as f64 + 0.5) / n_theta as f64;
        for j in 0..n_phi {
            let phi = 2.0 * std::f64::consts::PI * (j as f64) / n_phi as f64;
            let u = DVec3::new(
                theta.sin() * phi.cos(),
                theta.sin() * phi.sin(),
                theta.cos(),
            );
            surface.push(center + basis * u);
        }
    }

    // Deterministic pseudo-random sample points via a simple LCG.
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // map to [-8, 8]
        ((state >> 11) as f64 / (1u64 << 53) as f64) * 16.0 - 8.0
    };

    for _ in 0..200 {
        let x = center + DVec3::new(next(), next(), next());
        let f = node.implicit_eval_3d(&x);

        // Sign must match the exact membership test.
        let m = membership(center, basis, x);
        assert_eq!(
            f < 0.0,
            m < 0.0,
            "SDF sign must match exact membership at {:?}",
            x
        );

        // Magnitude must not exceed the true distance. The sampled minimum
        // distance to the surface overestimates the true distance, so
        // |f(x)| ≤ min_j |x − y_j| + ε is a valid one-sided check.
        let min_dist = surface
            .iter()
            .map(|y| (x - *y).length())
            .fold(f64::MAX, f64::min);
        assert!(
            f.abs() <= min_dist + 1e-6,
            "|SDF| = {} must underestimate distance-to-surface {} at {:?}",
            f.abs(),
            min_dist,
            x
        );
    }
}

// =============================================================================
// Spherical-basis snap (the cubic-regression guard)
// =============================================================================

#[test]
fn test_ellipsoid_snaps_axis_aligned_scaled_identity_to_sphere() {
    let center = DVec3::new(1.0, -2.0, 0.5);
    let s = 3.0;
    let basis = DMat3::from_cols(
        DVec3::new(s, 0.0, 0.0),
        DVec3::new(0.0, s, 0.0),
        DVec3::new(0.0, 0.0, s),
    );
    let node = GeoNode::ellipsoid(center, basis);

    assert!(
        variant_name(&node).starts_with("Sphere"),
        "s·I must snap to a Sphere, got: {}",
        variant_name(&node)
    );
    // Hash equality pins the SDF arm, the CSG arm, and cache identity in a
    // single assertion.
    assert_eq!(
        node.hash(),
        GeoNode::sphere(center, s).hash(),
        "snapped node must be hash-equal to GeoNode::sphere(center, s)"
    );
}

#[test]
fn test_ellipsoid_snaps_rotated_orthonormal_to_sphere() {
    let center = DVec3::new(0.0, 0.0, 0.0);
    let s = 2.0;
    // A rotation (orthonormal columns) scaled by s — still a Euclidean sphere.
    let angle = 0.7_f64;
    let (c, sn) = (angle.cos(), angle.sin());
    let r = DMat3::from_cols(
        DVec3::new(c, sn, 0.0),
        DVec3::new(-sn, c, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
    );
    let basis = DMat3::from_cols(r.x_axis * s, r.y_axis * s, r.z_axis * s);
    let node = GeoNode::ellipsoid(center, basis);

    assert!(
        variant_name(&node).starts_with("Sphere"),
        "rotated-orthonormal basis must snap to a Sphere, got: {}",
        variant_name(&node)
    );
    // Behaves as a Euclidean sphere of radius ≈ s.
    let on_surface = node.implicit_eval_3d(&DVec3::new(s, 0.0, 0.0));
    assert!(on_surface.abs() < 1e-9, "radius should be ≈ s");
}

// =============================================================================
// Near-threshold continuity (no magnitude jump across the snap)
// =============================================================================

#[test]
fn test_ellipsoid_near_threshold_stays_ellipsoid_and_agrees() {
    let center = DVec3::ZERO;
    let s = 4.0;
    // Perturb one column length by ~1e-6 (well outside the 1e-9 snap tolerance),
    // keeping the columns orthogonal.
    let eps = 1e-6;
    let basis = DMat3::from_cols(
        DVec3::new(s, 0.0, 0.0),
        DVec3::new(0.0, s, 0.0),
        DVec3::new(0.0, 0.0, s * (1.0 + eps)),
    );
    let node = GeoNode::ellipsoid(center, basis);
    assert!(
        variant_name(&node).starts_with("Ellipsoid"),
        "a basis just outside the snap tolerance must stay an Ellipsoid, got: {}",
        variant_name(&node)
    );

    // Its eval agrees with the snapped sphere's to ~the perturbation size.
    let sphere = GeoNode::sphere(center, s);
    let probes = [
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(0.0, -3.0, 1.0),
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(0.0, 0.0, 6.0),
    ];
    for p in probes {
        let diff = (node.implicit_eval_3d(&p) - sphere.implicit_eval_3d(&p)).abs();
        assert!(
            diff < 1e-3,
            "near-cubic ellipsoid SDF should stay within ~perturbation of the sphere, diff = {}",
            diff
        );
    }
}

// =============================================================================
// Hashing
// =============================================================================

#[test]
fn test_ellipsoid_hash_depends_only_on_center_and_basis() {
    let center = DVec3::new(1.0, 2.0, 3.0);
    let basis = skewed_basis();

    // Equal center/basis → equal hash (derived fields are recomputed identically
    // and excluded from the hash; constructed only via the public constructor).
    let a = GeoNode::ellipsoid(center, basis);
    let b = GeoNode::ellipsoid(center, basis);
    assert_eq!(a.hash(), b.hash(), "equal center/basis must hash equal");

    // Differing basis → differing hash.
    let basis2 = DMat3::from_cols(
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(1.0, 3.0, 0.0),
        DVec3::new(0.5, 0.5, 6.0), // changed
    );
    let c = GeoNode::ellipsoid(center, basis2);
    assert_ne!(
        a.hash(),
        c.hash(),
        "different basis must produce a different hash"
    );

    // Differing center → differing hash.
    let d = GeoNode::ellipsoid(DVec3::new(1.0, 2.0, 4.0), basis);
    assert_ne!(a.hash(), d.hash(), "different center must hash differently");
}

// =============================================================================
// Degenerate basis
// =============================================================================

#[test]
fn test_ellipsoid_degenerate_basis_is_empty() {
    let center = DVec3::new(1.0, 1.0, 1.0);
    // Third column is a linear combination of the first two → zero determinant.
    let basis = DMat3::from_cols(
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(1.0, 1.0, 0.0),
    );
    let node = GeoNode::ellipsoid(center, basis);
    assert!(
        variant_name(&node).starts_with("Ellipsoid"),
        "degenerate basis stays an Ellipsoid variant (marked empty)"
    );

    // Empty shape: f64::MAX everywhere, no panic.
    for p in [center, DVec3::ZERO, DVec3::new(100.0, -50.0, 7.0)] {
        assert_eq!(
            node.implicit_eval_3d(&p),
            f64::MAX,
            "degenerate ellipsoid must be empty (f64::MAX) everywhere"
        );
    }

    // Empty CSG mesh, no panic.
    let mesh = node
        .to_csg_mesh()
        .expect("degenerate ellipsoid should convert");
    assert!(
        mesh.polygons.is_empty(),
        "degenerate ellipsoid must yield an empty mesh"
    );
}

// =============================================================================
// CSG mesh
// =============================================================================

#[test]
fn test_ellipsoid_csg_vertices_lie_on_surface() {
    let center = DVec3::new(1.0, 0.0, -2.0);
    let basis = skewed_basis();
    let node = GeoNode::ellipsoid(center, basis);
    let inv = basis.inverse();

    let mesh = node
        .to_csg_mesh()
        .expect("ellipsoid should convert to a mesh");
    assert!(!mesh.polygons.is_empty(), "mesh should be non-empty");

    for poly in &mesh.polygons {
        for v in &poly.vertices {
            let p = DVec3::new(v.pos.x, v.pos.y, v.pos.z);
            let radial = (inv * (p - center)).length();
            assert!(
                (radial - 1.0).abs() < 1e-6,
                "every mesh vertex must lie on the ellipsoid (|inv_basis·(v−c)| ≈ 1), got {}",
                radial
            );
        }
    }

    // Vertex/polygon count matches the 24×12 sphere tessellation (the transform
    // preserves topology).
    let unit_sphere = GeoNode::sphere(center, 1.0)
        .to_csg_mesh()
        .expect("unit sphere should convert");
    assert_eq!(
        mesh.polygons.len(),
        unit_sphere.polygons.len(),
        "ellipsoid mesh should have the same polygon count as the sphere tessellation"
    );
}

// =============================================================================
// Batch == scalar
// =============================================================================

#[test]
fn test_ellipsoid_batch_matches_scalar() {
    let center = DVec3::new(0.5, -0.5, 1.0);
    let basis = skewed_basis();
    let node = GeoNode::ellipsoid(center, basis);

    let mut points = [DVec3::ZERO; BATCH_SIZE];
    for (i, p) in points.iter_mut().enumerate() {
        let t = i as f64 * 0.013;
        *p = center + DVec3::new(t.sin() * 6.0, (t * 1.3).cos() * 6.0, (t * 0.7).sin() * 6.0);
    }

    let mut batch_results = [0.0; BATCH_SIZE];
    node.implicit_eval_3d_batch(&points, &mut batch_results);

    for i in 0..BATCH_SIZE {
        let scalar = node.implicit_eval_3d(&points[i]);
        assert!(
            (batch_results[i] - scalar).abs() < 1e-12,
            "batch and scalar SDF must agree at index {}",
            i
        );
    }
}

#[test]
fn test_ellipsoid_degenerate_batch_is_empty() {
    let basis = DMat3::from_cols(
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(2.0, 0.0, 0.0), // parallel → degenerate
        DVec3::new(0.0, 0.0, 1.0),
    );
    let node = GeoNode::ellipsoid(DVec3::ZERO, basis);
    let points = [DVec3::new(1.0, 2.0, 3.0); BATCH_SIZE];
    let mut results = [0.0; BATCH_SIZE];
    node.implicit_eval_3d_batch(&points, &mut results);
    assert!(
        results.iter().all(|&r| r == f64::MAX),
        "degenerate ellipsoid batch must be f64::MAX everywhere"
    );
}

// =============================================================================
// Composition smoke
// =============================================================================

#[test]
fn test_ellipsoid_composition_signs() {
    let center = DVec3::ZERO;
    let basis = skewed_basis();
    let ellipsoid = GeoNode::ellipsoid(center, basis);

    // Cut the ellipsoid with the half-space whose interior is z < 0 (normal +Z,
    // through origin): Difference3D(ellipsoid, half_space) removes that interior,
    // keeping the z > 0 half of the ellipsoid.
    let half_space = GeoNode::half_space(DVec3::new(0.0, 0.0, 1.0), DVec3::ZERO);
    let diff = GeoNode::difference_3d(Box::new(ellipsoid), Box::new(half_space));

    // A point well inside the ellipsoid and clearly on the +Z (kept) side.
    let inside = DVec3::new(0.0, 0.0, 2.0);
    assert!(
        diff.implicit_eval_3d(&inside) < 0.0,
        "point inside ellipsoid and above the cut should be inside"
    );

    // The −Z half (the half-space interior) was carved away.
    let carved = DVec3::new(0.0, 0.0, -2.0);
    assert!(
        diff.implicit_eval_3d(&carved) > 0.0,
        "point in the carved (−Z) half should be outside"
    );

    // A point far outside the ellipsoid is outside.
    let far = DVec3::new(50.0, 0.0, 1.0);
    assert!(
        diff.implicit_eval_3d(&far) > 0.0,
        "far point should be outside"
    );
}

// =============================================================================
// Phase 2 — `GeoNodeKind::Ellipse` (2D)
//
// 2D mirrors of the Phase 1 checks, plus an extrude-integration test.
// =============================================================================

/// A generic skewed (non-orthogonal, unequal-length) 2D basis.
fn skewed_basis_2d() -> DMat2 {
    DMat2::from_cols(DVec2::new(4.0, 0.0), DVec2::new(1.0, 3.0))
}

/// Exact membership test: `y ∈ E ⇔ |inv_basis·(y − center)| ≤ 1`.
fn membership_2d(center: DVec2, basis: DMat2, y: DVec2) -> f64 {
    let inv = basis.inverse();
    (inv * (y - center)).length() - 1.0
}

#[test]
fn test_ellipse_sign_and_zero_set() {
    let center = DVec2::new(2.0, -1.0);
    let basis = skewed_basis_2d();
    let node = GeoNode::ellipse(center, basis);
    assert!(
        variant_name(&node).starts_with("Ellipse"),
        "skewed basis must stay an Ellipse, got: {}",
        variant_name(&node)
    );

    // Center is strictly inside.
    assert!(
        node.implicit_eval_2d(&center) < 0.0,
        "center should be inside (negative SDF)"
    );

    let unit_dirs = [
        DVec2::new(1.0, 0.0),
        DVec2::new(0.0, 1.0),
        DVec2::new(1.0, 1.0).normalize(),
        DVec2::new(-1.0, 2.0).normalize(),
    ];

    for u in unit_dirs {
        // center + basis·u lies exactly on the boundary (|q| = 1).
        let on_boundary = center + basis * u;
        assert!(
            node.implicit_eval_2d(&on_boundary).abs() < 1e-9,
            "center + basis·u should be on the boundary (SDF ≈ 0)"
        );

        // center + basis·(2u) lies outside (|q| = 2).
        let outside = center + basis * (2.0 * u);
        assert!(
            node.implicit_eval_2d(&outside) > 0.0,
            "center + basis·(2u) should be outside (positive SDF)"
        );
    }
}

#[test]
fn test_ellipse_conservative_and_correct_sign() {
    let center = DVec2::new(1.0, 2.0);
    let basis = skewed_basis_2d();
    let node = GeoNode::ellipse(center, basis);

    // Dense sampling of the boundary: y_j = center + basis·u_j over a ring of
    // unit directions u_j.
    let mut boundary = Vec::new();
    let n = 720;
    for i in 0..n {
        let phi = 2.0 * std::f64::consts::PI * (i as f64) / n as f64;
        let u = DVec2::new(phi.cos(), phi.sin());
        boundary.push(center + basis * u);
    }

    // Deterministic pseudo-random sample points via a simple LCG.
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64 / (1u64 << 53) as f64) * 16.0 - 8.0
    };

    for _ in 0..200 {
        let y = center + DVec2::new(next(), next());
        let f = node.implicit_eval_2d(&y);

        // Sign must match the exact membership test.
        let m = membership_2d(center, basis, y);
        assert_eq!(
            f < 0.0,
            m < 0.0,
            "SDF sign must match membership at {:?}",
            y
        );

        // Magnitude must not exceed true distance (sampled min overestimates it).
        let min_dist = boundary
            .iter()
            .map(|b| (y - *b).length())
            .fold(f64::MAX, f64::min);
        assert!(
            f.abs() <= min_dist + 1e-6,
            "|SDF| = {} must underestimate distance-to-boundary {} at {:?}",
            f.abs(),
            min_dist,
            y
        );
    }
}

#[test]
fn test_ellipse_snaps_axis_aligned_scaled_identity_to_circle() {
    let center = DVec2::new(1.0, -2.0);
    let s = 3.0;
    let basis = DMat2::from_cols(DVec2::new(s, 0.0), DVec2::new(0.0, s));
    let node = GeoNode::ellipse(center, basis);

    assert!(
        variant_name(&node).starts_with("Circle"),
        "s·I must snap to a Circle, got: {}",
        variant_name(&node)
    );
    // Hash equality pins the SDF arm, the CSG arm, and cache identity at once.
    assert_eq!(
        node.hash(),
        GeoNode::circle(center, s).hash(),
        "snapped node must be hash-equal to GeoNode::circle(center, s)"
    );
}

#[test]
fn test_ellipse_snaps_rotated_orthonormal_to_circle() {
    let center = DVec2::ZERO;
    let s = 2.0;
    let angle = 0.7_f64;
    let (c, sn) = (angle.cos(), angle.sin());
    // Rotation (orthonormal columns) scaled by s — still a Euclidean circle.
    let basis = DMat2::from_cols(DVec2::new(c, sn) * s, DVec2::new(-sn, c) * s);
    let node = GeoNode::ellipse(center, basis);

    assert!(
        variant_name(&node).starts_with("Circle"),
        "rotated-orthonormal basis must snap to a Circle, got: {}",
        variant_name(&node)
    );
    let on_boundary = node.implicit_eval_2d(&DVec2::new(s, 0.0));
    assert!(on_boundary.abs() < 1e-9, "radius should be ≈ s");
}

#[test]
fn test_ellipse_near_threshold_stays_ellipse_and_agrees() {
    let center = DVec2::ZERO;
    let s = 4.0;
    // Perturb one column length by ~1e-6 (outside the 1e-9 snap tolerance).
    let eps = 1e-6;
    let basis = DMat2::from_cols(DVec2::new(s, 0.0), DVec2::new(0.0, s * (1.0 + eps)));
    let node = GeoNode::ellipse(center, basis);
    assert!(
        variant_name(&node).starts_with("Ellipse"),
        "a basis just outside the snap tolerance must stay an Ellipse, got: {}",
        variant_name(&node)
    );

    let circle = GeoNode::circle(center, s);
    let probes = [
        DVec2::new(2.0, 0.0),
        DVec2::new(0.0, -3.0),
        DVec2::new(1.0, 1.0),
        DVec2::new(0.0, 6.0),
    ];
    for p in probes {
        let diff = (node.implicit_eval_2d(&p) - circle.implicit_eval_2d(&p)).abs();
        assert!(
            diff < 1e-3,
            "near-square ellipse SDF should stay within ~perturbation of the circle, diff = {}",
            diff
        );
    }
}

#[test]
fn test_ellipse_hash_depends_only_on_center_and_basis() {
    let center = DVec2::new(1.0, 2.0);
    let basis = skewed_basis_2d();

    let a = GeoNode::ellipse(center, basis);
    let b = GeoNode::ellipse(center, basis);
    assert_eq!(a.hash(), b.hash(), "equal center/basis must hash equal");

    // Differing basis → differing hash.
    let basis2 = DMat2::from_cols(DVec2::new(4.0, 0.0), DVec2::new(1.0, 4.0));
    let c = GeoNode::ellipse(center, basis2);
    assert_ne!(a.hash(), c.hash(), "different basis must hash differently");

    // Differing center → differing hash.
    let d = GeoNode::ellipse(DVec2::new(1.0, 3.0), basis);
    assert_ne!(a.hash(), d.hash(), "different center must hash differently");
}

#[test]
fn test_ellipse_degenerate_basis_is_empty() {
    let center = DVec2::new(1.0, 1.0);
    // Second column parallel to the first → zero determinant.
    let basis = DMat2::from_cols(DVec2::new(1.0, 0.0), DVec2::new(2.0, 0.0));
    let node = GeoNode::ellipse(center, basis);
    assert!(
        variant_name(&node).starts_with("Ellipse"),
        "degenerate basis stays an Ellipse variant (marked empty)"
    );

    for p in [center, DVec2::ZERO, DVec2::new(100.0, -50.0)] {
        assert_eq!(
            node.implicit_eval_2d(&p),
            f64::MAX,
            "degenerate ellipse must be empty (f64::MAX) everywhere"
        );
    }

    // Empty CSG sketch, no panic.
    let sketch = node
        .to_csg_sketch()
        .expect("degenerate ellipse should convert");
    assert!(
        sketch.geometry.0.is_empty(),
        "degenerate ellipse must yield an empty sketch"
    );
}

#[test]
fn test_ellipse_csg_vertices_lie_on_boundary() {
    let center = DVec2::new(1.0, 0.0);
    let basis = skewed_basis_2d();
    let node = GeoNode::ellipse(center, basis);
    let inv = basis.inverse();

    let sketch = node
        .to_csg_sketch()
        .expect("ellipse should convert to a sketch");

    // Collect the polygon-exterior vertices from the sketch geometry and assert
    // each lies on the ellipse (|inv_basis·(v−c)| ≈ 1).
    let mut checked = 0;
    for geom in &sketch.geometry.0 {
        if let geo::Geometry::Polygon(poly) = geom {
            for coord in poly.exterior().coords() {
                let p = DVec2::new(coord.x, coord.y);
                let radial = (inv * (p - center)).length();
                assert!(
                    (radial - 1.0).abs() < 1e-6,
                    "every sketch vertex must lie on the ellipse, got {}",
                    radial
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "sketch should expose polygon vertices to check"
    );

    // Vertex count matches the 36-segment circle tessellation.
    let unit_circle = GeoNode::circle(center, 1.0)
        .to_csg_sketch()
        .expect("unit circle should convert");
    let count_verts = |s: &rust_lib_flutter_cad::geo_tree::csg_types::CSGSketch| -> usize {
        s.geometry
            .0
            .iter()
            .filter_map(|g| match g {
                geo::Geometry::Polygon(p) => Some(p.exterior().coords().count()),
                _ => None,
            })
            .sum()
    };
    assert_eq!(
        count_verts(&sketch),
        count_verts(&unit_circle),
        "ellipse sketch should have the same vertex count as the circle tessellation"
    );
}

#[test]
fn test_ellipse_batch_matches_scalar() {
    let center = DVec2::new(0.5, -0.5);
    let basis = skewed_basis_2d();
    let node = GeoNode::ellipse(center, basis);

    let mut points = [DVec2::ZERO; BATCH_SIZE];
    for (i, p) in points.iter_mut().enumerate() {
        let t = i as f64 * 0.013;
        *p = center + DVec2::new(t.sin() * 6.0, (t * 1.3).cos() * 6.0);
    }

    let mut batch_results = [0.0; BATCH_SIZE];
    node.implicit_eval_2d_batch(&points, &mut batch_results);

    for i in 0..BATCH_SIZE {
        let scalar = node.implicit_eval_2d(&points[i]);
        assert!(
            (batch_results[i] - scalar).abs() < 1e-12,
            "batch and scalar SDF must agree at index {}",
            i
        );
    }
}

#[test]
fn test_ellipse_degenerate_batch_is_empty() {
    let basis = DMat2::from_cols(DVec2::new(1.0, 0.0), DVec2::new(2.0, 0.0));
    let node = GeoNode::ellipse(DVec2::ZERO, basis);
    let points = [DVec2::new(1.0, 2.0); BATCH_SIZE];
    let mut results = [0.0; BATCH_SIZE];
    node.implicit_eval_2d_batch(&points, &mut results);
    assert!(
        results.iter().all(|&r| r == f64::MAX),
        "degenerate ellipse batch must be f64::MAX everywhere"
    );
}

#[test]
fn test_extrude_of_ellipse_signs() {
    use rust_lib_flutter_cad::util::transform::Transform;

    // An extruded elliptic cylinder: the ellipse in the XY plane, extruded along
    // +Z from z=0 to z=height with an identity plane-to-world transform.
    let center = DVec2::ZERO;
    let basis = skewed_basis_2d();
    let ellipse = GeoNode::ellipse(center, basis);
    let height = 5.0;
    let extruded = GeoNode::extrude(
        height,
        DVec3::new(0.0, 0.0, 1.0),
        Box::new(ellipse),
        Transform::new(DVec3::ZERO, glam::f64::DQuat::IDENTITY),
        false,
    );

    // Inside the ellipse (xy) and within the z-slab → inside.
    let inside = DVec3::new(0.0, 0.0, 2.5);
    assert!(
        extruded.implicit_eval_3d(&inside) < 0.0,
        "point inside the elliptic cross-section and z-slab should be inside"
    );

    // Inside the ellipse (xy) but above the top cap → outside.
    let above = DVec3::new(0.0, 0.0, height + 2.0);
    assert!(
        extruded.implicit_eval_3d(&above) > 0.0,
        "point above the extrusion should be outside"
    );

    // A point far outside the ellipse in xy, within the z-slab → outside. Use a
    // far point along +X (the long axis reaches |basis·(1,0)| = 4, so x = 50 is
    // clearly outside).
    let outside_xy = DVec3::new(50.0, 0.0, 2.5);
    assert!(
        extruded.implicit_eval_3d(&outside_xy) > 0.0,
        "point outside the elliptic cross-section should be outside"
    );
}
