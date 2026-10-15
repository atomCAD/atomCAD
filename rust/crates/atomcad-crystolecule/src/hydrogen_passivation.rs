use crate::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO, halogen_bond_length};
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure::atom::{
    HYBRIDIZATION_AUTO, HYBRIDIZATION_SP1, HYBRIDIZATION_SP2, HYBRIDIZATION_SP3,
};
use crate::atomic_structure::inline_bond::BOND_SINGLE;
use crate::guided_placement::{
    Hybridization, TETRAHEDRAL_ANGLE, TRIGONAL_ANGLE, compute_sp3_case1_with_dihedral,
    count_active_neighbors, covalent_max_neighbors, detect_hybridization, find_dihedral_reference,
    gather_bond_directions, sp2_case2, sp3_case2, sp3_case3,
};
use glam::f64::DVec3;
use std::collections::HashMap;

// ============================================================================
// Options and result types
// ============================================================================

pub struct RemoveHydrogensOptions {
    /// Only remove H atoms that are themselves selected or bonded to a selected atom.
    pub selected_only: bool,
}

pub struct RemoveHydrogensResult {
    /// Number of hydrogen atoms removed.
    pub hydrogens_removed: usize,
}

/// Remove hydrogen atoms from the structure.
///
/// Two-phase approach: analyze immutably (collect IDs to remove), then mutate (delete atoms).
/// When `selected_only` is true, only hydrogen atoms that are themselves selected or bonded
/// to a selected atom are removed.
pub fn remove_hydrogens(
    structure: &mut AtomicStructure,
    options: &RemoveHydrogensOptions,
) -> RemoveHydrogensResult {
    remove_hydrogens_filtered(structure, options, &|_| true)
}

/// Like [`remove_hydrogens`], but an H atom is only removed when its **host**
/// (the heavy atom it is bonded to) satisfies `eligible`. The default
/// unfiltered variant passes an always-`true` predicate, so the two share a
/// single code path.
///
/// Region semantics (`doc/design_blueprint_region_atom_edits.md` §A4): the
/// membership decision uses the position of the existing heavy atom, not the
/// H's own position — an H sitting just outside the region boundary is still
/// stripped if its host is in-region. A lone H with no host falls back to its
/// own membership.
pub fn remove_hydrogens_filtered(
    structure: &mut AtomicStructure,
    options: &RemoveHydrogensOptions,
    eligible: &dyn Fn(u32) -> bool,
) -> RemoveHydrogensResult {
    // Phase 1: Analysis (immutable scan)
    let atom_ids: Vec<u32> = structure.atom_ids().copied().collect();
    let mut ids_to_remove: Vec<u32> = Vec::new();

    for &atom_id in &atom_ids {
        let atom = match structure.get_atom(atom_id) {
            Some(a) => a,
            None => continue,
        };

        if atom.atomic_number != 1 {
            continue;
        }

        // Region gate: remove this H only if its host heavy atom is in-region.
        // Lone H (no bonds) falls back to its own membership.
        let mut hosts = atom
            .bonds
            .iter()
            .filter(|b| !b.is_delete_marker())
            .map(|b| b.other_atom_id())
            .peekable();
        let host_in_region = if hosts.peek().is_none() {
            eligible(atom_id)
        } else {
            hosts.any(eligible)
        };
        if !host_in_region {
            continue;
        }

        if options.selected_only {
            let is_self_selected = atom.is_selected();
            let has_selected_neighbor =
                atom.bonds
                    .iter()
                    .filter(|b| !b.is_delete_marker())
                    .any(|b| {
                        structure
                            .get_atom(b.other_atom_id())
                            .is_some_and(|n| n.is_selected())
                    });
            if !is_self_selected && !has_selected_neighbor {
                continue;
            }
        }

        ids_to_remove.push(atom_id);
    }

    // Phase 2: Mutation (remove atoms)
    for &atom_id in &ids_to_remove {
        structure.delete_atom(atom_id);
    }

    RemoveHydrogensResult {
        hydrogens_removed: ids_to_remove.len(),
    }
}

pub struct AddHydrogensOptions {
    /// Only passivate atoms that are currently selected.
    pub selected_only: bool,
    /// Skip atoms already flagged as hydrogen-passivated.
    /// Default: true.
    pub skip_already_passivated: bool,
    /// The terminating element placed at each open valence slot. Hydrogen (1)
    /// by default; may be a halogen (9/17/35/53). See
    /// `doc/design_halogen_passivation.md` D1/D2. Callers are responsible for
    /// validating the value against `is_allowed_passivant`; this module treats
    /// any non-hydrogen number as "use `halogen_bond_length`".
    pub passivant_element: i16,
    /// The steric clash rule (`doc/design_chemisorb_mirrors_and_passivation.md`
    /// §5.1, D7): with `Some(f)`, a terminator is skipped — its host's valence
    /// left open — when it would sit closer than `f × (r_vdW(terminator) +
    /// r_vdW(other))` to another atom. `None` (the default) places every
    /// terminator, exactly as before the option existed.
    pub clash: Option<f64>,
}

impl Default for AddHydrogensOptions {
    fn default() -> Self {
        Self {
            selected_only: false,
            skip_already_passivated: true,
            passivant_element: 1,
            clash: None,
        }
    }
}

/// The default factor of the clash rule ([`AddHydrogensOptions::clash`]): with
/// the element table's radii, H–H 1.68 Å, H–C 2.08 Å, H–Si 2.37 Å. Checked
/// against the surface fixtures in §8.1 of
/// `doc/design_chemisorb_mirrors_and_passivation.md`.
pub const DEFAULT_PASSIVATION_CLASH_FACTOR: f64 = 0.7;

pub struct AddHydrogensResult {
    /// Number of terminating atoms added (hydrogens or halogens).
    pub atoms_added: usize,
    /// Number of terminators the clash rule skipped (always 0 with
    /// `clash: None`).
    pub skipped: usize,
}

// ============================================================================
// X-H bond length table
// ============================================================================

/// Hardcoded table of common X-H bond lengths in Angstroms.
/// Rounded experimental values from Calculla bond length tables,
/// Wikipedia, and NIST CCCBDB.
const XH_BOND_LENGTHS: &[(i16, f64)] = &[
    (6, 1.09),  // C-H
    (7, 1.01),  // N-H
    (8, 0.96),  // O-H
    (14, 1.48), // Si-H
    (15, 1.42), // P-H
    (16, 1.34), // S-H
    (5, 1.19),  // B-H
    (32, 1.53), // Ge-H
];

/// Look up the X-H bond length for a given element.
/// Falls back to covalent_radius(X) + covalent_radius(H) if not in the table.
fn lookup_xh_bond_length(atomic_number: i16) -> f64 {
    for &(z, length) in XH_BOND_LENGTHS {
        if z == atomic_number {
            return length;
        }
    }
    // Fallback: sum of covalent radii
    let r_x = ATOM_INFO
        .get(&(atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .covalent_radius;
    let r_h = ATOM_INFO
        .get(&1)
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .covalent_radius;
    r_x + r_h
}

/// Equilibrium host–terminator bond length in Angstroms, for **any** allowed
/// passivant.
///
/// This is the branch `add_hydrogens_filtered` has always taken inline: the
/// per-context `XH_BOND_LENGTHS` table for hydrogen, [`halogen_bond_length`]
/// otherwise (`doc/design_halogen_passivation.md` D2 — H must never reach that
/// function). It is public because `proxy_cut` places link-atom caps on
/// severed bonds and needs exactly the same length the general passivation
/// path would use; see `doc/design_proxy_node.md` §7.5. Adding a caller here
/// is deliberate — adding a fourth *table* would not be.
pub fn terminator_bond_length(host: i16, passivant: i16) -> f64 {
    if passivant == 1 {
        lookup_xh_bond_length(host)
    } else {
        halogen_bond_length(host, passivant)
    }
}

// ============================================================================
// Open valences
// ============================================================================

/// The unsaturated valence slots of one atom — the number of terminators
/// [`add_hydrogens_filtered`] would place on it, and the figure
/// `proxy_cut::ProxyStats::open_valences` sums (`doc/design_proxy_node.md`
/// §7.5). One function, so the multiplicity a panel shows and the number of
/// hydrogens `passivate` would add cannot disagree.
///
/// Zero for an atom that cannot host a terminator at all: a missing atom, a
/// marker or unresolved parameter (`Z <= 0`), a hydrogen, an atom of the
/// passivant's own element (the host-skip rule), and any saturated atom. The
/// *options* of the passivation path — `selected_only`,
/// `skip_already_passivated`, the region predicate — are caller-side filters
/// and play no part here.
pub fn open_valence_slots(
    structure: &AtomicStructure,
    atom_id: u32,
    passivant_element: i16,
) -> usize {
    open_valence_analysis(structure, atom_id, passivant_element).map_or(0, |(_, needed)| needed)
}

/// [`open_valence_slots`] together with the hybridization it was derived from,
/// which the placement path needs for the open bond directions. `None` is "no
/// slots", so the two callers cannot disagree about the count.
fn open_valence_analysis(
    structure: &AtomicStructure,
    atom_id: u32,
    passivant_element: i16,
) -> Option<(Hybridization, usize)> {
    let atom = structure.get_atom(atom_id)?;

    // Use effective atomic number to resolve parameter elements
    let atomic_number = structure.effective_atomic_number(atom);

    // Skip atoms that should not be passivated
    if atomic_number <= 0 {
        return None; // delete markers, unresolved parameters
    }
    // Host-skip rule (doc/design_halogen_passivation.md, "Host-skip rule"):
    // never cap an atom with its own element. `host == 1` keeps the
    // longstanding "H is a terminator, never a host" invariant for every
    // passivant; `host == passivant` prevents self-capping (e.g. no F₂).
    // For H passivation this is byte-identical to the old `== 1` check.
    if atomic_number == 1 || atomic_number == passivant_element {
        return None;
    }

    // Read the atom's hybridization override from flags (set by atom_edit evaluation).
    // Convert from u8 flag value to Option<Hybridization> for detect_hybridization.
    let hyb_override = match atom.hybridization_override() {
        HYBRIDIZATION_SP3 => Some(Hybridization::Sp3),
        HYBRIDIZATION_SP2 => Some(Hybridization::Sp2),
        HYBRIDIZATION_SP1 => Some(Hybridization::Sp1),
        HYBRIDIZATION_AUTO | _ => None,
    };
    let hybridization = detect_hybridization(structure, atom_id, hyb_override);
    let max_bonds = covalent_max_neighbors(atomic_number, hybridization);
    let current = count_active_neighbors(structure, atom_id);
    if current >= max_bonds {
        return None;
    }
    Some((hybridization, max_bonds - current))
}

// ============================================================================
// Geometry: compute_open_directions
// ============================================================================

/// Pick a deterministic perpendicular vector to `v`.
fn arbitrary_perpendicular(v: DVec3) -> DVec3 {
    let ref_axis = if v.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    v.cross(ref_axis).normalize()
}

/// Compute open bond directions for placing hydrogen atoms.
///
/// Given the hybridization, existing bond directions, and the number of open slots,
/// returns unit direction vectors pointing from the atom toward where hydrogens
/// should be placed.
fn compute_open_directions(
    structure: &AtomicStructure,
    atom_id: u32,
    hybridization: Hybridization,
    existing_dirs: &[DVec3],
    needed: usize,
) -> Vec<DVec3> {
    match hybridization {
        Hybridization::Sp3 => {
            compute_open_directions_sp3(structure, atom_id, existing_dirs, needed)
        }
        Hybridization::Sp2 => compute_open_directions_sp2(existing_dirs, needed),
        Hybridization::Sp1 => compute_open_directions_sp1(existing_dirs, needed),
    }
}

fn compute_open_directions_sp3(
    structure: &AtomicStructure,
    atom_id: u32,
    existing_dirs: &[DVec3],
    needed: usize,
) -> Vec<DVec3> {
    let num_existing = existing_dirs.len();
    match num_existing {
        0 => {
            // Standard tetrahedral orientation
            let dirs = [
                DVec3::new(1.0, 1.0, 1.0).normalize(),
                DVec3::new(-1.0, -1.0, 1.0).normalize(),
                DVec3::new(-1.0, 1.0, -1.0).normalize(),
                DVec3::new(1.0, -1.0, -1.0).normalize(),
            ];
            dirs[..needed.min(4)].to_vec()
        }
        1 => {
            // Try dihedral reference first
            let bond_dir = existing_dirs[0];
            let neighbor_id = structure.get_atom(atom_id).and_then(|atom| {
                atom.bonds
                    .iter()
                    .find(|b| !b.is_delete_marker())
                    .map(|b| b.other_atom_id())
            });

            if let Some(neighbor_id) = neighbor_id
                && let Some(ref_perp) = find_dihedral_reference(structure, atom_id, neighbor_id)
            {
                // Use staggered (Primary) positions only
                let dots = compute_sp3_case1_with_dihedral(DVec3::ZERO, bond_dir, ref_perp, 1.0);
                return dots
                    .iter()
                    .filter(|d| d.dot_type == crate::guided_placement::GuideDotType::Primary)
                    .take(needed)
                    .map(|d| d.position.normalize())
                    .collect();
            }

            // Fallback: cone at 70.53° from -bond_dir with arbitrary reference
            let cone_axis = -bond_dir;
            let cone_half_angle = std::f64::consts::PI - TETRAHEDRAL_ANGLE;
            let cos_cone = cone_half_angle.cos();
            let sin_cone = cone_half_angle.sin();

            let u = arbitrary_perpendicular(bond_dir);
            let v = bond_dir.cross(u).normalize();

            let angles = [0.0_f64, 120.0, 240.0];
            angles[..needed.min(3)]
                .iter()
                .map(|&angle_deg| {
                    let angle = angle_deg.to_radians();
                    let (sin_a, cos_a) = angle.sin_cos();
                    let radial = u * cos_a + v * sin_a;
                    (cone_axis * cos_cone + radial * sin_cone).normalize()
                })
                .collect()
        }
        2 => {
            // Reuse sp3_case2 logic
            let dots = sp3_case2(DVec3::ZERO, existing_dirs, 1.0);
            dots.into_iter()
                .take(needed)
                .map(|d| d.position.normalize())
                .collect()
        }
        3 => {
            // Reuse sp3_case3 logic
            let dots = sp3_case3(DVec3::ZERO, existing_dirs, 1.0);
            dots.into_iter()
                .take(needed)
                .map(|d| d.position.normalize())
                .collect()
        }
        _ => vec![],
    }
}

fn compute_open_directions_sp2(existing_dirs: &[DVec3], needed: usize) -> Vec<DVec3> {
    let num_existing = existing_dirs.len();
    match num_existing {
        0 => {
            // Equilateral triangle in XY plane
            let dirs = [
                DVec3::X,
                DVec3::new(-0.5, 3.0_f64.sqrt() / 2.0, 0.0).normalize(),
                DVec3::new(-0.5, -(3.0_f64.sqrt()) / 2.0, 0.0).normalize(),
            ];
            dirs[..needed.min(3)].to_vec()
        }
        1 => {
            // Two directions at ±120° from existing bond in an arbitrary plane
            let bond_dir = existing_dirs[0];
            let perp = arbitrary_perpendicular(bond_dir);

            let cos_120 = TRIGONAL_ANGLE.cos();
            let sin_120 = TRIGONAL_ANGLE.sin();

            let d1 = (bond_dir * cos_120 + perp * sin_120).normalize();
            let d2 = (bond_dir * cos_120 - perp * sin_120).normalize();

            [d1, d2][..needed.min(2)].to_vec()
        }
        2 => {
            // Reuse sp2_case2 logic
            let dots = sp2_case2(DVec3::ZERO, existing_dirs, 1.0);
            dots.into_iter()
                .take(needed)
                .map(|d| d.position.normalize())
                .collect()
        }
        _ => vec![],
    }
}

fn compute_open_directions_sp1(existing_dirs: &[DVec3], needed: usize) -> Vec<DVec3> {
    let num_existing = existing_dirs.len();
    match num_existing {
        0 => {
            // Arbitrary axis
            let dirs = [DVec3::X, DVec3::NEG_X];
            dirs[..needed.min(2)].to_vec()
        }
        1 => {
            // Directly opposite
            vec![-existing_dirs[0]]
        }
        _ => vec![],
    }
}

// ============================================================================
// Main algorithm
// ============================================================================

/// Add hydrogen atoms to satisfy valence requirements of all undersaturated atoms.
///
/// Two-step approach: analyze immutably (collect placements), then mutate (add atoms/bonds).
pub fn add_hydrogens(
    structure: &mut AtomicStructure,
    options: &AddHydrogensOptions,
) -> AddHydrogensResult {
    add_hydrogens_filtered(structure, options, &|_| true)
}

/// Like [`add_hydrogens`], but only atoms satisfying `eligible` are passivated.
/// The default unfiltered variant passes an always-`true` predicate, so the two
/// share a single code path.
///
/// Region semantics (`doc/design_blueprint_region_atom_edits.md` §A4): `eligible`
/// tests the **host** (dangling-bond) atom. A passivating H is placed wherever
/// the bond template puts it, regardless of where that lands relative to the
/// region — newly created H atoms are never themselves membership-tested.
pub fn add_hydrogens_filtered(
    structure: &mut AtomicStructure,
    options: &AddHydrogensOptions,
    eligible: &dyn Fn(u32) -> bool,
) -> AddHydrogensResult {
    // Step 1: Analysis (immutable scan)
    let mut atom_ids: Vec<u32> = structure.atom_ids().copied().collect();
    let mut placements: Vec<(u32, Vec<DVec3>)> = Vec::new();
    let mut clash_rule = options
        .clash
        .map(|factor| ClashRule::new(factor, options.passivant_element));
    let mut skipped = 0;
    if clash_rule.is_some() {
        // The rule is greedy, so the visiting order decides which terminator
        // of a clashing pair survives: hosts in atom-id order (D7).
        atom_ids.sort_unstable();
    }

    for &atom_id in &atom_ids {
        // Region gate: only passivate in-region host atoms.
        if !eligible(atom_id) {
            continue;
        }

        let atom = match structure.get_atom(atom_id) {
            Some(a) => a,
            None => continue,
        };

        if options.selected_only && !atom.is_selected() {
            continue;
        }
        if options.skip_already_passivated && atom.is_hydrogen_passivation() {
            continue;
        }

        // The element checks, the hybridization and the slot count are
        // `open_valence_analysis`: the one place they live, shared with
        // `open_valence_slots` (and so with the proxy report).
        let Some((hybridization, needed)) =
            open_valence_analysis(structure, atom_id, options.passivant_element)
        else {
            continue;
        };
        let atomic_number = structure.effective_atomic_number(atom);
        let position = atom.position;

        let existing_dirs = gather_bond_directions(structure, atom);
        // Bond length: H keeps the per-context molecular table (unchanged);
        // halogens use the shared molecular primitive (D2).
        let bond_len = terminator_bond_length(atomic_number, options.passivant_element);
        let open_dirs =
            compute_open_directions(structure, atom_id, hybridization, &existing_dirs, needed);
        let mut positions: Vec<DVec3> =
            open_dirs.iter().map(|d| position + *d * bond_len).collect();
        if let Some(rule) = clash_rule.as_mut() {
            let before = positions.len();
            positions.retain(|&p| rule.accept(structure, atom_id, p));
            skipped += before - positions.len();
        }

        if !positions.is_empty() {
            placements.push((atom_id, positions));
        }
    }

    // Step 2: Mutation (add atoms and bonds)
    let mut atoms_added = 0;
    for (parent_id, positions) in placements {
        for pos in positions {
            let term_id = structure.add_atom(options.passivant_element, pos);
            // General path flags its terminators (D5); the flag is inert on a
            // saturated halogen terminator, same as on H.
            structure.set_atom_hydrogen_passivation(term_id, true);
            structure.add_bond(parent_id, term_id, BOND_SINGLE);
            atoms_added += 1;
        }
    }

    AddHydrogensResult {
        atoms_added,
        skipped,
    }
}

/// Places exactly `count` terminators of `element` on each listed host: the
/// **first** `count` of the slots [`add_hydrogens_filtered`] would fill there,
/// in the order its direction code returns them (fewer if the host has fewer
/// open slots). No option of the general path applies: no clash rule, no
/// selection, and a host's hydrogen-passivation flag is ignored. Returns the
/// added atoms' ids, host by host.
///
/// `chemisorb`'s `passivate_sites` builds its reference state with it
/// (`doc/design_chemisorb_mirrors_and_passivation.md` §4.4): the substrate
/// alone carrying as many H on each host as a candidate got, where a used site
/// has more open slots than it had in the candidate.
pub fn add_terminators_counted(
    structure: &mut AtomicStructure,
    hosts: &[(u32, usize)],
    element: i16,
) -> Vec<u32> {
    let mut placements: Vec<(u32, Vec<DVec3>)> = Vec::new();
    for &(host, count) in hosts {
        if count == 0 {
            continue;
        }
        let Some((hybridization, needed)) = open_valence_analysis(structure, host, element) else {
            continue;
        };
        let atom = structure.get_atom(host).expect("analysed above");
        let bond_len = terminator_bond_length(structure.effective_atomic_number(atom), element);
        let existing_dirs = gather_bond_directions(structure, atom);
        let dirs = compute_open_directions(structure, host, hybridization, &existing_dirs, needed);
        let positions = dirs
            .iter()
            .take(count)
            .map(|d| atom.position + *d * bond_len)
            .collect();
        placements.push((host, positions));
    }
    let mut added = Vec::new();
    for (host, positions) in placements {
        for pos in positions {
            let id = structure.add_atom(element, pos);
            structure.set_atom_hydrogen_passivation(id, true);
            structure.add_bond(host, id, BOND_SINGLE);
            added.push(id);
        }
    }
    added
}

// ============================================================================
// The clash rule
// ============================================================================

/// The steric test of [`AddHydrogensOptions::clash`], run in the analysis step
/// against the structure plus the terminators this pass already accepted.
///
/// A terminator on host `h` is never tested against `h`, against `h`'s bonded
/// neighbours (its 1–3 partners: on carbon an H sits ~2.15 Å from the next C,
/// inside any useful fraction of the van der Waals sum), or against the other
/// terminators of `h` (also 1–3 partners: the two H of an ideal NH₂ or OH₂ are
/// 1.57–1.65 Å apart, inside 0.7 of the H–H sum).
struct ClashRule {
    factor: f64,
    passivant_radius: f64,
    /// The farthest a structure atom can be and still clash: `factor ×
    /// (r_passivant + the largest radius in the table)`.
    reach: f64,
    /// The accepted terminators, `(position, host)`, bucketed by cubes of side
    /// `cell` (at least the terminator–terminator reach), so the pass stays
    /// linear.
    accepted: HashMap<(i64, i64, i64), Vec<(DVec3, u32)>>,
    cell: f64,
}

impl ClashRule {
    fn new(factor: f64, passivant: i16) -> Self {
        let factor = factor.max(0.0);
        let passivant_radius = van_der_waals_radius(passivant);
        let largest = ATOM_INFO
            .values()
            .map(|info| info.van_der_waals_radius)
            .fold(DEFAULT_ATOM_INFO.van_der_waals_radius, f64::max);
        Self {
            factor,
            passivant_radius,
            reach: factor * (passivant_radius + largest),
            accepted: HashMap::new(),
            cell: (factor * 2.0 * passivant_radius).max(1.0),
        }
    }

    fn cell_of(&self, p: DVec3) -> (i64, i64, i64) {
        let c = (p / self.cell).floor();
        (c.x as i64, c.y as i64, c.z as i64)
    }

    /// Whether a terminator at `p` on `host` passes the rule. An accepted one
    /// is recorded, so later candidates are tested against it.
    fn accept(&mut self, structure: &AtomicStructure, host: u32, p: DVec3) -> bool {
        if self.clashes(structure, host, p) {
            return false;
        }
        let cell = self.cell_of(p);
        self.accepted.entry(cell).or_default().push((p, host));
        true
    }

    fn clashes(&self, structure: &AtomicStructure, host: u32, p: DVec3) -> bool {
        let Some(host_atom) = structure.get_atom(host) else {
            return false;
        };
        let excluded = |id: u32| {
            id == host
                || host_atom
                    .bonds
                    .iter()
                    .any(|b| !b.is_delete_marker() && b.other_atom_id() == id)
        };
        for id in structure.get_atoms_in_radius(&p, self.reach) {
            if excluded(id) {
                continue;
            }
            let Some(other) = structure.get_atom(id) else {
                continue;
            };
            let z = structure.effective_atomic_number(other);
            if z <= 0 {
                continue; // delete markers, unresolved parameters
            }
            let limit = self.factor * (self.passivant_radius + van_der_waals_radius(z));
            if p.distance_squared(other.position) < limit * limit {
                return true;
            }
        }
        let limit = self.factor * 2.0 * self.passivant_radius;
        let (cx, cy, cz) = self.cell_of(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(bucket) = self.accepted.get(&(cx + dx, cy + dy, cz + dz)) else {
                        continue;
                    };
                    if bucket.iter().any(|&(q, other_host)| {
                        other_host != host && p.distance_squared(q) < limit * limit
                    }) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn van_der_waals_radius(atomic_number: i16) -> f64 {
    ATOM_INFO
        .get(&(atomic_number as i32))
        .unwrap_or(&DEFAULT_ATOM_INFO)
        .van_der_waals_radius
}
