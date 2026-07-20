//! Tests for the per-region materialization settings engine in `lattice_fill`
//! (Part B / Phase B1 of `doc/design_blueprint_region_atom_edits.md`).
//!
//! These are pure `lattice_fill` tests: `LatticeFillConfig` is constructed
//! directly (no node network), so they exercise `RegionSpec` / `SettingsResolver`
//! and the per-position gates wired into cleanup, reconstruction, and passivation.

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::{
    DEFAULT_ZINCBLENDE_MOTIF, DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
};
use rust_lib_flutter_cad::crystolecule::lattice_fill::{
    DEFAULT_REGION_MARGIN, LatticeFillConfig, LatticeFillOptions, RegionSpec, fill_lattice,
};
use rust_lib_flutter_cad::crystolecule::motif::Motif;
use rust_lib_flutter_cad::crystolecule::motif_parser::parse_motif;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::util::daabox::DAABox;
use std::collections::HashMap;

const A: f64 = DIAMOND_UNIT_CELL_SIZE_ANGSTROM; // 3.567 Å, axis-aligned cubic diamond

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Axis-aligned box from `min` to `max` as an intersection of 6 half-spaces,
/// in absolute real (Å) coordinates (same frame the region SDFs use).
fn axis_box(min: DVec3, max: DVec3) -> GeoNode {
    GeoNode::intersection_3d(vec![
        GeoNode::half_space(DVec3::new(-1.0, 0.0, 0.0), min),
        GeoNode::half_space(DVec3::new(1.0, 0.0, 0.0), max),
        GeoNode::half_space(DVec3::new(0.0, -1.0, 0.0), min),
        GeoNode::half_space(DVec3::new(0.0, 1.0, 0.0), max),
        GeoNode::half_space(DVec3::new(0.0, 0.0, -1.0), min),
        GeoNode::half_space(DVec3::new(0.0, 0.0, 1.0), max),
    ])
}

/// All-five-settings options builder (matches `LatticeFillOptions` field order).
fn opts(
    passivate: bool,
    rm_unbonded: bool,
    rm_single: bool,
    surf_recon: bool,
    invert: bool,
) -> LatticeFillOptions {
    LatticeFillOptions {
        hydrogen_passivation: passivate,
        remove_unbonded_atoms: rm_unbonded,
        remove_single_bond_atoms: rm_single,
        reconstruct_surface: surf_recon,
        invert_phase: invert,
        passivation_element: 1,
    }
}

/// A region with the given volume, default margin, and all settings unset (inherit).
fn region(geometry: GeoNode) -> RegionSpec {
    RegionSpec {
        geometry,
        margin: DEFAULT_REGION_MARGIN,
        passivate: None,
        rm_single: None,
        surf_recon: None,
        invert_phase: None,
        rm_unbonded: None,
        passiv_elem: None,
    }
}

/// Runs `fill_lattice` over a generous fill region covering `[lo, hi]³`.
fn fill(
    unit_cell: &UnitCellStruct,
    motif: Motif,
    geometry: GeoNode,
    options: &LatticeFillOptions,
    regions: Vec<RegionSpec>,
    lo: f64,
    hi: f64,
) -> AtomicStructure {
    let config = LatticeFillConfig {
        unit_cell: unit_cell.clone(),
        motif,
        parameter_element_values: HashMap::new(),
        geometry,
        motif_offset: DVec3::ZERO,
        regions,
    };
    let fill_region = DAABox::new(DVec3::splat(lo), DVec3::splat(hi));
    fill_lattice(&config, options, &fill_region).atomic_structure
}

/// Single carbon site per cell, no bonds: every placed atom is unbonded ("lone"),
/// so `rm_unbonded` fully determines survival — ideal for membership/margin tests.
fn no_bond_motif() -> Motif {
    parse_motif("site 1 C 0.0 0.0 0.0").unwrap()
}

fn h_count(s: &AtomicStructure) -> usize {
    s.atoms_values().filter(|a| a.atomic_number == 1).count()
}

fn min_x(s: &AtomicStructure) -> f64 {
    s.atoms_values()
        .map(|a| a.position.x)
        .fold(f64::INFINITY, f64::min)
}

fn has_atom_near_x(s: &AtomicStructure, x: f64) -> bool {
    s.atoms_values().any(|a| (a.position.x - x).abs() < 0.05)
}

/// Positions sorted by (x, y, z) for order-independent comparison.
fn sorted_positions(s: &AtomicStructure) -> Vec<DVec3> {
    let mut ps: Vec<DVec3> = s.atoms_values().map(|a| a.position).collect();
    ps.sort_by(|p, q| {
        p.x.total_cmp(&q.x)
            .then(p.y.total_cmp(&q.y))
            .then(p.z.total_cmp(&q.z))
    });
    ps
}

// ---------------------------------------------------------------------------
// Empty / inert regions == today's behavior
// ---------------------------------------------------------------------------

/// An all-unset covering region produces exactly the same result as no regions
/// at all (the painter's resolution falls through to the root for every field).
#[test]
fn inert_covering_region_equals_no_regions() {
    let cell = UnitCellStruct::cubic_diamond();
    let geo = || axis_box(DVec3::ZERO, DVec3::splat(4.0 * A));
    let options = opts(true, true, false, false, false);

    let baseline = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![],
        -10.0,
        5.0 * A,
    );

    let covering = region(axis_box(DVec3::splat(-100.0), DVec3::splat(100.0)));
    let with_region = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![covering],
        -10.0,
        5.0 * A,
    );

    assert_eq!(
        sorted_positions(&baseline),
        sorted_positions(&with_region),
        "An inert (all-unset) covering region must be a no-op"
    );
}

// ---------------------------------------------------------------------------
// rm_unbonded — membership, margin, array order, inheritance
// ---------------------------------------------------------------------------

/// Lone atoms are kept outside the region but stripped inside it.
#[test]
fn rm_unbonded_regional_strips_only_inside() {
    let cell = UnitCellStruct::cubic_diamond();
    let geo = || axis_box(DVec3::new(-0.5, -0.5, -0.5), DVec3::new(6.5 * A, 0.5, 0.5));
    // root keeps lone atoms; region (x < 2a) strips them.
    let options = opts(false, false, false, false, false);

    let baseline = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![],
        -10.0,
        7.0 * A,
    );

    let mut r = region(GeoNode::half_space(
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(2.0 * A, 0.0, 0.0),
    ));
    r.rm_unbonded = Some(true);
    let result = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![r],
        -10.0,
        7.0 * A,
    );

    assert!(
        result.get_num_of_atoms() < baseline.get_num_of_atoms(),
        "Some lone atoms inside the region must be removed"
    );
    assert!(
        result.get_num_of_atoms() > 0,
        "Atoms outside the region must survive"
    );
    // Layers at x = 0, a, 2a are inside (sdf <= margin) → removed; survivors start at 3a.
    assert!(
        min_x(&result) > 2.0 * A + DEFAULT_REGION_MARGIN,
        "No surviving atom may be inside the strip region (min_x = {})",
        min_x(&result)
    );
}

/// Default margin captures the boundary-coincident layer; a negative margin excludes it.
#[test]
fn rm_unbonded_margin_default_vs_negative() {
    let cell = UnitCellStruct::cubic_diamond();
    let geo = || axis_box(DVec3::new(-0.5, -0.5, -0.5), DVec3::new(6.5 * A, 0.5, 0.5));
    let options = opts(false, false, false, false, false);
    let plane = || GeoNode::half_space(DVec3::new(1.0, 0.0, 0.0), DVec3::new(2.0 * A, 0.0, 0.0));

    // Default margin (0.1): the x = 2a layer sits at sdf ≈ 0 ≤ margin → stripped.
    let mut r_default = region(plane());
    r_default.rm_unbonded = Some(true);
    let default = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![r_default],
        -10.0,
        7.0 * A,
    );

    // Negative margin (-0.1): the x = 2a layer (sdf ≈ 0 > -0.1) is excluded → kept.
    let mut r_neg = region(plane());
    r_neg.margin = -0.1;
    r_neg.rm_unbonded = Some(true);
    let negative = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![r_neg],
        -10.0,
        7.0 * A,
    );

    assert!(
        negative.get_num_of_atoms() > default.get_num_of_atoms(),
        "A negative margin keeps the boundary layer the default margin strips"
    );
    assert!(
        !has_atom_near_x(&default, 2.0 * A),
        "Default margin must strip the boundary-coincident x = 2a layer"
    );
    assert!(
        has_atom_near_x(&negative, 2.0 * A),
        "Negative margin must keep the boundary-coincident x = 2a layer"
    );
}

/// Overlapping regions: the latest-in-array containing region wins in the overlap band.
#[test]
fn rm_unbonded_overlap_array_order_decides() {
    let cell = UnitCellStruct::cubic_diamond();
    let geo = || axis_box(DVec3::new(-0.5, -0.5, -0.5), DVec3::new(6.5 * A, 0.5, 0.5));
    let options = opts(false, false, false, false, false);

    // keep_region covers x < 5a (rm_unbonded = false); strip_region covers x < 2a (= true).
    let keep_region = || {
        let mut r = region(GeoNode::half_space(
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(5.0 * A, 0.0, 0.0),
        ));
        r.rm_unbonded = Some(false);
        r
    };
    let strip_region = || {
        let mut r = region(GeoNode::half_space(
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(2.0 * A, 0.0, 0.0),
        ));
        r.rm_unbonded = Some(true);
        r
    };

    // Order [keep, strip]: strip is later → wins in x < 2a overlap → those atoms removed.
    let strip_wins = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![keep_region(), strip_region()],
        -10.0,
        7.0 * A,
    );
    // Order [strip, keep]: keep is later → wins everywhere it covers (x < 5a) → nothing removed.
    let keep_wins = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![strip_region(), keep_region()],
        -10.0,
        7.0 * A,
    );

    assert!(
        strip_wins.get_num_of_atoms() < keep_wins.get_num_of_atoms(),
        "Array order must decide the overlap band: strip-last removes atoms, keep-last does not \
         ({} vs {})",
        strip_wins.get_num_of_atoms(),
        keep_wins.get_num_of_atoms()
    );
    assert!(
        min_x(&strip_wins) > 2.0 * A,
        "strip-last must remove the x < 2a atoms"
    );
}

/// A region that sets only one field inherits the rest from the root (per-field resolution).
#[test]
fn region_inherits_unset_fields_from_root() {
    let cell = UnitCellStruct::cubic_diamond();
    let geo = || axis_box(DVec3::new(-0.5, -0.5, -0.5), DVec3::new(6.5 * A, 0.5, 0.5));
    // root keeps lone atoms.
    let options = opts(false, false, false, false, false);
    let plane = || GeoNode::half_space(DVec3::new(1.0, 0.0, 0.0), DVec3::new(2.0 * A, 0.0, 0.0));

    let baseline = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![],
        -10.0,
        7.0 * A,
    );

    // Region sets ONLY surf_recon (a no-op on this non-zincblende motif). rm_unbonded
    // stays unset → inherits root (false) → atoms are still kept inside the region.
    let mut r_surf = region(plane());
    r_surf.surf_recon = Some(true);
    let surf_only = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![r_surf],
        -10.0,
        7.0 * A,
    );

    // Region sets rm_unbonded = true → atoms inside are stripped.
    let mut r_rm = region(plane());
    r_rm.rm_unbonded = Some(true);
    let rm = fill(
        &cell,
        no_bond_motif(),
        geo(),
        &options,
        vec![r_rm],
        -10.0,
        7.0 * A,
    );

    assert_eq!(
        surf_only.get_num_of_atoms(),
        baseline.get_num_of_atoms(),
        "Unset rm_unbonded must inherit the root value (keep), leaving the count unchanged"
    );
    assert!(
        rm.get_num_of_atoms() < baseline.get_num_of_atoms(),
        "Explicitly setting rm_unbonded = true must strip atoms inside the region"
    );
}

// ---------------------------------------------------------------------------
// passivate — regional gate, covering equivalence
// ---------------------------------------------------------------------------

/// `passivate` off on a top region leaves fewer hydrogens than global passivation,
/// while a covering region with `passivate = true` reproduces global passivation.
#[test]
fn passivate_regional_and_covering() {
    let cell = UnitCellStruct::cubic_diamond();
    let motif = || DEFAULT_ZINCBLENDE_MOTIF.clone();
    let geo = || axis_box(DVec3::ZERO, DVec3::splat(4.0 * A));
    let lo = -10.0;
    let hi = 5.0 * A;

    // Global passivation baseline.
    let global = fill(
        &cell,
        motif(),
        geo(),
        &opts(true, true, false, false, false),
        vec![],
        lo,
        hi,
    );
    let h_global = h_count(&global);
    assert!(h_global > 0, "global passivation should add hydrogens");

    // passivate off in the top half (z > 2a) only → fewer H, but the bottom is still passivated.
    let mut top = region(GeoNode::half_space(
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(0.0, 0.0, 2.0 * A),
    ));
    top.passivate = Some(false);
    let regional = fill(
        &cell,
        motif(),
        geo(),
        &opts(true, true, false, false, false),
        vec![top],
        lo,
        hi,
    );
    let h_regional = h_count(&regional);
    assert!(
        h_regional > 0 && h_regional < h_global,
        "passivate off on top only must reduce H without eliminating it (regional {h_regional}, global {h_global})"
    );

    // Root passivate OFF + a covering region passivate = true ≡ global passivation.
    let covering = {
        let mut r = region(axis_box(DVec3::splat(-100.0), DVec3::splat(100.0)));
        r.passivate = Some(true);
        r
    };
    let covered = fill(
        &cell,
        motif(),
        geo(),
        &opts(false, true, false, false, false),
        vec![covering],
        lo,
        hi,
    );
    assert_eq!(
        h_count(&covered),
        h_global,
        "a covering passivate=true region must reproduce global passivation"
    );
}

// ---------------------------------------------------------------------------
// rm_single — regional removal between baseline and global
// ---------------------------------------------------------------------------

/// Regional `rm_single` removes more atoms than the disabled baseline but fewer
/// than enabling it globally.
#[test]
fn rm_single_regional_between_baseline_and_global() {
    let cell = UnitCellStruct::cubic_diamond();
    let motif = || DEFAULT_ZINCBLENDE_MOTIF.clone();
    let geo = || axis_box(DVec3::ZERO, DVec3::splat(4.0 * A));
    let lo = -10.0;
    let hi = 5.0 * A;

    let base = fill(
        &cell,
        motif(),
        geo(),
        &opts(false, true, false, false, false),
        vec![],
        lo,
        hi,
    );

    let mut top = region(GeoNode::half_space(
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(0.0, 0.0, 2.0 * A),
    ));
    top.rm_single = Some(true);
    let regional = fill(
        &cell,
        motif(),
        geo(),
        &opts(false, true, false, false, false),
        vec![top],
        lo,
        hi,
    );

    let global = fill(
        &cell,
        motif(),
        geo(),
        &opts(false, true, true, false, false),
        vec![],
        lo,
        hi,
    );

    assert!(
        global.get_num_of_atoms() < regional.get_num_of_atoms()
            && regional.get_num_of_atoms() < base.get_num_of_atoms(),
        "regional rm_single must sit strictly between baseline and global \
         (base {}, regional {}, global {})",
        base.get_num_of_atoms(),
        regional.get_num_of_atoms(),
        global.get_num_of_atoms()
    );
}

// ---------------------------------------------------------------------------
// surf_recon — regional dimerization adds bonds between baseline and global
// ---------------------------------------------------------------------------

/// Regional surface reconstruction forms some dimers (more bonds than no recon)
/// but fewer than reconstructing the whole surface.
#[test]
fn surf_recon_regional_between_baseline_and_global() {
    let cell = UnitCellStruct::cubic_diamond();
    let motif = || DEFAULT_ZINCBLENDE_MOTIF.clone();
    let geo = || axis_box(DVec3::ZERO, DVec3::splat(4.0 * A));
    let lo = -10.0;
    let hi = 5.0 * A;
    // rm_single on (so reconstruction's internal removal is a no-op), no passivation
    // (so the only bond-count delta is dimer bonds).
    let base_opts = || opts(false, true, true, false, false);
    let recon_opts = || opts(false, true, true, true, false);

    let no_recon = fill(&cell, motif(), geo(), &base_opts(), vec![], lo, hi);

    let mut top = region(GeoNode::half_space(
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(0.0, 0.0, 2.0 * A),
    ));
    top.surf_recon = Some(true);
    let regional = fill(&cell, motif(), geo(), &base_opts(), vec![top], lo, hi);

    let global = fill(&cell, motif(), geo(), &recon_opts(), vec![], lo, hi);

    assert!(
        no_recon.get_num_of_bonds() < regional.get_num_of_bonds()
            && regional.get_num_of_bonds() < global.get_num_of_bonds(),
        "regional reconstruction must add fewer dimer bonds than global but more than none \
         (none {}, regional {}, global {})",
        no_recon.get_num_of_bonds(),
        regional.get_num_of_bonds(),
        global.get_num_of_bonds()
    );
}

// ---------------------------------------------------------------------------
// passiv_elem — per-region terminator element override (D8)
// ---------------------------------------------------------------------------

/// A region overriding only `passiv_elem` (inheriting the booleans) fluorinates
/// its volume while the rest of the surface stays hydrogen-terminated. The total
/// terminator count is conserved (both elements are monovalent).
#[test]
fn passiv_elem_regional_override() {
    let cell = UnitCellStruct::cubic_diamond();
    let motif = || DEFAULT_ZINCBLENDE_MOTIF.clone();
    let geo = || axis_box(DVec3::ZERO, DVec3::splat(4.0 * A));
    let lo = -10.0;
    let hi = 5.0 * A;
    let f_count = |s: &AtomicStructure| s.atoms_values().filter(|a| a.atomic_number == 9).count();

    // All-hydrogen baseline (no region).
    let global_h = fill(
        &cell,
        motif(),
        geo(),
        &opts(true, true, false, false, false),
        vec![],
        lo,
        hi,
    );
    assert!(h_count(&global_h) > 0, "baseline must add hydrogens");
    assert_eq!(
        f_count(&global_h),
        0,
        "no fluorine without a region override"
    );

    // Top half (z > 2a): passiv_elem = F only; passivate inherited (stays on).
    let mut top = region(GeoNode::half_space(
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(0.0, 0.0, 2.0 * A),
    ));
    top.passiv_elem = Some(9);
    let mixed = fill(
        &cell,
        motif(),
        geo(),
        &opts(true, true, false, false, false),
        vec![top],
        lo,
        hi,
    );

    assert!(f_count(&mixed) > 0, "the top region must place fluorine");
    assert!(
        h_count(&mixed) > 0,
        "the bottom must still be hydrogen-terminated"
    );
    assert_eq!(
        h_count(&mixed) + f_count(&mixed),
        h_count(&global_h),
        "swapping the element must not change the terminator count"
    );
}

// ---------------------------------------------------------------------------
// invert_phase — covering region reproduces global invert (per-atom plumbing)
// ---------------------------------------------------------------------------

/// A covering `invert_phase = true` region yields exactly the same reconstructed
/// structure as the global `invert_phase` flag (proves the per-atom resolution
/// feeds `is_primary_dimer_atom`).
#[test]
fn invert_phase_covering_equals_global() {
    let cell = UnitCellStruct::cubic_diamond();
    let motif = || DEFAULT_ZINCBLENDE_MOTIF.clone();
    let geo = || axis_box(DVec3::ZERO, DVec3::splat(4.0 * A));
    let lo = -10.0;
    let hi = 5.0 * A;

    // Global invert_phase = true.
    let global = fill(
        &cell,
        motif(),
        geo(),
        &opts(false, true, true, true, true),
        vec![],
        lo,
        hi,
    );

    // Root invert_phase = false + a covering region invert_phase = Some(true).
    let covering = {
        let mut r = region(axis_box(DVec3::splat(-100.0), DVec3::splat(100.0)));
        r.invert_phase = Some(true);
        r
    };
    let covered = fill(
        &cell,
        motif(),
        geo(),
        &opts(false, true, true, true, false),
        vec![covering],
        lo,
        hi,
    );

    let g = sorted_positions(&global);
    let c = sorted_positions(&covered);
    assert_eq!(g.len(), c.len(), "atom counts must match");
    for (pg, pc) in g.iter().zip(c.iter()) {
        assert!(
            pg.distance(*pc) < 1e-9,
            "covering invert region must reproduce global invert exactly ({pg} vs {pc})"
        );
    }
    assert_eq!(global.get_num_of_bonds(), covered.get_num_of_bonds());
}
