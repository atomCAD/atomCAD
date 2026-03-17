use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::geo_tree::implicit_geometry::{ImplicitGeometry2D, ImplicitGeometry3D};

const EPSILON: f64 = 0.0001;

// =============================================================================
// 3D Primitive Tests
// =============================================================================

#[test]
fn test_sphere_center_is_negative() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    // Center should be inside (negative SDF)
    let center_value = sphere.implicit_eval_3d(&DVec3::ZERO);
    assert!(
        center_value < 0.0,
        "Center of sphere should be inside (negative SDF)"
    );
    assert!(
        (center_value + 1.0).abs() < EPSILON,
        "SDF at center should be -radius"
    );
}

#[test]
fn test_sphere_surface_is_zero() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 2.0);

    // Points on surface should have SDF ≈ 0
    let on_surface_x = sphere.implicit_eval_3d(&DVec3::new(2.0, 0.0, 0.0));
    let on_surface_y = sphere.implicit_eval_3d(&DVec3::new(0.0, 2.0, 0.0));
    let on_surface_z = sphere.implicit_eval_3d(&DVec3::new(0.0, 0.0, 2.0));

    assert!(
        on_surface_x.abs() < EPSILON,
        "Point on +X surface should be ≈ 0"
    );
    assert!(
        on_surface_y.abs() < EPSILON,
        "Point on +Y surface should be ≈ 0"
    );
    assert!(
        on_surface_z.abs() < EPSILON,
        "Point on +Z surface should be ≈ 0"
    );
}

#[test]
fn test_sphere_outside_is_positive() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    // Points outside should have positive SDF
    let outside = sphere.implicit_eval_3d(&DVec3::new(3.0, 0.0, 0.0));
    assert!(outside > 0.0, "Point outside sphere should be positive");
    assert!(
        (outside - 2.0).abs() < EPSILON,
        "SDF at distance 3 should be 2"
    );
}

#[test]
fn test_sphere_with_offset_center() {
    let sphere = GeoNode::sphere(DVec3::new(5.0, 0.0, 0.0), 1.0);

    // Origin should be outside
    let at_origin = sphere.implicit_eval_3d(&DVec3::ZERO);
    assert!(at_origin > 0.0, "Origin should be outside offset sphere");
    assert!(
        (at_origin - 4.0).abs() < EPSILON,
        "SDF at origin should be 4"
    );

    // Center should be inside
    let at_center = sphere.implicit_eval_3d(&DVec3::new(5.0, 0.0, 0.0));
    assert!(at_center < 0.0, "Center should be inside");
}

#[test]
fn test_half_space_basic() {
    // Half space with normal pointing +Y, centered at origin
    let half_space = GeoNode::half_space(DVec3::new(0.0, 1.0, 0.0), DVec3::ZERO);

    // Points above should be positive (outside)
    let above = half_space.implicit_eval_3d(&DVec3::new(0.0, 5.0, 0.0));
    assert!(above > 0.0, "Points above should be outside");

    // Points below should be negative (inside)
    let below = half_space.implicit_eval_3d(&DVec3::new(0.0, -5.0, 0.0));
    assert!(below < 0.0, "Points below should be inside");

    // Points on plane should be zero
    let on_plane = half_space.implicit_eval_3d(&DVec3::new(10.0, 0.0, 10.0));
    assert!(on_plane.abs() < EPSILON, "Points on plane should be zero");
}

#[test]
fn test_half_space_with_offset() {
    // Half space at y=5
    let half_space = GeoNode::half_space(DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, 5.0, 0.0));

    // Origin should be inside (below the plane)
    let at_origin = half_space.implicit_eval_3d(&DVec3::ZERO);
    assert!(at_origin < 0.0, "Origin should be inside");
    assert!(
        (at_origin + 5.0).abs() < EPSILON,
        "SDF at origin should be -5"
    );
}

// =============================================================================
// 2D Primitive Tests
// =============================================================================

#[test]
fn test_circle_center_is_negative() {
    let circle = GeoNode::circle(DVec2::ZERO, 1.0);

    let center_value = circle.implicit_eval_2d(&DVec2::ZERO);
    assert!(center_value < 0.0, "Center of circle should be inside");
    assert!(
        (center_value + 1.0).abs() < EPSILON,
        "SDF at center should be -radius"
    );
}

#[test]
fn test_circle_surface_is_zero() {
    let circle = GeoNode::circle(DVec2::ZERO, 3.0);

    let on_surface_x = circle.implicit_eval_2d(&DVec2::new(3.0, 0.0));
    let on_surface_y = circle.implicit_eval_2d(&DVec2::new(0.0, 3.0));

    assert!(
        on_surface_x.abs() < EPSILON,
        "Point on +X edge should be ≈ 0"
    );
    assert!(
        on_surface_y.abs() < EPSILON,
        "Point on +Y edge should be ≈ 0"
    );
}

#[test]
fn test_circle_outside_is_positive() {
    let circle = GeoNode::circle(DVec2::ZERO, 1.0);

    let outside = circle.implicit_eval_2d(&DVec2::new(4.0, 0.0));
    assert!(outside > 0.0, "Point outside circle should be positive");
    assert!(
        (outside - 3.0).abs() < EPSILON,
        "SDF at distance 4 should be 3"
    );
}

#[test]
fn test_half_plane_basic() {
    // Half plane defined by line from (0,0) to (1,0)
    // The normal is perpendicular to line direction, rotated 90 degrees CCW
    // For direction +X (1,0), normal is +Y (0,1)
    // Points in the direction of normal (above) are positive (outside)
    // Points opposite to normal (below) are negative (inside)
    let half_plane = GeoNode::half_plane(DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0));

    // Points above (in +Y) are in the direction of normal = positive = outside
    let above = half_plane.implicit_eval_2d(&DVec2::new(0.5, 1.0));
    assert!(
        above > 0.0,
        "Points above line should be outside (positive SDF)"
    );

    // Points below (in -Y) are opposite to normal = negative = inside
    let below = half_plane.implicit_eval_2d(&DVec2::new(0.5, -1.0));
    assert!(
        below < 0.0,
        "Points below line should be inside (negative SDF)"
    );

    // Points on line should be zero
    let on_line = half_plane.implicit_eval_2d(&DVec2::new(0.5, 0.0));
    assert!(on_line.abs() < EPSILON, "Points on line should be zero");
}

// =============================================================================
// CSG Operation Tests - 3D
// =============================================================================

#[test]
fn test_union_3d_basic() {
    let sphere1 = GeoNode::sphere(DVec3::new(-1.0, 0.0, 0.0), 1.0);
    let sphere2 = GeoNode::sphere(DVec3::new(1.0, 0.0, 0.0), 1.0);
    let union = GeoNode::union_3d(vec![sphere1, sphere2]);

    // Center of first sphere should be inside
    let in_sphere1 = union.implicit_eval_3d(&DVec3::new(-1.0, 0.0, 0.0));
    assert!(in_sphere1 < 0.0, "Center of sphere1 should be inside union");

    // Center of second sphere should be inside
    let in_sphere2 = union.implicit_eval_3d(&DVec3::new(1.0, 0.0, 0.0));
    assert!(in_sphere2 < 0.0, "Center of sphere2 should be inside union");

    // Far point should be outside
    let far_point = union.implicit_eval_3d(&DVec3::new(10.0, 0.0, 0.0));
    assert!(far_point > 0.0, "Far point should be outside union");
}

#[test]
fn test_intersection_3d_basic() {
    let sphere1 = GeoNode::sphere(DVec3::new(-0.5, 0.0, 0.0), 1.0);
    let sphere2 = GeoNode::sphere(DVec3::new(0.5, 0.0, 0.0), 1.0);
    let intersection = GeoNode::intersection_3d(vec![sphere1, sphere2]);

    // Origin should be inside both spheres, so inside intersection
    let at_origin = intersection.implicit_eval_3d(&DVec3::ZERO);
    assert!(at_origin < 0.0, "Origin should be inside intersection");

    // Far point should be outside
    let far_point = intersection.implicit_eval_3d(&DVec3::new(10.0, 0.0, 0.0));
    assert!(far_point > 0.0, "Far point should be outside intersection");
}

#[test]
fn test_difference_3d_basic() {
    let base = GeoNode::sphere(DVec3::ZERO, 2.0);
    let sub = GeoNode::sphere(DVec3::ZERO, 1.0);
    let difference = GeoNode::difference_3d(Box::new(base), Box::new(sub));

    // Origin should be outside (was carved out)
    let at_origin = difference.implicit_eval_3d(&DVec3::ZERO);
    assert!(at_origin > 0.0, "Origin should be outside (carved out)");

    // Point between radii should be inside
    let between_radii = difference.implicit_eval_3d(&DVec3::new(1.5, 0.0, 0.0));
    assert!(between_radii < 0.0, "Point between radii should be inside");

    // Point outside both should be outside
    let outside = difference.implicit_eval_3d(&DVec3::new(3.0, 0.0, 0.0));
    assert!(outside > 0.0, "Point outside both should be outside");
}

// =============================================================================
// CSG Operation Tests - 2D
// =============================================================================

#[test]
fn test_union_2d_basic() {
    let circle1 = GeoNode::circle(DVec2::new(-1.0, 0.0), 1.0);
    let circle2 = GeoNode::circle(DVec2::new(1.0, 0.0), 1.0);
    let union = GeoNode::union_2d(vec![circle1, circle2]);

    // Center of first circle should be inside
    let in_circle1 = union.implicit_eval_2d(&DVec2::new(-1.0, 0.0));
    assert!(in_circle1 < 0.0, "Center of circle1 should be inside union");

    // Center of second circle should be inside
    let in_circle2 = union.implicit_eval_2d(&DVec2::new(1.0, 0.0));
    assert!(in_circle2 < 0.0, "Center of circle2 should be inside union");
}

#[test]
fn test_intersection_2d_basic() {
    let circle1 = GeoNode::circle(DVec2::new(-0.5, 0.0), 1.0);
    let circle2 = GeoNode::circle(DVec2::new(0.5, 0.0), 1.0);
    let intersection = GeoNode::intersection_2d(vec![circle1, circle2]);

    // Origin should be inside both circles
    let at_origin = intersection.implicit_eval_2d(&DVec2::ZERO);
    assert!(at_origin < 0.0, "Origin should be inside intersection");
}

#[test]
fn test_difference_2d_basic() {
    let base = GeoNode::circle(DVec2::ZERO, 2.0);
    let sub = GeoNode::circle(DVec2::ZERO, 1.0);
    let difference = GeoNode::difference_2d(Box::new(base), Box::new(sub));

    // Origin should be outside (carved out)
    let at_origin = difference.implicit_eval_2d(&DVec2::ZERO);
    assert!(at_origin > 0.0, "Origin should be outside (carved out)");

    // Point between radii should be inside
    let between_radii = difference.implicit_eval_2d(&DVec2::new(1.5, 0.0));
    assert!(between_radii < 0.0, "Point between radii should be inside");
}

// =============================================================================
// Gradient Tests
// =============================================================================

#[test]
fn test_sphere_gradient() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);

    // Gradient at point on +X axis should point outward in +X direction
    let (gradient, value) = sphere.get_gradient(&DVec3::new(2.0, 0.0, 0.0));

    assert!(gradient.x > 0.5, "Gradient should point +X");
    assert!(gradient.y.abs() < 0.1, "Gradient Y should be near zero");
    assert!(gradient.z.abs() < 0.1, "Gradient Z should be near zero");
    assert!(
        (value - 1.0).abs() < EPSILON,
        "Value should be distance - radius = 1.0"
    );
}

#[test]
fn test_circle_gradient_2d() {
    let circle = GeoNode::circle(DVec2::ZERO, 1.0);

    // Gradient at point on +X axis should point outward
    let (gradient, value) = circle.get_gradient_2d(&DVec2::new(2.0, 0.0));

    assert!(gradient.x > 0.5, "Gradient should point +X");
    assert!(gradient.y.abs() < 0.1, "Gradient Y should be near zero");
    assert!(
        (value - 1.0).abs() < EPSILON,
        "Value should be distance - radius = 1.0"
    );
}

// =============================================================================
// Dimension Classification Tests
// =============================================================================

#[test]
fn test_is_2d() {
    let circle = GeoNode::circle(DVec2::ZERO, 1.0);
    assert!(circle.is2d(), "Circle should be 2D");

    let half_plane = GeoNode::half_plane(DVec2::ZERO, DVec2::new(1.0, 0.0));
    assert!(half_plane.is2d(), "Half plane should be 2D");

    let union_2d = GeoNode::union_2d(vec![
        GeoNode::circle(DVec2::ZERO, 1.0),
        GeoNode::circle(DVec2::new(1.0, 0.0), 1.0),
    ]);
    assert!(union_2d.is2d(), "Union2D should be 2D");
}

#[test]
fn test_is_3d() {
    let sphere = GeoNode::sphere(DVec3::ZERO, 1.0);
    assert!(sphere.is3d(), "Sphere should be 3D");

    let half_space = GeoNode::half_space(DVec3::new(0.0, 1.0, 0.0), DVec3::ZERO);
    assert!(half_space.is3d(), "Half space should be 3D");

    let union_3d = GeoNode::union_3d(vec![
        GeoNode::sphere(DVec3::ZERO, 1.0),
        GeoNode::sphere(DVec3::new(1.0, 0.0, 0.0), 1.0),
    ]);
    assert!(union_3d.is3d(), "Union3D should be 3D");
}

// =============================================================================
// Empty Collection Tests
// =============================================================================

#[test]
fn test_union_3d_empty() {
    let union = GeoNode::union_3d(vec![]);

    // Empty union should return MAX (outside everything)
    let value = union.implicit_eval_3d(&DVec3::ZERO);
    assert!(
        value > 1e10,
        "Empty union should return very large positive value"
    );
}

#[test]
fn test_intersection_3d_empty() {
    let intersection = GeoNode::intersection_3d(vec![]);

    // Empty intersection should return MIN (inside everything)
    let value = intersection.implicit_eval_3d(&DVec3::ZERO);
    assert!(
        value < -1e10,
        "Empty intersection should return very large negative value"
    );
}

#[test]
fn test_union_2d_empty() {
    let union = GeoNode::union_2d(vec![]);

    let value = union.implicit_eval_2d(&DVec2::ZERO);
    assert!(
        value > 1e10,
        "Empty union should return very large positive value"
    );
}

#[test]
fn test_intersection_2d_empty() {
    let intersection = GeoNode::intersection_2d(vec![]);

    let value = intersection.implicit_eval_2d(&DVec2::ZERO);
    assert!(
        value < -1e10,
        "Empty intersection should return very large negative value"
    );
}
