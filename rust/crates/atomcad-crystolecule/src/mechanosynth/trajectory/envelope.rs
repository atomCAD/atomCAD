//! The collision envelope and the sweep that finds an approach direction.
//!
//! **Pure geometry.** Nothing here knows about a [`Scene`](super::super::Scene),
//! a step or a library: an envelope, a point and a list of obstacle spheres go
//! in, a direction with its clearance comes out. That is what lets a sequence
//! generator ask the same question the replay asks, about whatever scene it
//! happens to hold.
//!
//! The envelope is a **solid of revolution** — a cone continuing into a
//! cylinder, about the tool axis, with its apex at the tool-side reaction
//! point — so the tool's roll about that axis cannot matter to collisions and
//! the search is over *directions* rather than over full orientations.
//!
//! Design doc: `doc/design_mechanosynth_trajectory.md` §The sweep.

use glam::DVec3;
use std::sync::LazyLock;

/// How many directions the sweep samples, over the whole sphere.
pub const SWEEP_DIRECTIONS: usize = 256;

/// Bisection steps taken between the first free sampled direction and `+z`, to
/// land on the smallest tilt the obstacles allow rather than on the coarsest
/// sample of it.
pub const SWEEP_REFINEMENTS: usize = 8;

/// How far outside the envelope every obstacle sphere must sit before a
/// direction counts as free, Å.
pub const CLEAR_MARGIN: f64 = 0.5;

/// By how much a later candidate has to beat the best one seen so far before it
/// is preferred to it, Å.
///
/// Sub-picometre, so it never changes which direction is really better — it
/// exists because directions **tie**. A site whose only obstacle is the host
/// atom behind the reaction point has the same clearance from every direction
/// that clears the cone, and float noise in the vector arithmetic would
/// otherwise hand the tie to whichever sample rounded upward. Ties belong to the
/// earlier candidate, which is the less tilted one.
const SWEEP_TIE_EPSILON: f64 = 1e-9;

/// A tool type's collision envelope: a cone of `half_angle` about the tool
/// axis with its apex at the tool-side reaction point, continuing as a cylinder
/// of `radius` where the cone has grown that wide.
///
/// It is the library's claim about the **instrument**, not about the wired
/// molecule: a tooltip bonded in reality to a shaft the design does not model
/// states the shaft's radius here, and the sweep keeps the shaft out of the
/// workpiece. That the molecule fits inside it is checked once, at binding
/// (`build_scene`), and is an error when it does not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    /// The cone's half-angle, **radians**, in `(0, π/2)`.
    pub half_angle: f64,
    /// The cylinder's radius, Å, positive.
    pub radius: f64,
}

impl Envelope {
    /// The axial position at which the cone reaches the cylinder's radius.
    fn rim_axial(&self) -> f64 {
        self.radius / self.half_angle.tan()
    }

    /// The rim's position measured along the cone's slant.
    fn rim_slant(&self) -> f64 {
        self.radius / self.half_angle.sin()
    }

    /// Whether a point at axial coordinate `s` and radial distance `rho` is
    /// inside the envelope.
    pub fn contains(&self, s: f64, rho: f64) -> bool {
        s > 0.0 && rho <= (s * self.half_angle.tan()).min(self.radius)
    }

    /// The signed distance from a point at axial coordinate `s` and radial
    /// distance `rho` to the envelope's **surface** — to the apex point, the
    /// cone's slant, the rim circle or the cylinder wall, whichever is nearest.
    /// Negative inside.
    ///
    /// A true distance, not the radial excess `rho − s·tan α`: the radial gap
    /// overstates the room by `1/cos α`, which is fifteen per cent at 30° and
    /// seventy-four at 55°, and an obstacle the sweep called clear by the full
    /// margin could then sit inside the cone.
    pub fn gap(&self, s: f64, rho: f64) -> f64 {
        let (sin_a, cos_a) = self.half_angle.sin_cos();

        if self.contains(s, rho) {
            // Inside: minus the distance to the nearer of the slant and the
            // wall. The slant's distance is exact while the point is short of
            // the rim and an under-estimate past it, which is the safe side.
            let to_slant = s * sin_a - rho * cos_a;
            let to_wall = self.radius - rho;
            return -to_slant.min(to_wall);
        }

        // The foot of the perpendicular onto the cone's slant, measured along
        // the slant from the apex. Negative behind the apex.
        let slant_foot = s * cos_a + rho * sin_a;
        if slant_foot <= 0.0 {
            // Behind the apex: the nearest surface point is the apex itself.
            return (s * s + rho * rho).sqrt();
        }
        if slant_foot < self.rim_slant() {
            // Beside the cone: the perpendicular lands on the slant.
            return rho * cos_a - s * sin_a;
        }
        // Past the rim: the rim circle, or the cylinder wall when the point is
        // beyond the rim axially. The infinite cone's slant is deliberately not
        // consulted here — it would call a point beside the cylinder inside.
        let rim_axial = self.rim_axial();
        let to_rim = ((s - rim_axial).powi(2) + (rho - self.radius).powi(2)).sqrt();
        if s >= rim_axial {
            to_rim.min(rho - self.radius)
        } else {
            to_rim
        }
    }

    /// The clearance of direction `d` for an envelope whose apex sits at `at`:
    /// the smallest `gap − margin` over `obstacles`, each a design-space centre
    /// with the margin its sphere claims.
    ///
    /// Positive means every obstacle sphere is outside the envelope.
    /// `f64::INFINITY` when there are no obstacles at all — open sky.
    pub fn clearance(&self, at: DVec3, d: DVec3, obstacles: &[(DVec3, f64)]) -> f64 {
        let mut worst = f64::INFINITY;
        for (position, margin) in obstacles {
            let relative = *position - at;
            let s = relative.dot(d);
            let rho = (relative - s * d).length();
            let clearance = self.gap(s, rho) - margin;
            if clearance < worst {
                worst = clearance;
            }
        }
        worst
    }
}

/// What the sweep found for one direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Approach {
    /// Unit vector the tool axis points along once the reaction points
    /// coincide. The tool arrives along `−direction` and leaves along it.
    pub direction: DVec3,
    /// The smallest margin by which an obstacle clears the envelope, Å;
    /// negative when one is inside it.
    pub clearance: f64,
    /// Angle from global `+z`, radians.
    pub tilt: f64,
}

impl Approach {
    fn new(direction: DVec3, clearance: f64) -> Self {
        Self {
            direction,
            clearance,
            tilt: direction.z.clamp(-1.0, 1.0).acos(),
        }
    }
}

/// The [`SWEEP_DIRECTIONS`] unit vectors the sweep walks, a Fibonacci sphere
/// **generated from the north pole down**, so that index order is tilt order:
/// entry 0 is `+z`, the last is `−z`, and the tilt never decreases along the
/// sequence. The sequence is fixed, so the same scene gives the same direction
/// every time.
pub fn sweep_directions() -> &'static [DVec3] {
    static DIRECTIONS: LazyLock<Vec<DVec3>> = LazyLock::new(|| {
        // The golden angle; any irrational turn would do, and this one spreads
        // consecutive samples as evenly as a turn can.
        let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
        let last = (SWEEP_DIRECTIONS - 1) as f64;
        (0..SWEEP_DIRECTIONS)
            .map(|index| {
                let z = 1.0 - 2.0 * (index as f64) / last;
                let radius = (1.0 - z * z).max(0.0).sqrt();
                let theta = golden * index as f64;
                DVec3::new(radius * theta.cos(), radius * theta.sin(), z).normalize()
            })
            .collect()
    });
    &DIRECTIONS
}

/// The free direction of least tilt from `+z` for a tool whose reaction point
/// sits at `at`, or — when no sampled direction is free — the least blocked one.
///
/// **Always an answer**, the clearance carrying the verdict: a positive
/// clearance means the site is reachable along that direction, a negative one
/// that it is blocked and that this is merely the best of a bad set. Who acts
/// on the verdict is the caller's business: a sequence generator refuses to
/// emit the step, a replay visits anyway and reports it.
///
/// `obstacles` are design-space positions with their margins; see
/// [`obstacles_for`](super::landing::obstacles_for), the one function that
/// decides what an obstacle is.
///
/// The preferred direction is global `+z`: a scanning probe is vertical unless
/// something forces a tilt, and the workpiece's top face is the `xy` plane by
/// every convention of this application. The walk stops at the **first** free
/// candidate, so an unobstructed site costs one pass over the obstacles.
pub fn approach_direction(envelope: &Envelope, at: DVec3, obstacles: &[(DVec3, f64)]) -> Approach {
    let directions = sweep_directions();

    let mut best = Approach::new(directions[0], f64::NEG_INFINITY);
    for (index, direction) in directions.iter().enumerate() {
        let clearance = envelope.clearance(at, *direction, obstacles);
        if clearance >= CLEAR_MARGIN {
            if index == 0 {
                // Vertical and free: there is nothing to refine toward.
                return Approach::new(*direction, clearance);
            }
            return refine(envelope, at, obstacles, *direction, clearance);
        }
        if clearance > best.clearance + SWEEP_TIE_EPSILON {
            best = Approach::new(*direction, clearance);
        }
    }
    best
}

/// Bisects the great-circle arc from a free direction toward `+z`, keeping the
/// last direction whose clearance held — so the tilt is the smallest the
/// obstacles allow rather than the coarsest sample of it.
fn refine(
    envelope: &Envelope,
    at: DVec3,
    obstacles: &[(DVec3, f64)],
    free: DVec3,
    free_clearance: f64,
) -> Approach {
    let preferred = DVec3::Z;
    // Antipodal to `+z`: no great circle between the two is determined, and a
    // straight-down approach has nothing sensible to bisect toward anyway.
    if free.cross(preferred).length() < 1e-9 {
        return Approach::new(free, free_clearance);
    }

    let mut lower = free;
    let mut lower_clearance = free_clearance;
    let mut upper = preferred;
    for _ in 0..SWEEP_REFINEMENTS {
        let bisector = lower + upper;
        if bisector.length() < 1e-9 {
            // The two ends have drifted antipodal; there is no midpoint to take.
            break;
        }
        let middle = bisector.normalize();
        let clearance = envelope.clearance(at, middle, obstacles);
        if clearance >= CLEAR_MARGIN {
            lower = middle;
            lower_clearance = clearance;
        } else {
            upper = middle;
        }
    }
    Approach::new(lower, lower_clearance)
}
