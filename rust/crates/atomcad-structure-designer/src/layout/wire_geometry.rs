//! Wires as Flutter draws them: where a wire runs, for the layout report.
//!
//! `node_network_painter.dart`'s `_getPath` draws every wire as one cubic from
//! the source pin to the destination pin, with both control points
//! [`WIRE_CONTROL_OFFSET`] out horizontally — rightward from the source,
//! leftward into the destination. A [`WireCurve`] is that cubic flattened to a
//! polyline, with a bounding box that is cheap to test first.
//!
//! The box is exact in y and conservative in x: both control points share their
//! own pin's height, so the curve never leaves the band between the two pin
//! heights, and it never leaves the hull of its four x values. Box tests can
//! therefore reject a pair without ever missing a hit.

use glam::DVec2;

use super::motion::Rect;

/// Horizontal distance of a wire's control points from its pins. Flutter's
/// `CUBIC_SPLINE_HORIZ_OFFSET` (`node_network.dart`).
pub const WIRE_CONTROL_OFFSET: f64 = 50.0;

/// Straight segments a wire is flattened into. A 200 px wire is cut into
/// ~8 px pieces, well under a node's size and a pin row's height.
const WIRE_SEGMENTS: usize = 24;

/// One wire, flattened.
#[derive(Debug, Clone)]
pub struct WireCurve {
    points: Vec<DVec2>,
    bounds: Rect,
}

impl WireCurve {
    /// The wire from the output pin at `from` to the input pin at `to`.
    pub fn new(from: DVec2, to: DVec2) -> Self {
        let c1 = from + DVec2::new(WIRE_CONTROL_OFFSET, 0.0);
        let c2 = to - DVec2::new(WIRE_CONTROL_OFFSET, 0.0);
        let points = (0..=WIRE_SEGMENTS)
            .map(|i| {
                let t = i as f64 / WIRE_SEGMENTS as f64;
                let u = 1.0 - t;
                from * (u * u * u)
                    + c1 * (3.0 * u * u * t)
                    + c2 * (3.0 * u * t * t)
                    + to * (t * t * t)
            })
            .collect();
        let min = DVec2::new(from.x.min(c2.x), from.y.min(to.y));
        let max = DVec2::new(c1.x.max(to.x), from.y.max(to.y));
        Self {
            points,
            bounds: Rect::new(min, max - min),
        }
    }

    /// A box the whole curve lies in.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// How far the wire runs inside `rect`'s interior, in px; `0.0` when it
    /// does not enter it. Touching an edge is not entering.
    pub fn length_inside(&self, rect: Rect) -> f64 {
        if !boxes_meet(self.bounds, rect) {
            return 0.0;
        }
        self.points
            .windows(2)
            .map(|pair| clipped_length(pair[0], pair[1], rect))
            .sum()
    }

    /// Whether the two wires cross. A shared end point counts too, so callers
    /// skip pairs that share a pin.
    pub fn crosses(&self, other: &WireCurve) -> bool {
        if !boxes_meet(self.bounds, other.bounds) {
            return false;
        }
        for a in self.points.windows(2) {
            let a_box = segment_box(a[0], a[1]);
            if !boxes_meet(a_box, other.bounds) {
                continue;
            }
            for b in other.points.windows(2) {
                if segments_cross(a[0], a[1], b[0], b[1]) {
                    return true;
                }
            }
        }
        false
    }
}

/// Whether two boxes meet. Touching counts: this only ever rejects pairs.
fn boxes_meet(a: Rect, b: Rect) -> bool {
    a.left() <= b.right() && b.left() <= a.right() && a.top() <= b.bottom() && b.top() <= a.bottom()
}

fn segment_box(p: DVec2, q: DVec2) -> Rect {
    let min = p.min(q);
    Rect::new(min, p.max(q) - min)
}

/// The length of segment `p`–`q` inside `rect` (Liang–Barsky clipping). A
/// segment lying on an edge, or touching one corner, has length zero.
fn clipped_length(p: DVec2, q: DVec2, rect: Rect) -> f64 {
    let d = q - p;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (denominator, numerator) in [
        (-d.x, p.x - rect.left()),
        (d.x, rect.right() - p.x),
        (-d.y, p.y - rect.top()),
        (d.y, rect.bottom() - p.y),
    ] {
        if denominator == 0.0 {
            // Parallel to this edge: inside the open slab, or no overlap.
            if numerator <= 0.0 {
                return 0.0;
            }
            continue;
        }
        let t = numerator / denominator;
        if denominator < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 >= t1 {
            return 0.0;
        }
    }
    (t1 - t0) * d.length()
}

/// Whether segments `a`–`b` and `c`–`d` meet: each pair of end points on
/// opposite sides of, or on, the other segment.
///
/// Touching counts, because two mirror-image wires cross exactly at a sample
/// point (the midpoint of both), and a grid layout makes mirror images all the
/// time. Collinear segments do not: two wires running along one line are not
/// a crossing.
fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let d1 = side(c, d, a);
    let d2 = side(c, d, b);
    let d3 = side(a, b, c);
    let d4 = side(a, b, d);
    if d1 == 0.0 && d2 == 0.0 {
        return false;
    }
    d1 * d2 <= 0.0 && d3 * d4 <= 0.0
}
