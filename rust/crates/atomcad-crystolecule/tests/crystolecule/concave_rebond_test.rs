//! Concave-corner rebonding — `doc/design_concave_rebonding.md`.
//!
//! At a concave corner (a (100) terrace meeting an ascending (111) wall) the
//! dimer search leaves a row of surface atoms unpaired, passivation gives them
//! two terminators each, and one of those terminators lands ~1.5 A from a
//! terminator on the wall — a steric clash that should be resolved by dropping
//! both and bonding the two hosts directly (design §1).
//!
//! The clash detector here (`unresolved_clashes`) is a deliberate
//! re-implementation of the design's §5 criterion, independent of the
//! production code, so these tests check the *structure* rather than agreeing
//! with the implementation by construction.
//!
//! Status: `concave_corner_*` and `halogen_*` are RED until the rebonding pass
//! lands. The `surf_recon_off_*` / `flat_*` / `bevelled_*` controls are green
//! now and must stay green.

use atomcad_crystolecule::atomic_constants::{ALLOWED_PASSIVANTS, ATOM_INFO};
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use atomcad_crystolecule::lattice_fill::{
    LatticeFillConfig, LatticeFillOptions, LatticeFillResult, fill_lattice,
};
use atomcad_crystolecule::motif::Motif;
use atomcad_crystolecule::unit_cell_struct::UnitCellStruct;
use atomcad_geo_tree::GeoNode;
use atomcad_util::daabox::DAABox;
use glam::f64::DVec3;
use std::collections::HashMap;

// =============================================================================
// Fixture geometry
// =============================================================================

/// Silicon diamond-cubic lattice parameter (A). Must match
/// `SILICON_UNIT_CELL_SIZE_ANGSTROM` in `surface_reconstruction.rs` to within
/// 0.05 A or the reconstruction gate rejects the cell and nothing happens.
const SI_A: f64 = 5.431;

/// Slab extent (A): a 6x6x3 cell block.
const SLAB_L: f64 = 6.0 * SI_A;
const SLAB_H: f64 = 3.0 * SI_A;
/// Height of the lower (100) terrace — a whole number of cells, so the terrace
/// lands exactly on a lattice layer.
const Z_LOW: f64 = 2.0 * SI_A;

/// (111) cut offset for the concave-corner fixture.
///
/// Derived with `sweep_cut_offsets` (below, `#[ignore]`d): the offset only
/// matters modulo the lattice, and the sweep shows plateaus one cell wide. The
/// plateau spanning 36.97..41.04 is the registry that leaves a whole row of
/// terrace atoms unpaired against the ascending wall — 9 clashes, the largest
/// crop in the sweep. 39.0 is its midpoint, so small changes elsewhere cannot
/// slide the fixture off the edge of the plateau.
const SUM_CUT: f64 = 39.0;

/// `half_space(n, c)` is `{ p : dot(p - c, n) <= 0 }` — the normal points
/// *outward* from the solid (matches `axis_aligned_box` in `lattice_fill_test.rs`).
fn axis_aligned_box(min: DVec3, max: DVec3) -> GeoNode {
    GeoNode::intersection_3d(vec![
        GeoNode::half_space(DVec3::new(-1.0, 0.0, 0.0), DVec3::new(min.x, 0.0, 0.0)),
        GeoNode::half_space(DVec3::new(1.0, 0.0, 0.0), DVec3::new(max.x, 0.0, 0.0)),
        GeoNode::half_space(DVec3::new(0.0, -1.0, 0.0), DVec3::new(0.0, min.y, 0.0)),
        GeoNode::half_space(DVec3::new(0.0, 1.0, 0.0), DVec3::new(0.0, max.y, 0.0)),
        GeoNode::half_space(DVec3::new(0.0, 0.0, -1.0), DVec3::new(0.0, 0.0, min.z)),
        GeoNode::half_space(DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, max.z)),
    ])
}

fn cubic_cell(a: f64) -> UnitCellStruct {
    UnitCellStruct::new(
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, a, 0.0),
        DVec3::new(0.0, 0.0, a),
    )
}

/// A genuine silicon zincblende motif — Si baked into the PARAM defaults,
/// exactly as the user's `structure.14Si` custom node does. The reconstruction
/// gate keys on the *effective* element values, so this is what makes
/// `get_reconstruction_params` hand back the silicon parameters.
fn silicon_motif() -> Motif {
    let mut motif = DEFAULT_ZINCBLENDE_MOTIF.clone();
    for p in &mut motif.parameters {
        p.default_atomic_number = 14;
    }
    motif
}

/// A (100) terrace meeting an ascending (111) wall — Lukas's minimal case.
///
/// The slab spans `[0, SLAB_L]^2 x [0, SLAB_H]`. Above `Z_LOW` the solid is cut
/// back to `x + y + z > sum_cut`, whose boundary is a {111} plane inclined
/// 54.7 deg from horizontal. Below `Z_LOW` everything survives. Walking along
/// the terrace toward increasing `x + y` you reach the plane and material rises
/// above you — a concave corner whose step edge runs along [1-10].
///
/// What the resulting terrace looks like (established by the exploration dumps
/// that produced `SUM_CUT`): the last row before the wall picks up a third
/// lattice bond *into* the wall base, so `classify_atom_surface_orientation`
/// sees three bonds and files it as `Unknown`. Its would-be dimer partner — the
/// next row back — is then left with no valid partner and stays dihydride,
/// which is exactly the "only one row of the dimer row left" Lukas described.
fn concave_corner_geometry(sum_cut: f64) -> GeoNode {
    let slab = axis_aligned_box(DVec3::ZERO, DVec3::new(SLAB_L, SLAB_L, SLAB_H));
    let third = sum_cut / 3.0;
    let removed = GeoNode::intersection_3d(vec![
        // { z >= Z_LOW }
        GeoNode::half_space(DVec3::new(0.0, 0.0, -1.0), DVec3::new(0.0, 0.0, Z_LOW)),
        // { x + y + z <= sum_cut }
        GeoNode::half_space(
            DVec3::new(1.0, 1.0, 1.0).normalize(),
            DVec3::new(third, third, third),
        ),
    ]);
    GeoNode::difference_3d(Box::new(slab), Box::new(removed))
}

/// A plain slab — six {100} faces, every corner convex. Control fixture.
fn flat_slab_geometry() -> GeoNode {
    axis_aligned_box(DVec3::ZERO, DVec3::new(SLAB_L, SLAB_L, SLAB_H))
}

/// A slab with one corner sliced off by a {111} plane. Exposed (111) atoms
/// carry a single dangling bond along the surface normal, so their terminators
/// are parallel rather than facing — the geometry §5 test 5 exists to reject.
fn bevelled_slab_geometry() -> GeoNode {
    let cut = 1.6 * SI_A;
    GeoNode::intersection_3d(vec![
        flat_slab_geometry(),
        GeoNode::half_space(
            DVec3::new(-1.0, -1.0, -1.0).normalize(),
            DVec3::new(cut, cut, cut),
        ),
    ])
}

/// Materializes `geometry` with the silicon motif under the given options.
fn materialize(geometry: GeoNode, options: &LatticeFillOptions) -> LatticeFillResult {
    let config = LatticeFillConfig {
        unit_cell: cubic_cell(SI_A),
        motif: silicon_motif(),
        parameter_element_values: HashMap::new(),
        geometry,
        motif_offset: DVec3::ZERO,
        regions: Vec::new(),
    };
    let margin = 6.0;
    let fill_region = DAABox::new(
        DVec3::splat(-margin),
        DVec3::new(SLAB_L + margin, SLAB_L + margin, SLAB_H + margin),
    );
    fill_lattice(&config, options, &fill_region)
}

fn options(reconstruct: bool, passivant: i16) -> LatticeFillOptions {
    LatticeFillOptions {
        hydrogen_passivation: true,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms: false,
        reconstruct_surface: reconstruct,
        invert_phase: false,
        passivation_element: passivant,
    }
}

// =============================================================================
// Independent clash detector (the design's §5 criterion, re-derived)
// =============================================================================

/// §5 test 3: clash threshold as a fraction of the van der Waals radius sum.
/// Validated on this fixture: real clashes sit at 0.634 of the H-H vdW sum and
/// the nearest legitimate contact at 1.195, so 0.75 has ~1.2x headroom below
/// and ~1.6x margin above.
const CLASH_FRACTION: f64 = 0.75;
/// §5 test 4: hosts must be within ~one second-neighbour separation (`a/sqrt(2)`).
const HOST_SEPARATION_FACTOR: f64 = 1.05;
/// §5 test 5: each dangling bond must point within 60 deg of the other host.
const FACING_COS: f64 = 0.5;

fn vdw(z: i16) -> f64 {
    ATOM_INFO
        .get(&(z as i32))
        .map(|i| i.van_der_waals_radius)
        .unwrap_or(1.2)
}

/// Every atom that looks like a passivation terminator, paired with its host.
///
/// A terminator has exactly one bond and a monovalent-passivant element. This
/// is the test-side approximation of design §6 — the production pass
/// discriminates exactly, by absence from the `PlacedAtomTracker`. The fixtures
/// here use a pure silicon motif, so no lattice atom can be mistaken for a
/// terminator and the two definitions coincide.
fn terminators(structure: &AtomicStructure) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = structure
        .iter_atoms()
        .filter(|(_, a)| a.bonds.len() == 1 && ALLOWED_PASSIVANTS.contains(&a.atomic_number))
        .map(|(id, a)| (*id, a.bonds[0].other_atom_id()))
        .collect();
    out.sort_unstable();
    out
}

/// Terminator pairs satisfying §5 tests 1, 3, 4 and 5 — clashes the rebonding
/// pass is expected to have resolved. Test 2 (unpaired surface host) is
/// deliberately omitted: it is an implementation gate, not an observable
/// property of the finished structure.
fn unresolved_clashes(structure: &AtomicStructure) -> Vec<(u32, u32)> {
    let max_host_separation = HOST_SEPARATION_FACTOR * SI_A / 2.0_f64.sqrt();
    let terms = terminators(structure);
    let mut found = Vec::new();

    for (i, &(t_a, host_a)) in terms.iter().enumerate() {
        for &(t_b, host_b) in terms.iter().skip(i + 1) {
            // Test 1: distinct hosts, not already bonded.
            if host_a == host_b || structure.has_bond_between(host_a, host_b) {
                continue;
            }
            let pa = structure.get_atom(t_a).unwrap().position;
            let pb = structure.get_atom(t_b).unwrap().position;
            let ha = structure.get_atom(host_a).unwrap().position;
            let hb = structure.get_atom(host_b).unwrap().position;
            // Test 3: steric clash between the terminators.
            let limit = CLASH_FRACTION
                * (vdw(structure.get_atom(t_a).unwrap().atomic_number)
                    + vdw(structure.get_atom(t_b).unwrap().atomic_number));
            if (pa - pb).length() > limit {
                continue;
            }
            // Test 4: hosts close enough to bond.
            if (ha - hb).length() > max_host_separation {
                continue;
            }
            // Test 5: the two dangling bonds face each other.
            let to_b = (hb - ha).normalize();
            if (pa - ha).normalize().dot(to_b) < FACING_COS
                || (pb - hb).normalize().dot(-to_b) < FACING_COS
            {
                continue;
            }
            found.push((t_a, t_b));
        }
    }
    found
}

/// Si-Si bonds joining two atoms of the *same* (100) layer. Lattice bonds
/// always run between adjacent layers, so every such bond was added by
/// reconstruction: a dimer (pulled in to ~2.34 A) or a concave rebond (left at
/// the unrelaxed second-neighbour separation, ~3.84 A — design D6).
fn same_layer_si_bonds(structure: &AtomicStructure) -> Vec<(u32, u32, f64)> {
    let mut out = Vec::new();
    for (id, atom) in structure.iter_atoms() {
        if atom.atomic_number != 14 {
            continue;
        }
        for b in &atom.bonds {
            let other_id = b.other_atom_id();
            if other_id <= *id {
                continue; // count each bond once
            }
            let other = structure.get_atom(other_id).unwrap();
            if other.atomic_number != 14 {
                continue;
            }
            if (other.position.z - atom.position.z).abs() < 0.6 {
                out.push((*id, other_id, (other.position - atom.position).length()));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    out
}

/// Same-layer bonds long enough to be concave rebonds rather than dimers.
fn rebond_count(structure: &AtomicStructure) -> usize {
    same_layer_si_bonds(structure)
        .iter()
        .filter(|(_, _, d)| *d > 3.0)
        .count()
}

/// Silicon atoms exceeding their valence — design §8 says the rewrite is
/// coordination-neutral, so this must stay empty.
fn overcoordinated(structure: &AtomicStructure) -> Vec<u32> {
    let mut out: Vec<u32> = structure
        .iter_atoms()
        .filter(|(_, a)| a.atomic_number == 14 && a.bonds.len() > 4)
        .map(|(id, _)| *id)
        .collect();
    out.sort_unstable();
    out
}

// =============================================================================
// RED — the bug
// =============================================================================

/// The headline regression. A concave (100)/(111) corner must not leave any
/// terminator pair clashing: the rebonding pass should have dropped both and
/// bonded the hosts.
///
/// Before the fix this finds 9 clashes — 7 between the unpaired terrace row and
/// the wall-base row (the reported bug), and 2 more between adjacent unpaired
/// terrace atoms near the corner tip.
#[test]
fn concave_corner_leaves_no_terminator_clash() {
    let result = materialize(concave_corner_geometry(SUM_CUT), &options(true, 1));
    let clashes = unresolved_clashes(&result.atomic_structure);
    assert!(
        clashes.is_empty(),
        "{} clashing terminator pairs survived at the concave corner: {:?}",
        clashes.len(),
        clashes
    );
}

/// The structural counterpart: each resolved clash must leave behind a
/// host-host bond. Nine clashes, nine new same-layer bonds at the unrelaxed
/// second-neighbour separation.
#[test]
fn concave_corner_creates_one_rebond_per_clash() {
    let result = materialize(concave_corner_geometry(SUM_CUT), &options(true, 1));
    assert_eq!(
        rebond_count(&result.atomic_structure),
        9,
        "expected one ~3.84 A same-layer Si-Si bond per resolved clash; \
         same-layer bonds found: {:?}",
        same_layer_si_bonds(&result.atomic_structure)
    );
}

/// Design §8: dropping a terminator and adding a host-host bond is
/// coordination-neutral, so no silicon may end up with more than four bonds.
/// Guards the failure mode where the pass adds the bond without removing the
/// terminator, or lets one terminator be consumed twice.
#[test]
fn concave_corner_rebonding_is_coordination_neutral() {
    let result = materialize(concave_corner_geometry(SUM_CUT), &options(true, 1));
    let bad = overcoordinated(&result.atomic_structure);
    assert!(bad.is_empty(), "over-coordinated silicon atoms: {bad:?}");
}

/// Design D2: the criterion is a fraction of the van der Waals radius sum, so
/// it must scale across the whole passivant set without retuning. A chlorine
/// terminator clashes far harder than hydrogen (0.15 of the vdW sum vs 0.63),
/// and the same corner must resolve identically for every allowed element.
#[test]
fn halogen_passivant_clash_also_resolved() {
    for passivant in ALLOWED_PASSIVANTS {
        let result = materialize(concave_corner_geometry(SUM_CUT), &options(true, passivant));
        let clashes = unresolved_clashes(&result.atomic_structure);
        assert!(
            clashes.is_empty(),
            "{} clashing pairs survived at the concave corner with passivant {}: {:?}",
            clashes.len(),
            passivant,
            clashes
        );
    }
}

// =============================================================================
// GREEN — controls that must not regress
// =============================================================================

/// Design §7, the structural gate. With reconstruction off, the whole (100)
/// surface is dihydride and terminators clash everywhere — but the user asked
/// for that, and the pass must not fire. No same-layer Si-Si bond may exist:
/// the lattice has none, so any such bond would mean the pass silently
/// dimerized a surface the user chose to leave unreconstructed.
#[test]
fn surf_recon_off_leaves_the_surface_untouched() {
    let result = materialize(concave_corner_geometry(SUM_CUT), &options(false, 1));
    let bonds = same_layer_si_bonds(&result.atomic_structure);
    assert!(
        bonds.is_empty(),
        "reconstruction is off, so no same-layer Si-Si bond may exist, found {}: {:?}",
        bonds.len(),
        bonds
    );
}

/// A plain slab has six {100} faces and only convex edges, so reconstruction
/// leaves nothing clashing. Pins the "the pass has no work to do on ordinary
/// geometry" baseline — if this ever goes red, the criterion has widened.
///
/// Run for **every** allowed passivant, not just hydrogen. That is not
/// belt-and-braces: with a bulky halogen the terminator reaches 0.6 A further
/// out while the threshold grows with its van der Waals radius, so §5 test 3
/// alone starts flagging ~98 perfectly ordinary contacts on this slab (see
/// `dump_control_contact_spectrum_by_passivant`). Tests 4 and 5 are what reject
/// them, and this test is what proves it.
#[test]
fn flat_slab_has_no_clashes_for_any_passivant() {
    for passivant in ALLOWED_PASSIVANTS {
        let result = materialize(flat_slab_geometry(), &options(true, passivant));
        let clashes = unresolved_clashes(&result.atomic_structure);
        assert!(
            clashes.is_empty(),
            "a plain slab passivated with element {} should present no terminator \
             clashes, found {}: {:?}",
            passivant,
            clashes.len(),
            clashes
        );
    }
}

/// Design §5 test 5. On a (111) facet every dangling bond points along the
/// surface normal, so neighbouring terminators are parallel rather than facing
/// and can never be rebonded however close they sit — which is exactly what has
/// to hold for the halogens, whose terminators do come within test 3's range.
#[test]
fn bevelled_111_facet_has_no_clashes_for_any_passivant() {
    for passivant in ALLOWED_PASSIVANTS {
        let result = materialize(bevelled_slab_geometry(), &options(true, passivant));
        let clashes = unresolved_clashes(&result.atomic_structure);
        assert!(
            clashes.is_empty(),
            "a (111) facet passivated with element {} should present no terminator \
             clashes, found {}: {:?}",
            passivant,
            clashes.len(),
            clashes
        );
    }
}

// =============================================================================
// Exploration (ignored) — how SUM_CUT was chosen
// =============================================================================

/// Sweeps the (111) cut offset and reports, per offset, the atom count, the
/// number of dihydride sites, the number of unresolved clashes and any
/// over-coordination. Kept so `SUM_CUT` can be re-derived if the reconstruction
/// or passivation geometry ever changes (design §10).
///
/// Run with:
///   cargo test -p atomcad-crystolecule --test crystolecule \
///       concave_rebond_test::sweep_cut_offsets -- --ignored --nocapture
#[test]
#[ignore]
fn sweep_cut_offsets() {
    println!(
        "{:>10} {:>8} {:>10} {:>10} {:>8}",
        "sum_cut", "atoms", "dihydride", "clashes", "over"
    );
    for step in 0..32 {
        let sum_cut = 20.0 + step as f64 * (SI_A / 8.0);
        let result = materialize(concave_corner_geometry(sum_cut), &options(true, 1));
        let s = &result.atomic_structure;
        let mut counts: HashMap<u32, usize> = HashMap::new();
        for (_, host) in terminators(s) {
            *counts.entry(host).or_default() += 1;
        }
        println!(
            "{:>10.3} {:>8} {:>10} {:>10} {:>8}",
            sum_cut,
            s.get_num_of_atoms(),
            counts.values().filter(|n| **n >= 2).count(),
            unresolved_clashes(s).len(),
            overcoordinated(s).len(),
        );
    }
}

/// Contact spectrum under a chosen passivant, on a fixture that should present
/// no clashes at all. Reports which of §5's tests rejects each near contact --
/// with a bulky halogen the distance test alone may no longer suffice, and it
/// matters whether tests 4 and 5 are actually carrying the load.
/// Exploration only.
#[test]
#[ignore]
fn dump_control_contact_spectrum_by_passivant() {
    let max_host_separation = HOST_SEPARATION_FACTOR * SI_A / 2.0_f64.sqrt();
    for (name, passivant) in [("H", 1i16), ("F", 9), ("Cl", 17), ("Br", 35), ("I", 53)] {
        for (fixture, geo) in [
            ("flat slab", flat_slab_geometry()),
            ("(111) bevel", bevelled_slab_geometry()),
        ] {
            let result = materialize(geo, &options(true, passivant));
            let s = &result.atomic_structure;
            let limit = CLASH_FRACTION * 2.0 * vdw(passivant);
            let terms = terminators(s);
            let mut worst: Option<(f64, f64, f64)> = None; // (dist, host_sep, facing)
            let mut fired3 = 0;
            for (i, &(t_a, host_a)) in terms.iter().enumerate() {
                for &(t_b, host_b) in terms.iter().skip(i + 1) {
                    if host_a == host_b || s.has_bond_between(host_a, host_b) {
                        continue;
                    }
                    let pa = s.get_atom(t_a).unwrap().position;
                    let pb = s.get_atom(t_b).unwrap().position;
                    let ha = s.get_atom(host_a).unwrap().position;
                    let hb = s.get_atom(host_b).unwrap().position;
                    let d = (pa - pb).length();
                    if d > limit {
                        continue;
                    }
                    fired3 += 1;
                    let hs = (ha - hb).length();
                    let to_b = (hb - ha).normalize();
                    let facing = (pa - ha)
                        .normalize()
                        .dot(to_b)
                        .min((pb - hb).normalize().dot(-to_b));
                    if worst.is_none() || d < worst.unwrap().0 {
                        worst = Some((d, hs, facing));
                    }
                }
            }
            let clashes = unresolved_clashes(s).len();
            match worst {
                None => println!(
                    "{name:>3} {fixture:>12}: threshold {limit:>5.2} A -- no contact even trips test 3; clashes={clashes}"
                ),
                Some((d, hs, facing)) => println!(
                    "{name:>3} {fixture:>12}: threshold {limit:>5.2} A -- test 3 fires on {fired3:>3} pair(s), \
closest {d:.3} A; host sep {hs:.3} (cap {max_host_separation:.3}) {}, facing {facing:.3} (min 0.5) {}; clashes={clashes}",
                    if hs > max_host_separation {
                        "REJECTS"
                    } else {
                        "passes"
                    },
                    if facing < FACING_COS {
                        "REJECTS"
                    } else {
                        "passes"
                    },
                ),
            }
        }
    }
}
