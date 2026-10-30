//! `layout::wire_geometry`: wires as Flutter draws them.

use glam::DVec2;

use atomcad_structure_designer::layout::Rect;
use atomcad_structure_designer::layout::wire_geometry::{WIRE_CONTROL_OFFSET, WireCurve};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(DVec2::new(x, y), DVec2::new(w, h))
}

/// The bounding box is what lets every pair be rejected cheaply, so it must
/// hold the whole curve: forward, backward (the control points overshoot
/// both ends by the offset) and steep.
#[test]
fn the_bounds_hold_the_whole_curve() {
    for (from, to) in [
        (DVec2::new(0.0, 0.0), DVec2::new(400.0, 0.0)),
        (DVec2::new(400.0, 0.0), DVec2::new(0.0, 300.0)),
        (DVec2::new(0.0, 500.0), DVec2::new(60.0, 0.0)),
    ] {
        let curve = WireCurve::new(from, to);
        let bounds = curve.bounds();
        assert_eq!(bounds.top(), from.y.min(to.y));
        assert_eq!(bounds.bottom(), from.y.max(to.y));
        // A node just outside the box is never entered.
        for outside in [
            rect(bounds.right() + 0.5, bounds.top(), 100.0, bounds.size.y),
            rect(bounds.left() - 100.5, bounds.top(), 100.0, bounds.size.y),
            rect(bounds.left(), bounds.bottom() + 0.5, bounds.size.x, 100.0),
        ] {
            assert_eq!(curve.length_inside(outside), 0.0);
        }
    }
    let backward = WireCurve::new(DVec2::new(400.0, 0.0), DVec2::new(0.0, 300.0));
    assert_eq!(backward.bounds().left(), -WIRE_CONTROL_OFFSET);
    assert_eq!(backward.bounds().right(), 400.0 + WIRE_CONTROL_OFFSET);
}

/// A level wire through a box is hidden for the box's width; just above it,
/// for nothing.
#[test]
fn a_level_wire_is_hidden_for_the_width_it_crosses() {
    let curve = WireCurve::new(DVec2::new(0.0, 50.0), DVec2::new(600.0, 50.0));
    assert!((curve.length_inside(rect(200.0, 0.0, 160.0, 100.0)) - 160.0).abs() < 1e-9);
    assert_eq!(curve.length_inside(rect(200.0, 51.0, 160.0, 100.0)), 0.0);
}

/// A steep wire is an S-curve, not the rectangle between its pins: a box in
/// the rectangle's top-right corner passes the box test and the curve still
/// misses it, so the curve test is what decides.
#[test]
fn a_steep_wire_misses_a_box_its_pin_band_covers() {
    let curve = WireCurve::new(DVec2::new(0.0, 0.0), DVec2::new(400.0, 400.0));
    let top_right = rect(300.0, 20.0, 60.0, 60.0);
    assert!(top_right.left() < curve.bounds().right());
    assert_eq!(curve.length_inside(top_right), 0.0);
    assert!(curve.length_inside(rect(170.0, 170.0, 60.0, 60.0)) > 0.0);
}

#[test]
fn swapped_ends_cross_and_parallel_wires_do_not() {
    let a = WireCurve::new(DVec2::new(0.0, 0.0), DVec2::new(400.0, 200.0));
    let b = WireCurve::new(DVec2::new(0.0, 200.0), DVec2::new(400.0, 0.0));
    let c = WireCurve::new(DVec2::new(0.0, 100.0), DVec2::new(400.0, 300.0));
    assert!(a.crosses(&b));
    assert!(b.crosses(&a));
    assert!(!a.crosses(&c));
}
