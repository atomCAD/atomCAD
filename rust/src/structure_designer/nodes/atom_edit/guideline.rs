//! Atom placement **guideline** geometry (issue #368).
//!
//! A guideline is a temporary line in 3D space that constrains atom placement to
//! positions that are hard to hit by free clicking (e.g. the ad-atom site of a
//! Si(111) √3×√3 R30° reconstruction, equidistant from three surface atoms).
//!
//! This module is Phase 1: the **pure geometry** plus the `Guideline` value type,
//! isolated from all interaction. No `AtomEditData` wiring lives here.
//!
//! The struct stays in `atom_edit` rather than a domain-free geometry util because
//! it carries interaction state (`t` / `snapped`); only the pure helpers
//! (circumcenter, decomposition, ray↔line) would be candidates for factoring out
//! into a `crystolecule` util if they ever prove reusable. See
//! `doc/atom_edit/design_atom_guidelines.md`.

use glam::f64::DVec3;

/// Length tolerance (Å) below which a vector is treated as zero-length.
/// Guards the coincident-pair and zero-direction degeneracy cases.
const LENGTH_EPSILON: f64 = 1.0e-6;

/// Circumradius cap (Å). A triangle whose circumradius exceeds this is treated as
/// (near-)collinear — atoms sit at most a few Å apart, so a circumradius this large
/// (10 µm) is physically meaningless and numerically unstable. This catches the
/// *near*-collinear case that an exact area==0 test misses.
const CIRCUMRADIUS_CAP: f64 = 1.0e4;

/// Squared-area guard for the circumcenter division. The cross product `u × v` has
/// magnitude `2 · area`; when its squared length is below this the triangle is
/// exactly (or essentially) collinear and the circumcenter is undefined.
const AREA_SQ_EPSILON: f64 = 1.0e-18;

/// `sin²θ` threshold below which a ray is treated as parallel to the guideline
/// (θ ≈ 1e-6 rad). Used by `closest_t_to_ray`.
const PARALLEL_SIN_SQ_EPSILON: f64 = 1.0e-12;

/// Reasons a guideline cannot be constructed from the current selection.
///
/// Degeneracy is detected with **tolerances**, not exact tests (see the module
/// constants): a near-collinear triangle or a near-coincident pair is rejected
/// just like the exact-degenerate case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GuidelineError {
    /// Three atoms whose circumradius is undefined or numerically unstable — the
    /// triangle is collinear or nearly so.
    #[error("the three selected atoms are collinear or nearly collinear")]
    Collinear,
    /// Two atoms that are coincident or near-coincident.
    #[error("the two selected atoms are coincident or nearly coincident")]
    Coincident,
    /// A one-atom directional line with a zero-/near-zero-length direction.
    #[error("the entered direction is zero-length or nearly zero-length")]
    ZeroDirection,
}

/// A placement guideline: a frozen line (`origin` + unit `direction`) plus the
/// current 1D position `t` (signed Å from `origin` along `direction`) and a
/// transient `snapped` mode bit.
///
/// All fields are transient interaction state — a `Guideline` is *not* serialized
/// to `.cnnd` and is *not* part of undo/redo history.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guideline {
    /// A point on the line.
    pub origin: DVec3,
    /// Unit direction along the line. Its sign is deterministic (selection order
    /// or the entered vector) so `t` is reproducible.
    pub direction: DVec3,
    /// Current 1D position along the line (signed Å from `origin`).
    pub t: f64,
    /// Whether the selected atom is locked onto the line. Transient mode bit; see
    /// the design doc's "Snap to guideline" section.
    pub snapped: bool,
}

impl Guideline {
    /// Build a guideline from a frozen `origin` and unit `direction`, with `t = 0`
    /// and `snapped = false`. `direction` is assumed already normalized (the
    /// `from_*` constructors below produce unit directions).
    pub fn new(origin: DVec3, direction: DVec3) -> Self {
        Self {
            origin,
            direction,
            t: 0.0,
            snapped: false,
        }
    }

    /// Equidistant line for **three** selected atoms: `origin` = circumcenter of the
    /// triangle, `direction` = triangle normal (normalized). Every point on the line
    /// is equidistant from all three atoms.
    ///
    /// The normal sign follows selection order (`(b-a) × (c-a)`). Returns
    /// [`GuidelineError::Collinear`] for a collinear or near-collinear triangle.
    pub fn from_three_atoms(
        a: DVec3,
        b: DVec3,
        c: DVec3,
    ) -> Result<(DVec3, DVec3), GuidelineError> {
        let u = b - a;
        let v = c - a;
        let n = u.cross(v);
        let n_len_sq = n.length_squared();
        if n_len_sq < AREA_SQ_EPSILON {
            // Exact (or essentially exact) collinearity: circumcenter is undefined.
            return Err(GuidelineError::Collinear);
        }

        // Circumcenter relative to vertex `a` (one vertex at the origin), with
        // p = u, q = v:
        //   O - a = ((|u|² v - |v|² u) × (u × v)) / (2 |u × v|²)
        let circumcenter =
            a + ((u.length_squared() * v - v.length_squared() * u).cross(n)) / (2.0 * n_len_sq);

        // Equidistant ⇒ the circumradius is the distance from the circumcenter to
        // any vertex. A huge radius means near-collinear (the area test alone misses
        // it for well-separated, barely-bent triangles).
        let circumradius = (circumcenter - a).length();
        if circumradius > CIRCUMRADIUS_CAP {
            return Err(GuidelineError::Collinear);
        }

        Ok((circumcenter, n.normalize()))
    }

    /// Center line for **two** selected atoms: `origin` = midpoint, `direction` =
    /// normalized `a → b` (by selection order). Returns
    /// [`GuidelineError::Coincident`] when the atoms are coincident or near-coincident.
    pub fn from_two_atoms(a: DVec3, b: DVec3) -> Result<(DVec3, DVec3), GuidelineError> {
        let dir = b - a;
        if dir.length() < LENGTH_EPSILON {
            return Err(GuidelineError::Coincident);
        }
        let origin = (a + b) * 0.5;
        Ok((origin, dir.normalize()))
    }

    /// Directional line for **one** selected atom: `origin` = the atom, `direction` =
    /// normalized entered `dir`. Returns [`GuidelineError::ZeroDirection`] when `dir`
    /// is zero-/near-zero-length.
    pub fn from_one_atom(p: DVec3, dir: DVec3) -> Result<(DVec3, DVec3), GuidelineError> {
        if dir.length() < LENGTH_EPSILON {
            return Err(GuidelineError::ZeroDirection);
        }
        Ok((p, dir.normalize()))
    }

    /// Decompose a point relative to the line into its along-line projection `t`
    /// (signed Å from `origin`) and the perpendicular offset vector from the line to
    /// the point (its length is the point's distance from the line).
    pub fn decompose(&self, point: DVec3) -> (f64, DVec3) {
        let rel = point - self.origin;
        let t = rel.dot(self.direction);
        let offset = rel - t * self.direction;
        (t, offset)
    }

    /// The point on the line at along-line position `t`: `origin + t · direction`.
    pub fn point_at(&self, t: f64) -> DVec3 {
        self.origin + t * self.direction
    }

    /// The along-line position `t` of the point on the guideline closest to the given
    /// ray. Returns `None` when the ray is parallel to the line (no unique closest
    /// point — the caller ignores the click).
    pub fn closest_t_to_ray(&self, ray_origin: DVec3, ray_dir: DVec3) -> Option<f64> {
        // Closest points between two lines (geomalgorithms / Ericson). The guideline
        // is L1(t) = origin + t·direction (direction is unit), the ray is
        // L2(s) = ray_origin + s·ray_dir. We solve for the guideline parameter t.
        let w0 = self.origin - ray_origin;
        // a = direction·direction = 1 (unit); b, c, d, e as in the reference.
        let b = self.direction.dot(ray_dir);
        let c = ray_dir.dot(ray_dir);
        let d = self.direction.dot(w0);
        let e = ray_dir.dot(w0);
        // denom = a·c - b² = c - b² = |direction × ray_dir|² = c · sin²θ ≥ 0.
        let denom = c - b * b;
        if denom < PARALLEL_SIN_SQ_EPSILON * c {
            // Ray parallel to the guideline: no unique foot.
            return None;
        }
        Some((b * e - c * d) / denom)
    }
}
