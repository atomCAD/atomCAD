//! Per-atom baked ambient occlusion (`doc/design_depth_perception.md` D6, D7,
//! D10).
//!
//! For each receiver atom, the directions of a fixed 128-direction Fibonacci
//! sphere that are blocked by an occluder within [`R_MAX`] — the cone from
//! the atom's centre subtending the occluder's covalent sphere — form a
//! 128-bit mask. The open fraction is the exposure `E`, the mean open
//! direction (scaled by `E`) the bent vector `b`; the pair travels to the GPU
//! packed as one `Snorm8x4` `(4b, E)`.
//!
//! Every function here is pure and none reads a preference. The rules in it
//! are rendering policy, which is why it lives in the display crate and not
//! in crystolecule (D7, "Why `atomcad-display`"):
//!
//! - an occluder is any atom whose **structure** alpha is 1, culled or not,
//!   never judged by the preference-scaled display alpha;
//! - its radius is the covalent radius, whatever the display style;
//! - delete and unchanged markers are not matter and occlude nothing.
//!
//! The receiver set (which atoms are drawn opaque) is decided by
//! `atomic_tessellator::ao_receivers`, not here.

use atomcad_crystolecule::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use atomcad_crystolecule::atomic_structure::{Atom, AtomicStructure};
use atomcad_crystolecule::simulation::spatial_grid::SpatialGrid;
use glam::f32::Vec3;
use glam::f64::DVec3;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::f64::consts::FRAC_PI_2;
use std::sync::OnceLock;

/// The number of sky directions, one bit each of a `u128` mask (D6).
pub const DIRECTION_COUNT: usize = 128;

/// Occluders farther than this from a receiver's centre are ignored, Å (D6).
pub const R_MAX: f64 = 6.0;

/// Above this fraction of the receivers to (re)compute,
/// [`update_atom_exposure`] takes the full path (D7).
pub const FULL_PATH_FRACTION: f64 = 0.25;

/// The packed exposure of a surface nothing occludes: `b = 0`, `E = 1`.
pub use atomcad_renderer::atom_impostor_mesh::FULLY_EXPOSED;

/// The cap table's resolution (D7): cube-map cells per face edge, and
/// half-angle buckets over [0°, 90°].
const FACE_CELLS: usize = 16;
const CUBE_CELLS: usize = 6 * FACE_CELLS * FACE_CELLS;
const ANGLE_BUCKETS: usize = 32;

/// Two atoms closer than this (squared, Å²) are the same place: the
/// direction between them is undefined, so neither occludes the other.
const COINCIDENT_D2: f64 = 1e-12;

const FULL_MASK: u128 = u128::MAX;

// ============================================================================
// Occluders
// ============================================================================

/// The radius an atom occludes with: its covalent radius, for every display
/// style (D6; Open question 1 is decided in this one function).
pub fn occluder_radius(atom: &Atom) -> f64 {
    ATOM_INFO
        .get(&(atom.atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .covalent_radius
}

/// Whether an atom occludes: it is matter (not a delete or unchanged marker)
/// and its structure alpha is 1. Culled atoms occlude; ghosts (alpha < 1,
/// set by `xray`) do not. Never the display alpha, which a preference scales.
pub fn is_occluder(structure: &AtomicStructure, atom: &Atom) -> bool {
    !atom.is_delete_marker()
        && !atom.is_unchanged_marker()
        && structure.get_atom_alpha(atom.id) >= 1.0
}

/// The one neighbour predicate both computation paths share, so the two
/// grids cannot disagree on an occluder at the boundary (D7).
#[inline]
pub fn within_r_max(distance_squared: f64) -> bool {
    distance_squared <= R_MAX * R_MAX
}

// ============================================================================
// Directions and cap masks
// ============================================================================

const HALF_COUNT: usize = DIRECTION_COUNT / 2;

/// The fixed sky directions: [`DIRECTION_COUNT`] unit vectors, direction `k`
/// being bit `k` of a mask. The first half is a Fibonacci spiral over the
/// upper hemisphere; direction `k + 64` is the opposite of direction `k`.
///
/// A plain Fibonacci sphere's directions do not sum to zero (their mean is
/// about 4·10⁻⁴ off), which would give an isolated atom a bent vector. The
/// antipodal pairs make the set centrally symmetric, so the open sky of an
/// isolated or a fully buried atom sums to exactly zero, and any half-space
/// blocks exactly half the directions.
pub fn directions() -> &'static [DVec3; DIRECTION_COUNT] {
    static DIRECTIONS: OnceLock<[DVec3; DIRECTION_COUNT]> = OnceLock::new();
    DIRECTIONS.get_or_init(|| {
        let golden_angle = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        let upper = |k: usize| {
            let z = 1.0 - (2.0 * k as f64 + 1.0) / DIRECTION_COUNT as f64;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let phi = golden_angle * k as f64;
            DVec3::new(r * phi.cos(), r * phi.sin(), z)
        };
        std::array::from_fn(|k| {
            if k < HALF_COUNT {
                upper(k)
            } else {
                -upper(k - HALF_COUNT)
            }
        })
    })
}

/// The exact cone test, the table's oracle: the directions within
/// `half_angle` (radians) of the unit vector `axis`.
pub fn cap_mask_exact(axis: DVec3, half_angle: f64) -> u128 {
    let cos_half = half_angle.cos();
    let mut mask = 0u128;
    for (k, d) in directions().iter().enumerate() {
        if d.dot(axis) >= cos_half {
            mask |= 1u128 << k;
        }
    }
    mask
}

/// The cube-map cell of a unit vector: face, then row and column of a
/// `FACE_CELLS`² grid on it.
fn cube_cell(u: DVec3) -> usize {
    let a = u.abs();
    let (face, s, t) = if a.x >= a.y && a.x >= a.z {
        (if u.x > 0.0 { 0 } else { 1 }, u.y / a.x, u.z / a.x)
    } else if a.y >= a.z {
        (if u.y > 0.0 { 2 } else { 3 }, u.x / a.y, u.z / a.y)
    } else {
        (if u.z > 0.0 { 4 } else { 5 }, u.x / a.z, u.y / a.z)
    };
    let index = |c: f64| (((c + 1.0) * 0.5 * FACE_CELLS as f64) as usize).min(FACE_CELLS - 1);
    (face * FACE_CELLS + index(s)) * FACE_CELLS + index(t)
}

/// The unit vector through the centre of a cube-map cell.
fn cube_cell_centre(cell: usize) -> DVec3 {
    let face = cell / (FACE_CELLS * FACE_CELLS);
    let i = (cell / FACE_CELLS) % FACE_CELLS;
    let j = cell % FACE_CELLS;
    let coordinate = |n: usize| (n as f64 + 0.5) / FACE_CELLS as f64 * 2.0 - 1.0;
    let (s, t) = (coordinate(i), coordinate(j));
    let sign = if face.is_multiple_of(2) { 1.0 } else { -1.0 };
    match face / 2 {
        0 => DVec3::new(sign, s, t),
        1 => DVec3::new(s, sign, t),
        _ => DVec3::new(s, t, sign),
    }
    .normalize()
}

/// The half-angle bucket of an angle in [0, π/2]: the nearest of
/// `ANGLE_BUCKETS` values from 0 to 90° inclusive, so a fully blocking
/// occluder (90°) is represented exactly.
fn angle_bucket(half_angle: f64) -> usize {
    let step = FRAC_PI_2 / (ANGLE_BUCKETS - 1) as f64;
    ((half_angle / step).round().max(0.0) as usize).min(ANGLE_BUCKETS - 1)
}

/// The precomputed cap masks: one per cube-map cell and half-angle bucket,
/// built once (about 0.8 MB).
fn cap_table() -> &'static [u128] {
    static TABLE: OnceLock<Vec<u128>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let step = FRAC_PI_2 / (ANGLE_BUCKETS - 1) as f64;
        let mut table = Vec::with_capacity(CUBE_CELLS * ANGLE_BUCKETS);
        for cell in 0..CUBE_CELLS {
            let centre = cube_cell_centre(cell);
            for bucket in 0..ANGLE_BUCKETS {
                table.push(cap_mask_exact(centre, bucket as f64 * step));
            }
        }
        table
    })
}

/// The cap mask of an occluder in direction `axis` (unit) subtending
/// `half_angle` (radians, 0 to π/2), from the table (D7): the axis is
/// quantised to its cube-map cell's centre and the angle to its bucket.
#[inline]
pub fn cap_mask(axis: DVec3, half_angle: f64) -> u128 {
    cap_table()[cube_cell(axis) * ANGLE_BUCKETS + angle_bucket(half_angle)]
}

/// The bucket boundaries as `sin²` of the half-angle: bucket `b` and `b + 1`
/// meet at `sin²((b + ½)·step)`, so counting the boundaries at or below
/// `sin² θ` is `angle_bucket(θ)` without an `asin`.
fn sin2_bucket_boundaries() -> &'static [f64; ANGLE_BUCKETS - 1] {
    static BOUNDARIES: OnceLock<[f64; ANGLE_BUCKETS - 1]> = OnceLock::new();
    BOUNDARIES.get_or_init(|| {
        let step = FRAC_PI_2 / (ANGLE_BUCKETS - 1) as f64;
        std::array::from_fn(|b| ((b as f64 + 0.5) * step).sin().powi(2))
    })
}

/// Adds to `mask` what the occluder `(occluder, radius)` blocks as seen from
/// `centre`, if it is within [`R_MAX`]. The kernel both paths share.
///
/// It is [`cap_mask`] of the occluder's direction and half-angle, computed
/// without a square root or an `asin`: the cube-map cell does not depend on
/// the offset's length, and the half-angle bucket follows from
/// `sin² θ = r² / d²`.
#[inline]
fn add_occluder(mask: &mut u128, centre: DVec3, occluder: DVec3, radius: f64) {
    let offset = occluder - centre;
    let distance_squared = offset.length_squared();
    if !within_r_max(distance_squared) || distance_squared <= COINCIDENT_D2 {
        return;
    }
    let sin2 = (radius * radius / distance_squared).min(1.0);
    let bucket = sin2_bucket_boundaries().partition_point(|boundary| *boundary <= sin2);
    *mask |= cap_table()[cube_cell(offset) * ANGLE_BUCKETS + bucket];
}

/// The exposure `E` (open fraction) and bent vector `b` (mean open direction,
/// scaled by `E`) of a blocked-direction mask (D10). The sum runs over the
/// antipodal pairs in order, so it is deterministic, and a pair that is open
/// (or blocked) on both sides adds exactly nothing.
pub fn exposure_of_mask(mask: u128) -> (f64, DVec3) {
    let open = !mask;
    let directions = directions();
    let mut bent = DVec3::ZERO;
    for (k, d) in directions.iter().take(HALF_COUNT).enumerate() {
        let up = open & (1u128 << k) != 0;
        let down = open & (1u128 << (k + HALF_COUNT)) != 0;
        if up != down {
            bent += if up { *d } else { -*d };
        }
    }
    let n = DIRECTION_COUNT as f64;
    (open.count_ones() as f64 / n, bent / n)
}

/// `(E, b)` packed as the vertex delivers it, `Snorm8x4` `(4b, E)` (D10).
/// `|b| ≤ ¼`, so `4b` uses the full range; it is clamped all the same.
pub fn pack_exposure(exposure: f64, bent: DVec3) -> [i8; 4] {
    let snorm = |v: f64| (v.clamp(-1.0, 1.0) * 127.0).round() as i8;
    let b4 = bent * 4.0;
    [snorm(b4.x), snorm(b4.y), snorm(b4.z), snorm(exposure)]
}

/// The packed value decoded as the GPU decodes `Snorm8x4`: `(E, b)`.
pub fn unpack_exposure(packed: [i8; 4]) -> (f32, Vec3) {
    let unorm = |v: i8| (v as f32 / 127.0).max(-1.0);
    (
        unorm(packed[3]),
        Vec3::new(unorm(packed[0]), unorm(packed[1]), unorm(packed[2])) * 0.25,
    )
}

fn pack_mask(mask: u128) -> [i8; 4] {
    let (exposure, bent) = exposure_of_mask(mask);
    pack_exposure(exposure, bent)
}

// ============================================================================
// Exposure values
// ============================================================================

/// The packed `(4b, E)` of each receiver, keyed by atom id. An absent id is
/// fully exposed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtomExposure(FxHashMap<u32, [i8; 4]>);

impl AtomExposure {
    /// The packed value of `atom_id`, if it has one.
    pub fn get(&self, atom_id: u32) -> Option<[i8; 4]> {
        self.0.get(&atom_id).copied()
    }

    /// The packed value of `atom_id`, [`FULLY_EXPOSED`] when it has none.
    pub fn packed_or_exposed(&self, atom_id: u32) -> [i8; 4] {
        self.get(atom_id).unwrap_or(FULLY_EXPOSED)
    }

    pub fn contains(&self, atom_id: u32) -> bool {
        self.0.contains_key(&atom_id)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, [i8; 4])> + '_ {
        self.0.iter().map(|(id, packed)| (*id, *packed))
    }

    /// Bytes held, for a memory estimate.
    pub fn memory_usage_bytes(&self) -> usize {
        self.0.capacity() * (std::mem::size_of::<u32>() + std::mem::size_of::<[i8; 4]>())
    }
}

/// What one call did, returned per call and never kept in a global (Testing
/// layer 1): how many receivers it computed, and whether it took the full
/// path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExposureStats {
    pub receivers_computed: usize,
    pub full_path: bool,
}

/// The full path: one `SpatialGrid` (cell `R_MAX`) over the occluders,
/// receivers in parallel. Receivers absent from the structure get no value.
fn compute_with_spatial_grid(
    structure: &AtomicStructure,
    receivers: &[u32],
) -> Vec<(u32, [i8; 4])> {
    // Atoms in a flat array, found by id through a dense table (ids are
    // small and nearly contiguous; a hash map here cost more than the grid).
    let count = structure.get_num_of_atoms();
    let mut positions = Vec::with_capacity(count * 3);
    let mut radii = Vec::with_capacity(count);
    let mut index_of = Vec::new();
    let mut occluders = Vec::with_capacity(count);
    for (index, atom) in structure.atoms_values().enumerate() {
        positions.extend_from_slice(&[atom.position.x, atom.position.y, atom.position.z]);
        radii.push(occluder_radius(atom));
        let id = atom.id as usize;
        if index_of.len() <= id {
            index_of.resize(id + 1, usize::MAX);
        }
        index_of[id] = index;
        if is_occluder(structure, atom) {
            occluders.push(index);
        }
    }
    let grid = SpatialGrid::from_positions_subset(&positions, &occluders, R_MAX);
    let position =
        |i: usize| DVec3::new(positions[3 * i], positions[3 * i + 1], positions[3 * i + 2]);
    // The grid's own cut is strict; widen it a hair and let `within_r_max`
    // decide, as the other path does.
    let search = R_MAX * (1.0 + 1e-9) + 1e-9;
    receivers
        .par_iter()
        .filter_map(|id| {
            let i = *index_of.get(*id as usize).filter(|i| **i != usize::MAX)?;
            let centre = position(i);
            let mut mask = 0u128;
            grid.for_each_neighbor(&positions, i, search, |j| {
                if mask != FULL_MASK {
                    add_occluder(&mut mask, centre, position(j), radii[j]);
                }
            });
            Some((*id, pack_mask(mask)))
        })
        .collect()
}

/// The incremental path: each receiver through `AtomicStructure`'s own grid.
fn compute_with_structure_grid(
    structure: &AtomicStructure,
    receivers: &[u32],
) -> Vec<(u32, [i8; 4])> {
    let search = R_MAX * (1.0 + 1e-9) + 1e-9;
    receivers
        .par_iter()
        .filter_map(|id| {
            let atom = structure.get_atom(*id)?;
            let centre = atom.position;
            let mut mask = 0u128;
            for other_id in structure.get_atoms_in_radius(&centre, search) {
                if mask == FULL_MASK {
                    break;
                }
                if other_id == *id {
                    continue;
                }
                if let Some(other) = structure.get_atom(other_id)
                    && is_occluder(structure, other)
                {
                    add_occluder(&mut mask, centre, other.position, occluder_radius(other));
                }
            }
            Some((*id, pack_mask(mask)))
        })
        .collect()
}

/// The packed `(4b, E)` of each atom in `receivers`, keyed by atom id;
/// absent ids are fully exposed. Builds one `SpatialGrid` over the
/// occluders and runs the receivers under `rayon`.
pub fn atom_exposure(
    structure: &AtomicStructure,
    receivers: &[u32],
) -> (AtomExposure, ExposureStats) {
    let values = compute_with_spatial_grid(structure, receivers);
    let stats = ExposureStats {
        receivers_computed: values.len(),
        full_path: true,
    };
    (AtomExposure(values.into_iter().collect()), stats)
}

// ============================================================================
// Incremental updates
// ============================================================================

/// The occluder state of one atom: what its neighbours' exposure depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OccluderState {
    position: DVec3,
    radius: f64,
    occluder: bool,
}

fn occluder_state(structure: &AtomicStructure, atom: &Atom) -> OccluderState {
    OccluderState {
        position: atom.position,
        radius: occluder_radius(atom),
        occluder: is_occluder(structure, atom),
    }
}

/// The occluder state of every atom of a structure: id → (position,
/// occluder radius, occluder or not). The diff runs against it, so the cache
/// never needs the old structure alive (D9).
#[derive(Clone, Debug, Default)]
pub struct OccluderSnapshot(FxHashMap<u32, OccluderState>);

impl OccluderSnapshot {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Bytes held, for a memory estimate.
    pub fn memory_usage_bytes(&self) -> usize {
        self.0.capacity() * (std::mem::size_of::<u32>() + std::mem::size_of::<OccluderState>())
    }
}

/// The occluder state of every atom of `structure`. O(atoms).
pub fn occluder_snapshot(structure: &AtomicStructure) -> OccluderSnapshot {
    OccluderSnapshot(
        structure
            .atoms_values()
            .map(|atom| (atom.id, occluder_state(structure, atom)))
            .collect(),
    )
}

/// A place whose occluder state changed: every receiver within `reach` of
/// it may have a stale value.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ChangedPlace {
    position: DVec3,
    reach: f64,
}

/// The places where occluders changed between two states of a structure:
/// the old and the new position of every atom that moved, changed radius,
/// started or stopped occluding, appeared or disappeared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcclusionChanges(Vec<ChangedPlace>);

impl OcclusionChanges {
    /// The number of changed places (an atom that moved counts twice).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn push(&mut self, state: &OccluderState) {
        // A non-occluder affects nobody where it is.
        if state.occluder {
            self.0.push(ChangedPlace {
                position: state.position,
                reach: R_MAX + state.radius,
            });
        }
    }
}

/// The atoms whose occluder state differs between `old` and `new`
/// (position, occluder radius, occluder or not, added, removed), with their
/// old and new positions. O(atoms).
pub fn occlusion_changes(old: &OccluderSnapshot, new: &AtomicStructure) -> OcclusionChanges {
    let mut changes = OcclusionChanges::default();
    for atom in new.atoms_values() {
        let state = occluder_state(new, atom);
        match old.0.get(&atom.id) {
            Some(previous) if *previous == state => {}
            Some(previous) => {
                changes.push(previous);
                changes.push(&state);
            }
            None => changes.push(&state),
        }
    }
    for (id, previous) in &old.0 {
        if new.get_atom(*id).is_none() {
            changes.push(previous);
        }
    }
    changes
}

/// The bits an atom's place is matched by: what its neighbours' exposure,
/// and its own, depend on.
type PlaceKey = (u64, u64, u64, u64, bool);

fn place_key(state: &OccluderState) -> PlaceKey {
    (
        state.position.x.to_bits(),
        state.position.y.to_bits(),
        state.position.z.to_bits(),
        state.radius.to_bits(),
        state.occluder,
    )
}

/// How the atoms of a snapshot are numbered in a newer structure, as
/// [`occlusion_changes_by_place`] matched them: the ids that changed, and
/// the ids whose atom has no counterpart. Every other id is unchanged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtomRenumbering {
    renumbered: FxHashMap<u32, u32>,
    lost: FxHashSet<u32>,
}

impl AtomRenumbering {
    /// Whether no matched atom changed its id.
    pub fn keeps_ids(&self) -> bool {
        self.renumbered.is_empty()
    }

    /// The number of matched atoms whose id changed.
    pub fn renumbered_count(&self) -> usize {
        self.renumbered.len()
    }

    /// The new id of old atom `id`, `None` when it has no counterpart.
    pub fn new_id(&self, id: u32) -> Option<u32> {
        if self.lost.contains(&id) {
            None
        } else {
            Some(self.renumbered.get(&id).copied().unwrap_or(id))
        }
    }
}

/// [`occlusion_changes`] matched by **place** rather than by atom id, plus
/// how the atoms of `old` are numbered in `new`.
///
/// An atom of `new` whose position, occluder radius and occluder flag equal,
/// bit for bit, those of an atom of `old` is the same occluder whatever its
/// id, and a receiver there keeps its value. This matters because a producer
/// may renumber atoms that did not move: `atom_edit`'s `apply_diff` does on
/// every change of its diff, and an id-keyed diff would then report most of
/// the structure as changed. Atoms that kept their id and their state are
/// matched first, at the cost of the id diff; only the rest are matched by
/// place. Atoms at one place are matched one to one, and a surplus on either
/// side is a change there.
///
/// The changes are complete from `old` to `new`, so the result replaces,
/// rather than extends, a pending set diffed earlier from the same `old`.
pub fn occlusion_changes_by_place(
    old: &OccluderSnapshot,
    new: &AtomicStructure,
) -> (OcclusionChanges, AtomRenumbering) {
    let same_as_old = |id: u32, state: &OccluderState| old.0.get(&id) == Some(state);

    // The atoms of `new` that did not keep both their id and their state.
    let unmatched_new: Vec<(u32, OccluderState)> = new
        .atoms_values()
        .map(|atom| (atom.id, occluder_state(new, atom)))
        .filter(|(id, state)| !same_as_old(*id, state))
        .collect();

    // The atoms of `old` likewise, by place; a second atom at one place goes
    // to `more_at`, which is empty but for coincident atoms.
    let mut at: FxHashMap<PlaceKey, u32> = FxHashMap::default();
    let mut more_at: FxHashMap<PlaceKey, Vec<u32>> = FxHashMap::default();
    for (id, state) in &old.0 {
        let kept = new
            .get_atom(*id)
            .is_some_and(|atom| occluder_state(new, atom) == *state);
        if kept {
            continue;
        }
        let key = place_key(state);
        match at.entry(key) {
            std::collections::hash_map::Entry::Occupied(_) => {
                more_at.entry(key).or_default().push(*id);
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(*id);
            }
        }
    }

    let mut changes = OcclusionChanges::default();
    let mut renumbering = AtomRenumbering::default();
    for (new_id, state) in &unmatched_new {
        let key = place_key(state);
        let old_id = match more_at.get_mut(&key).and_then(Vec::pop) {
            Some(id) => Some(id),
            None => at.remove(&key),
        };
        match old_id {
            Some(old_id) => {
                if old_id != *new_id {
                    renumbering.renumbered.insert(old_id, *new_id);
                }
            }
            None => changes.push(state),
        }
    }
    for id in at.values().chain(more_at.values().flatten()) {
        changes.push(&old.0[id]);
        renumbering.lost.insert(*id);
    }
    (changes, renumbering)
}

impl AtomExposure {
    /// The values re-keyed to the ids of the structure `renumbering` was
    /// taken against; an atom without a counterpart loses its value.
    pub fn remapped(&self, renumbering: &AtomRenumbering) -> AtomExposure {
        AtomExposure(
            self.0
                .iter()
                .filter_map(|(id, packed)| renumbering.new_id(*id).map(|new_id| (new_id, *packed)))
                .collect(),
        )
    }
}

/// The union of two change sets, keeping every old and new position of
/// either. Folds a new diff into changes still pending.
pub fn merge_changes(pending: OcclusionChanges, new: OcclusionChanges) -> OcclusionChanges {
    let mut merged = pending;
    merged.0.extend(new.0);
    merged
}

/// `previous` brought up to date for `structure`, holding values for
/// `receivers` only. Drops the values within `R_MAX + occluder radius` of a
/// changed position, old or new, and the values of atoms that are no longer
/// receivers, then computes every receiver that has no value. When at most
/// [`FULL_PATH_FRACTION`] of the receivers need computing, they go through
/// `AtomicStructure`'s own grid; above it, through [`atom_exposure`]'s path.
///
/// Both paths run the same kernel over the same neighbour set, so the result
/// is bit-identical to `atom_exposure(structure, receivers)`.
pub fn update_atom_exposure(
    previous: &AtomExposure,
    changes: &OcclusionChanges,
    structure: &AtomicStructure,
    receivers: &[u32],
) -> (AtomExposure, ExposureStats) {
    plan_exposure_update(previous, changes, structure, receivers).run(structure)
}

/// An [`update_atom_exposure`] split in two: what it keeps and what it would
/// compute, decided without computing anything. The scene's cache asks
/// [`ExposureUpdatePlan::takes_full_path`] during a drag, where a full-path
/// update is deferred to the drag's end (D9).
pub struct ExposureUpdatePlan {
    kept: FxHashMap<u32, [i8; 4]>,
    missing: Vec<u32>,
    full_path: bool,
}

impl ExposureUpdatePlan {
    /// Whether running the plan takes the full path: more than
    /// [`FULL_PATH_FRACTION`] of the receivers need computing.
    pub fn takes_full_path(&self) -> bool {
        self.full_path
    }

    /// The number of receivers running the plan computes.
    pub fn receivers_to_compute(&self) -> usize {
        self.missing.len()
    }

    /// The updated values, as [`update_atom_exposure`] returns them.
    pub fn run(self, structure: &AtomicStructure) -> (AtomExposure, ExposureStats) {
        let computed = if self.full_path {
            compute_with_spatial_grid(structure, &self.missing)
        } else {
            compute_with_structure_grid(structure, &self.missing)
        };
        let stats = ExposureStats {
            receivers_computed: computed.len(),
            full_path: self.full_path,
        };
        let mut values = self.kept;
        values.extend(computed);
        (AtomExposure(values), stats)
    }
}

/// The first half of [`update_atom_exposure`]: which values survive the
/// changes and which receivers must be computed.
pub fn plan_exposure_update(
    previous: &AtomExposure,
    changes: &OcclusionChanges,
    structure: &AtomicStructure,
    receivers: &[u32],
) -> ExposureUpdatePlan {
    let limit = receivers.len() as f64 * FULL_PATH_FRACTION;

    // The receivers a change may have touched. More changed places than the
    // full-path limit means the update is going to be a full one anyway, so
    // the per-place queries are skipped and every value is dropped.
    let drop_all = changes.len() as f64 > limit;
    let mut stale = FxHashSet::default();
    if !drop_all {
        for place in &changes.0 {
            stale.extend(structure.get_atoms_in_radius(&place.position, place.reach));
        }
    }

    let mut values = FxHashMap::default();
    let mut missing = Vec::new();
    for id in receivers {
        match previous.get(*id) {
            Some(packed) if !drop_all && !stale.contains(id) => {
                values.insert(*id, packed);
            }
            _ => missing.push(*id),
        }
    }

    let full_path = missing.len() as f64 > limit;
    ExposureUpdatePlan {
        kept: values,
        missing,
        full_path,
    }
}
