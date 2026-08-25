//! Turning a color field's value into per-vertex albedo.
//!
//! The domain is never auto-fitted — see
//! [`IsosurfaceColoring::Field::range`](atomcad_crystolecule::field::IsosurfaceColoring)
//! for why. Out-of-domain values clamp to the ramp's ends, which is also what a
//! surface extending past the *color* field's box gets: `ScalarField::sample`
//! returns `0.0` there, so the overhang paints as whatever `0.0` maps to
//! (neutral, for a symmetric ESP domain). That is a documented sharp edge of
//! the node, not a bug to be papered over here — the design deliberately
//! refuses to add a second out-of-bounds convention.

use atomcad_crystolecule::field::Colormap;
use glam::Vec3;

// The ends are saturated in their own channel and dark in the other, so the
// red channel rises monotonically across the ramp while the blue one falls —
// the property a reader uses to tell sign from a still image.
const BLUE: Vec3 = Vec3::new(0.10, 0.25, 1.0);
const WHITE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
const RED: Vec3 = Vec3::new(1.0, 0.20, 0.15);

/// Color for one field value under `colormap`, with `range` as the domain.
///
/// A degenerate domain (`max <= min`, including a hand-entered pair the wrong
/// way round) maps everything to the ramp's midpoint rather than dividing by
/// zero.
pub fn sample_colormap(colormap: Colormap, value: f64, range: (f64, f64)) -> Vec3 {
    let (min, max) = range;
    let t = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
    } else {
        0.5
    };
    match colormap {
        Colormap::BlueWhiteRed => {
            if t <= 0.5 {
                BLUE.lerp(WHITE, t * 2.0)
            } else {
                WHITE.lerp(RED, (t - 0.5) * 2.0)
            }
        }
    }
}
