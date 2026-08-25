//! Shared fixtures and invariant helpers for the isosurface extractor tests.
//!
//! `doc/design_isosurface_node.md` P2 asks for the invariants as *shared
//! helpers* rather than per-test assertions, so that every table applies all of
//! them for free and the fuzz rows stay three lines each. The earlier draft of
//! that plan named closedness once, against the sphere; it belongs on
//! everything.
//!
//! # Why the analytic fixtures decay instead of being signed distances
//!
//! The design's table C names "analytic sphere (`r - R`), level 0". A signed
//! distance is the wrong shape for an *extraction box* test: its `psi >= 0`
//! region is unbounded, so the positive pass's level set leaves
//! `suggested_bounds()` and the extractor — which marches exactly the lattice
//! the design specifies, with no capping layer — returns a surface that is open
//! where it was clipped. Every fixture here is therefore a decaying bump, whose
//! level set at a positive level is a bounded closed surface of an exactly
//! known radius. The assertions the design asks for (vertices at `R`, radial
//! outward normals, `V - E + F == 2`) are unchanged; only the field that
//! produces the sphere is.
//!
//! The same reasoning shapes the fuzz grids: their outermost shell is forced
//! below the level, so the level set never reaches the lattice boundary.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use atomcad_crystolecule::field::{
    FieldBounds, GridGeometry, IsosurfaceColoring, IsosurfaceData, SampledField, ScalarField,
};
use atomcad_display::isosurface::{ExtractionSettings, SurfaceMesh, extract_isosurface};
use glam::{DVec3, Vec3};

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

/// A field defined by a plain function, with no sample grid — the `None` branch
/// of every `native_grid`-shaped decision, and the consumer most likely to have
/// been wrongly written against a grid.
pub struct AnalyticField {
    name: &'static str,
    function: fn(DVec3) -> f64,
    bounds: FieldBounds,
    value_range: Option<(f64, f64)>,
}

impl std::fmt::Debug for AnalyticField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnalyticField({})", self.name)
    }
}

impl AnalyticField {
    pub fn new(
        name: &'static str,
        function: fn(DVec3) -> f64,
        half_extent: f64,
        value_range: Option<(f64, f64)>,
    ) -> Self {
        Self {
            name,
            function,
            bounds: FieldBounds::new(DVec3::splat(-half_extent), DVec3::splat(half_extent)),
            value_range,
        }
    }
}

impl ScalarField for AnalyticField {
    fn sample(&self, point: DVec3) -> f64 {
        (self.function)(point)
    }

    fn data_bounds(&self) -> Option<FieldBounds> {
        None
    }

    fn suggested_bounds(&self) -> FieldBounds {
        self.bounds
    }

    fn native_grid(&self) -> Option<GridGeometry> {
        None
    }

    fn value_range(&self) -> Option<(f64, f64)> {
        self.value_range
    }

    fn estimate_memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

/// Width of every decaying bump below. Chosen so a level of [`BUMP_LEVEL`] puts
/// the surface near radius 1 with plenty of room inside a half-extent-3 box.
pub const BUMP_SIGMA: f64 = 1.0;

/// The level the bump fixtures are extracted at.
pub const BUMP_LEVEL: f64 = 0.6;

/// Radius of `exp(-r^2 / 2 sigma^2) == level`, the exact answer the sphere
/// fixture's vertices must land on.
pub fn bump_radius(level: f64) -> f64 {
    (2.0 * BUMP_SIGMA * BUMP_SIGMA * (1.0 / level).ln()).sqrt()
}

fn gaussian(distance_squared: f64) -> f64 {
    (-distance_squared / (2.0 * BUMP_SIGMA * BUMP_SIGMA)).exp()
}

/// `exp(-r^2 / 2 sigma^2)` — one closed sphere per level, non-negative, so the
/// negative pass short-circuits.
pub fn sphere_bump() -> Arc<dyn ScalarField> {
    Arc::new(AnalyticField::new(
        "sphere_bump",
        |p| gaussian(p.length_squared()),
        3.0,
        Some((0.0, 1.0)),
    ))
}

/// A bump ridge around the circle `r = 1.5` in the xy plane: one component of
/// genus 1. The sphere's `V - E + F == 2` catches neither a handle welded shut
/// nor a handle invented; this does.
pub fn torus_bump() -> Arc<dyn ScalarField> {
    Arc::new(AnalyticField::new(
        "torus_bump",
        |p| {
            let radial = (p.x * p.x + p.y * p.y).sqrt() - 1.5;
            gaussian(radial * radial + p.z * p.z)
        },
        4.0,
        Some((0.0, 1.0)),
    ))
}

/// A bump ridge at radius 2: its level set is **two** concentric spheres, one
/// just inside and one just outside, with coincident centroids.
pub fn concentric_shells() -> Arc<dyn ScalarField> {
    Arc::new(AnalyticField::new(
        "concentric_shells",
        |p| {
            let radial = p.length() - 2.0;
            gaussian(radial * radial)
        },
        4.0,
        Some((0.0, 1.0)),
    ))
}

/// `z * exp(-alpha r^2)` — the signed fixture. Two lobes of opposite sign
/// either side of the `z = 0` nodal plane, so both sign passes have work to do.
pub fn p2z() -> Arc<dyn ScalarField> {
    Arc::new(AnalyticField::new(
        "p2z",
        |p| p.z * (-0.25 * p.length_squared()).exp(),
        6.0,
        None,
    ))
}

/// Two same-sign lobes a hair over one fallback cell apart — the surface-nets
/// failure mode, where one vertex per cell cannot represent two sheets and
/// welds them into a single component.
pub fn twin_bumps() -> Arc<dyn ScalarField> {
    Arc::new(AnalyticField::new(
        "twin_bumps",
        |p| {
            let a = p - DVec3::new(-0.62, 0.0, 0.0);
            let b = p - DVec3::new(0.62, 0.0, 0.0);
            (0.35 * gaussian(a.length_squared() * 9.0))
                .max(0.35 * gaussian(b.length_squared() * 9.0))
        },
        3.0,
        Some((0.0, 0.35)),
    ))
}

/// A linear ramp along x — the color field whose colormap output must vary
/// monotonically.
pub fn linear_ramp() -> Arc<dyn ScalarField> {
    Arc::new(AnalyticField::new("linear_ramp", |p| p.x, 5.0, None))
}

/// Sample an analytic function onto a (possibly sheared) grid, in the layout
/// `SampledField` documents: row-major with the last axis contiguous.
pub fn sample_onto_grid(grid: GridGeometry, function: impl Fn(DVec3) -> f64) -> SampledField {
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..grid.dims[0] {
        for j in 0..grid.dims[1] {
            for k in 0..grid.dims[2] {
                samples.push(function(grid.sample_position(i, j, k)) as f32);
            }
        }
    }
    SampledField::new(grid, samples).expect("valid sampled grid")
}

// ---------------------------------------------------------------------------
// Extraction shorthands
// ---------------------------------------------------------------------------

pub const POSITIVE_COLOR: Vec3 = Vec3::new(0.20, 0.40, 0.90);
pub const NEGATIVE_COLOR: Vec3 = Vec3::new(0.90, 0.30, 0.25);

pub fn phase_data(field: Arc<dyn ScalarField>, level: f64) -> IsosurfaceData {
    IsosurfaceData {
        field,
        level,
        coloring: IsosurfaceColoring::Phase {
            positive: POSITIVE_COLOR,
            negative: NEGATIVE_COLOR,
        },
        alpha: 1.0,
    }
}

/// Settings whose fallback spacing is fine enough for the analytic fixtures.
pub fn fine_settings() -> ExtractionSettings {
    ExtractionSettings {
        fallback_spacing: 0.12,
        ..ExtractionSettings::default()
    }
}

/// Defaults, for the fixtures that come with their own grid: a sampled field
/// ignores the fallback spacing entirely.
pub fn settings_for_snapshots() -> ExtractionSettings {
    ExtractionSettings::default()
}

pub fn extract(field: Arc<dyn ScalarField>, level: f64) -> SurfaceMesh {
    extract_isosurface(&phase_data(field, level), &fine_settings()).expect("within budget")
}

// ---------------------------------------------------------------------------
// Invariants
// ---------------------------------------------------------------------------

pub fn triangles(mesh: &SurfaceMesh) -> impl Iterator<Item = [u32; 3]> + '_ {
    mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]])
}

/// No boundary: every directed edge is matched by its reverse, with equal
/// multiplicity.
///
/// This is the property the renderer needs — `d(surface) = 0`, so every view
/// ray crosses an even number of times. It deliberately tolerates a
/// non-manifold *edge* (two sheets meeting where the tie rule merged a corner),
/// which is geometrically odd but still closed.
pub fn assert_closed_manifold(mesh: &SurfaceMesh, what: &str) {
    let mut directed: HashMap<(u32, u32), i32> = HashMap::new();
    for [a, b, c] in triangles(mesh) {
        for edge in [(a, b), (b, c), (c, a)] {
            *directed.entry(edge).or_insert(0) += 1;
        }
    }
    for (&(a, b), &count) in &directed {
        let reverse = directed.get(&(b, a)).copied().unwrap_or(0);
        assert_eq!(
            count, reverse,
            "{what}: edge {a}->{b} appears {count} times but {b}->{a} appears {reverse} \
             — the surface has a boundary"
        );
    }
}

/// Every vertex sits on `+level` or `-level` of the field.
pub fn assert_on_isosurface(mesh: &SurfaceMesh, field: &dyn ScalarField, level: f64, tol: f64) {
    for (i, position) in mesh.positions.iter().enumerate() {
        let value = field.sample(position.as_dvec3());
        let error = (value.abs() - level.abs()).abs();
        assert!(
            error <= tol,
            "vertex {i} at {position:?} samples {value} — off the +/-{level} isosurface by {error}"
        );
    }
}

/// Triangles are counter-clockwise seen from outside.
///
/// Checked globally by signed volume rather than per triangle: a closed
/// outward-wound surface encloses a positive volume, and the test is immune to
/// the sliver triangles marching cubes routinely emits (whose own face normal
/// is ill-conditioned, which is exactly why the extractor takes its normals
/// from the field gradient instead).
pub fn assert_outward_winding(mesh: &SurfaceMesh, what: &str) {
    let mut volume = 0.0f64;
    for [a, b, c] in triangles(mesh) {
        let pa = mesh.positions[a as usize].as_dvec3();
        let pb = mesh.positions[b as usize].as_dvec3();
        let pc = mesh.positions[c as usize].as_dvec3();
        volume += pa.cross(pb).dot(pc) / 6.0;
    }
    assert!(
        volume > 0.0,
        "{what}: enclosed volume is {volume}, so the triangles are wound inward"
    );
}

/// Components tile the index buffer exactly, in order, and share no vertices.
pub fn assert_components_partition_indices(mesh: &SurfaceMesh, what: &str) {
    let mut cursor = 0u32;
    let mut owner: HashMap<u32, usize> = HashMap::new();
    for (c, component) in mesh.components.iter().enumerate() {
        assert_eq!(
            component.first_index, cursor,
            "{what}: component {c} starts at {} but the previous one ended at {cursor}",
            component.first_index
        );
        assert!(
            component.index_count > 0 && component.index_count % 3 == 0,
            "{what}: component {c} spans {} indices, not a positive multiple of 3",
            component.index_count
        );
        let range = component.first_index as usize
            ..(component.first_index + component.index_count) as usize;
        for &vertex in &mesh.indices[range] {
            let previous = owner.insert(vertex, c);
            assert!(
                previous.is_none_or(|p| p == c),
                "{what}: vertex {vertex} is shared by components {previous:?} and {c}"
            );
        }
        cursor += component.index_count;
    }
    assert_eq!(
        cursor as usize,
        mesh.indices.len(),
        "{what}: components cover {cursor} of {} indices",
        mesh.indices.len()
    );
    assert_eq!(
        mesh.positions.len(),
        mesh.normals.len(),
        "{what}: normals do not match positions"
    );
    assert_eq!(
        mesh.positions.len(),
        mesh.albedo.len(),
        "{what}: albedo does not match positions"
    );
}

/// All four invariants, which is how every table below gets them for free.
pub fn assert_surface_invariants(
    mesh: &SurfaceMesh,
    field: &dyn ScalarField,
    level: f64,
    tol: f64,
    what: &str,
) {
    assert_closed_manifold(mesh, what);
    assert_on_isosurface(mesh, field, level, tol);
    assert_outward_winding(mesh, what);
    assert_components_partition_indices(mesh, what);
}

pub fn assert_finite(mesh: &SurfaceMesh, what: &str) {
    for (i, v) in mesh.positions.iter().enumerate() {
        assert!(v.is_finite(), "{what}: position {i} is {v:?}");
    }
    for (i, n) in mesh.normals.iter().enumerate() {
        assert!(n.is_finite(), "{what}: normal {i} is {n:?}");
        assert!(
            (n.length() - 1.0).abs() < 1e-3,
            "{what}: normal {i} has length {}",
            n.length()
        );
    }
    for (i, a) in mesh.albedo.iter().enumerate() {
        assert!(a.is_finite(), "{what}: albedo {i} is {a:?}");
    }
    for component in &mesh.components {
        assert!(
            component.centroid.is_finite(),
            "{what}: centroid {:?}",
            component.centroid
        );
    }
}

/// `V - E + F` over the whole mesh, summed across its components.
pub fn euler_characteristic(mesh: &SurfaceMesh) -> i64 {
    let mut vertices: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut edges: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut faces = 0i64;
    for [a, b, c] in triangles(mesh) {
        faces += 1;
        for v in [a, b, c] {
            vertices.insert(v);
        }
        for (x, y) in [(a, b), (b, c), (c, a)] {
            edges.insert((x.min(y), x.max(y)));
        }
    }
    vertices.len() as i64 - edges.len() as i64 + faces
}

/// Vertices of one component, as positions.
pub fn component_positions(mesh: &SurfaceMesh, component: usize) -> Vec<DVec3> {
    let c = mesh.components[component];
    let range = c.first_index as usize..(c.first_index + c.index_count) as usize;
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for &index in &mesh.indices[range] {
        if seen.insert(index) {
            out.push(mesh.positions[index as usize].as_dvec3());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Seeded randomness
// ---------------------------------------------------------------------------

/// xorshift64*, so the fuzz rows are replayable.
///
/// Seeds are a fixed list, never `rand::random()`: a fuzz failure that cannot
/// be replayed is a flake, and this suite has to be able to fail loudly in CI.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next_f64()
    }

    pub fn pick<T: Copy>(&mut self, options: &[T]) -> T {
        options[(self.next_u64() % options.len() as u64) as usize]
    }
}

/// A random grid whose **outermost shell is pinned to `0.0`**, so the level set
/// never reaches the lattice boundary.
///
/// Zero specifically, rather than "something small": at any level in `(0, 1)`
/// it is outside for the positive pass (`0 >= level` is false) *and* for the
/// negative pass (`0 <= -level` is false), so both passes close. Without the
/// shell the surface is legitimately open where it leaves the box — the
/// extractor marches exactly the lattice the design specifies and adds no
/// capping layer — and closure would be untestable on random data.
pub fn shelled_grid(rng: &mut Rng, dims: usize, body: impl Fn(&mut Rng) -> f64) -> SampledField {
    let grid = GridGeometry {
        origin: DVec3::new(-0.7, 0.3, -0.1),
        // Deliberately asymmetric: a cubic grid hides index transpositions.
        axes: [
            DVec3::new(0.31, 0.0, 0.0),
            DVec3::new(0.0, 0.27, 0.0),
            DVec3::new(0.0, 0.0, 0.35),
        ],
        dims: [dims, dims, dims],
    };
    let mut samples = Vec::with_capacity(grid.sample_count());
    for i in 0..dims {
        for j in 0..dims {
            for k in 0..dims {
                let on_shell =
                    i == 0 || j == 0 || k == 0 || i == dims - 1 || j == dims - 1 || k == dims - 1;
                samples.push(if on_shell { 0.0 } else { body(rng) as f32 });
            }
        }
    }
    SampledField::new(grid, samples).expect("valid sampled grid")
}

/// A grid whose every sample is the same value — the "all-equal at exactly the
/// level" degeneracy, where the non-strict tie rule classifies every corner as
/// inside and no edge is cut.
pub fn uniform_grid(dims: usize, value: f64) -> SampledField {
    let grid = GridGeometry {
        origin: DVec3::ZERO,
        axes: [
            DVec3::new(0.31, 0.0, 0.0),
            DVec3::new(0.0, 0.27, 0.0),
            DVec3::new(0.0, 0.0, 0.35),
        ],
        dims: [dims, dims, dims],
    };
    SampledField::new(grid, vec![value as f32; grid.sample_count()]).expect("valid sampled grid")
}
