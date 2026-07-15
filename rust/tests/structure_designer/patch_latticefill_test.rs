//! Phase 3 tests for `patch_latticefill` apply + compatibility stats (see
//! `doc/design_surface_patches.md` §5 / §6 / §9 Phase 3).
//!
//! The core `apply_patch` (and the public `select_patch_cells` helper) are plain
//! functions, tested here on hand-built `AtomicStructure`s without the
//! node-network machinery. The mechanics under test: cut-then-weld, periodic
//! (tile↔tile) and bulk (tile↔collar) welds, dropping unwelded patch-ghosts,
//! the asymmetric periodic/non-periodic containment rule, and the
//! welded/orphaned/over-coordination compatibility stats.

use glam::f64::DVec3;
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::Atom;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::nodes::patch_latticefill::{
    CompatibilityReport, apply_patch, region_center_depths, select_patch_cells,
};
use rust_lib_flutter_cad::util::daabox::DAABox;

const CARBON: i16 = 6;
const SINGLE: u8 = 1;

/// Non-debug `apply_patch` (both debug flags off) — keeps the existing tests
/// terse now that the core takes two extra debug booleans.
#[allow(clippy::too_many_arguments)]
fn apply_patch_t(
    target: &AtomicStructure,
    region_lattice: &UnitCellStruct,
    region_volume: Option<&GeoNode>,
    region_bounds: &DAABox,
    tile: &AtomicStructure,
    tiling_vectors: &[glam::i32::IVec3],
    cut_volume: &GeoNode,
    origin: glam::i32::IVec3,
    passivate: bool,
    tolerance: f64,
) -> (AtomicStructure, CompatibilityReport) {
    apply_patch(
        target,
        region_lattice,
        region_volume,
        region_bounds,
        tile,
        tiling_vectors,
        cut_volume,
        origin,
        passivate,
        tolerance,
        true, // test_height_at_origin (the default)
        false,
        false,
    )
    .expect("apply_patch: tag tables fit within test fixtures")
}

/// A cubic lattice with edge `L` — keeps lattice/real arithmetic legible.
fn cubic(l: f64) -> UnitCellStruct {
    UnitCellStruct::new(
        DVec3::new(l, 0.0, 0.0),
        DVec3::new(0.0, l, 0.0),
        DVec3::new(0.0, 0.0, l),
    )
}

fn box_bounds(min: DVec3, max: DVec3) -> DAABox {
    DAABox::from_min_max(min, max)
}

fn find_atom_at(s: &AtomicStructure, pos: DVec3, tol: f64) -> Option<&Atom> {
    s.iter_atoms()
        .find(|(_, a)| (a.position - pos).length() < tol)
        .map(|(_, a)| a)
}

fn num_ghosts(s: &AtomicStructure) -> usize {
    s.atoms_values().filter(|a| a.is_patch_ghost()).count()
}

/// Number of active bonds on the atom at `pos`.
fn bonds_at(s: &AtomicStructure, pos: DVec3) -> usize {
    let atom = find_atom_at(s, pos, 1e-6).expect("atom must exist");
    atom.bonds.iter().filter(|b| !b.is_delete_marker()).count()
}

/// A cut volume that contains nothing relevant (degenerate point sphere).
fn empty_cut() -> GeoNode {
    GeoNode::sphere(DVec3::ZERO, 0.0)
}

// ============================================================================
// 1. Periodic weld (tile↔tile): adjacent tiles' shared/ghost atoms coincide and
//    weld into a continuous structure; the boundary-crossing bond becomes
//    ordinary; no duplicate atoms remain.
// ============================================================================

#[test]
fn periodic_weld_fuses_adjacent_tiles() {
    let lattice = cubic(4.0);
    // Tile: interior A + a ghost copy of the next tile's A (the cross-edge bond
    // partner) one tiling vector over. The neighbour's real A welds onto it.
    let mut tile = AtomicStructure::new();
    let a = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 1.0));
    let g = tile.add_atom(CARBON, DVec3::new(5.0, 1.0, 1.0)); // = A + (4,0,0)
    tile.set_atom_patch_ghost(g, true);
    tile.add_bond(a, g, SINGLE);

    let target = AtomicStructure::new(); // tile↔tile only
    let tiling = [IVec3::new(1, 0, 0)];
    // Select exactly cells 0 and +1.
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(8.5, 2.0, 2.0));

    let (result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false, // no passivation
        0.1,
    );

    // A0 + welded(G0=A1); the far ghost G1 is dropped.
    assert_eq!(
        result.get_num_of_atoms(),
        2,
        "exactly two atoms after weld+drop"
    );
    assert_eq!(
        result.get_num_of_bonds(),
        1,
        "the boundary bond is now ordinary"
    );
    assert_eq!(num_ghosts(&result), 0, "no patch-ghosts remain");
    assert!(
        find_atom_at(&result, DVec3::new(5.0, 1.0, 1.0), 1e-6).is_some(),
        "the welded boundary atom is present"
    );
    assert_eq!(report.welded_ghosts, 1);
    assert_eq!(report.orphaned_ghosts, 1); // the outer ghost of the +1 tile
}

// ============================================================================
// 2. Bulk weld (tile↔collar): collar patch-ghosts weld onto surviving substrate
//    atoms and inherit bulk bonds; coordination preserved.
// ============================================================================

#[test]
fn bulk_weld_collar_inherits_bulk_bonds() {
    let lattice = cubic(4.0);
    // Substrate: a sub-surface atom B (the collar's target) bonded deeper to D.
    let mut target = AtomicStructure::new();
    let b = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    let d = target.add_atom(CARBON, DVec3::new(1.0, 1.0, -4.0));
    target.add_bond(b, d, SINGLE);

    // Tile: interior I, collar ghost C coincident with B, bond I–C.
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 4.0));
    let c = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    tile.set_atom_patch_ghost(c, true);
    tile.add_bond(i, c, SINGLE);

    let tiling = [IVec3::new(1, 0, 0)];
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let (result, _report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
    );

    // D, the welded collar (now real), and the interior I.
    assert_eq!(result.get_num_of_atoms(), 3);
    assert_eq!(result.get_num_of_bonds(), 2);
    assert_eq!(num_ghosts(&result), 0, "collar welded → real, flag cleared");
    // Continuous chain: bulk —(inherited)— collar —(tile)— interior.
    assert_eq!(
        bonds_at(&result, DVec3::new(1.0, 1.0, 0.0)),
        2,
        "the welded collar carries both the inherited bulk bond and the tile bond"
    );
}

// ============================================================================
// 3. Cut-then-weld coordination: the cut removes the displaced surface and the
//    collar's bond to it; the collar's inward bond replaces it — no net dangler.
// ============================================================================

#[test]
fn cut_then_weld_preserves_collar_coordination() {
    let lattice = cubic(4.0);
    // Substrate: D — B — Sold (the old surface atom the reconstruction displaces).
    let mut target = AtomicStructure::new();
    let d = target.add_atom(CARBON, DVec3::new(1.0, 1.0, -4.0));
    let b = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    let s_old = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 4.0));
    target.add_bond(d, b, SINGLE);
    target.add_bond(b, s_old, SINGLE);

    // Cut volume removes the old surface (a sphere around Sold) but not B/D.
    let cut = GeoNode::sphere(DVec3::new(1.0, 1.0, 4.0), 1.0);

    // Tile: interior I (replaces the old surface) + collar ghost C onto B.
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 4.0));
    let c = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    tile.set_atom_patch_ghost(c, true);
    tile.add_bond(i, c, SINGLE);

    let tiling = [IVec3::new(1, 0, 0)];
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let (result, _report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &cut,
        IVec3::ZERO,
        false,
        0.1,
    );

    // Sold is gone, replaced by the tile interior; B (welded) keeps coordination 2.
    assert_eq!(result.get_num_of_atoms(), 3);
    assert_eq!(result.get_num_of_bonds(), 2);
    assert_eq!(
        bonds_at(&result, DVec3::new(1.0, 1.0, 0.0)),
        2,
        "the welded collar's coordination is preserved (D + interior, no dangler)"
    );
    assert!(
        !find_atom_at(&result, DVec3::new(1.0, 1.0, 4.0), 1e-6)
            .unwrap()
            .is_patch_ghost(),
        "the atom now at the old surface position is the real tile interior"
    );
}

// ============================================================================
// 4. Drop unwelded patch-ghosts: a tile at a true edge (no neighbour in P)
//    leaves a dangling bond on the boundary interior atom after its ghost drops.
// ============================================================================

#[test]
fn unwelded_ghost_is_dropped_leaving_a_dangler() {
    let lattice = cubic(4.0);
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 1.0));
    let i2 = tile.add_atom(CARBON, DVec3::new(2.5, 1.0, 1.0));
    let g = tile.add_atom(CARBON, DVec3::new(5.0, 1.0, 1.0)); // cross-edge ghost
    tile.set_atom_patch_ghost(g, true);
    tile.add_bond(i, i2, SINGLE);
    tile.add_bond(i, g, SINGLE);

    let target = AtomicStructure::new();
    let tiling = [IVec3::new(1, 0, 0)];
    // Only cell 0 (no neighbour for the ghost to weld onto).
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let (result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
    );

    assert_eq!(
        result.get_num_of_atoms(),
        2,
        "I + I2; the ghost was dropped"
    );
    assert_eq!(result.get_num_of_bonds(), 1);
    assert_eq!(num_ghosts(&result), 0);
    assert_eq!(
        bonds_at(&result, DVec3::new(1.0, 1.0, 1.0)),
        1,
        "the interior atom lost its bond to the dropped ghost (a dangler)"
    );
    assert_eq!(report.orphaned_ghosts, 1);
    assert_eq!(report.welded_ghosts, 0);
}

// ============================================================================
// 5. Containment rule: periodic directions require whole-cell containment; the
//    non-periodic direction(s) are free. 1D / 2D / 3D exercised.
// ============================================================================

/// One cell's footprint as a set of corner points: all subset-sums of the real
/// tiling vectors at the origin cell. Used so "all interior atoms inside" tests
/// whole-cell containment, matching the design's "no partial lateral tiles".
fn cell_corners(tiling: &[IVec3], lattice: &UnitCellStruct) -> Vec<DVec3> {
    let real: Vec<DVec3> = tiling
        .iter()
        .map(|v| lattice.ivec3_lattice_to_real(v))
        .collect();
    (0..(1u32 << real.len()))
        .map(|mask| {
            let mut p = DVec3::ZERO;
            for (i, rv) in real.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    p += *rv;
                }
            }
            p
        })
        .collect()
}

#[test]
fn containment_2d_normal_is_free() {
    let lattice = cubic(4.0);
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    let interior = cell_corners(&tiling, &lattice); // four x–y corners at z=0
    // Region far away in z; z is the free (normal) axis, so cells are selected
    // by their x–y footprint after projection to the region's z centre.
    let bounds = box_bounds(DVec3::new(-0.5, -0.5, 20.0), DVec3::new(8.5, 8.5, 30.0));
    let free_dirs = vec![DVec3::Z];
    let center_depths = vec![25.0];

    let cells = select_patch_cells(
        IVec3::ZERO,
        &tiling,
        &lattice,
        None,
        &bounds,
        &interior,
        &free_dirs,
        &center_depths,
    );

    assert_eq!(cells.len(), 4, "the 2×2 block whose x–y footprint fits");
    for o in [
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(1, 1, 0),
    ] {
        assert!(
            cells.iter().any(|c| c.offset == o),
            "cell {o:?} should be placed"
        );
    }
}

#[test]
fn containment_1d_transverse_free() {
    let lattice = cubic(4.0);
    let tiling = [IVec3::new(1, 0, 0)];
    let interior = cell_corners(&tiling, &lattice); // two x endpoints at y=z=0
    // Both transverse axes (y, z) are free; only the x footprint gates.
    let bounds = box_bounds(DVec3::new(-0.5, 20.0, 20.0), DVec3::new(8.5, 30.0, 30.0));
    let free_dirs = vec![DVec3::Y, DVec3::Z];
    let center_depths = vec![25.0, 25.0];

    let cells = select_patch_cells(
        IVec3::ZERO,
        &tiling,
        &lattice,
        None,
        &bounds,
        &interior,
        &free_dirs,
        &center_depths,
    );

    assert_eq!(cells.len(), 2);
    assert!(cells.iter().any(|c| c.offset == IVec3::new(0, 0, 0)));
    assert!(cells.iter().any(|c| c.offset == IVec3::new(1, 0, 0)));
}

#[test]
fn containment_3d_has_no_free_axis() {
    let lattice = cubic(4.0);
    let tiling = [
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, 0, 1),
    ];
    let interior = cell_corners(&tiling, &lattice); // 8 corners of one cubic cell
    let free_dirs: Vec<DVec3> = vec![]; // no free axis
    let center_depths: Vec<f64> = vec![];

    // Tight box around the origin cell → exactly one cell.
    let tight = box_bounds(DVec3::new(-0.5, -0.5, -0.5), DVec3::new(4.5, 4.5, 4.5));
    let cells = select_patch_cells(
        IVec3::ZERO,
        &tiling,
        &lattice,
        None,
        &tight,
        &interior,
        &free_dirs,
        &center_depths,
    );
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].offset, IVec3::ZERO);

    // With no free axis, z is a periodic direction requiring containment: a
    // region offset in z does NOT place the origin-plane cell (unlike the 2D
    // case above, where z is free and the origin cell is placed regardless).
    let far_z = box_bounds(DVec3::new(-0.5, -0.5, 20.0), DVec3::new(4.5, 4.5, 30.0));
    let far = select_patch_cells(
        IVec3::ZERO,
        &tiling,
        &lattice,
        None,
        &far_z,
        &interior,
        &free_dirs,
        &center_depths,
    );
    assert!(
        !far.iter().any(|c| c.offset == IVec3::ZERO),
        "the origin-plane cell is not placed when z is periodic and out of range"
    );
}

// ============================================================================
// 6. Cut == place cell set: substrate is never removed in a cell that is not
//    also reconstructed.
// ============================================================================

#[test]
fn cut_only_happens_in_placed_cells() {
    let lattice = cubic(4.0);
    let mut target = AtomicStructure::new();
    target.add_atom(CARBON, DVec3::new(2.0, 1.0, 1.0)); // inside cut @ cell 0
    target.add_atom(CARBON, DVec3::new(6.0, 1.0, 1.0)); // would be inside cut @ cell +1

    // Tile has one interior atom that fits cell 0's region but not cell +1's, so
    // cell +1 is not selected → its substrate atom must not be cut.
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::new(2.0, 1.0, 1.0));
    let cut = GeoNode::sphere(DVec3::new(2.0, 1.0, 1.0), 1.5);

    let tiling = [IVec3::new(1, 0, 0)];
    // x range admits cell 0's interior atom (x=2) but not cell +1's (x=6).
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let (result, _report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &cut,
        IVec3::ZERO,
        false,
        0.1,
    );

    // Cell 0: original (2,1,1) cut, replaced by the placed tile atom. Cell +1 not
    // selected → its (6,1,1) survives, and nothing was placed there.
    assert_eq!(result.get_num_of_atoms(), 2);
    assert!(
        find_atom_at(&result, DVec3::new(6.0, 1.0, 1.0), 1e-6).is_some(),
        "the atom in the un-reconstructed cell survives (cell +1 not cut)"
    );
    assert!(
        find_atom_at(&result, DVec3::new(2.0, 1.0, 1.0), 1e-6).is_some(),
        "the reconstructed cell holds the placed tile atom"
    );
}

// ============================================================================
// 7. Passivate on/off: true saturates residual danglers; false leaves them.
// ============================================================================

#[test]
fn passivation_saturates_danglers_when_enabled() {
    let lattice = cubic(4.0);
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 1.0));
    let i2 = tile.add_atom(CARBON, DVec3::new(2.54, 1.0, 1.0));
    let g = tile.add_atom(CARBON, DVec3::new(5.0, 1.0, 1.0));
    tile.set_atom_patch_ghost(g, true);
    tile.add_bond(i, i2, SINGLE);
    tile.add_bond(i, g, SINGLE);

    let target = AtomicStructure::new();
    let tiling = [IVec3::new(1, 0, 0)];
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let run = |passivate: bool| {
        apply_patch_t(
            &target,
            &lattice,
            None,
            &bounds,
            &tile,
            &tiling,
            &empty_cut(),
            IVec3::ZERO,
            passivate,
            0.1,
        )
        .0
    };

    let off = run(false);
    assert_eq!(
        off.atoms_values().filter(|a| a.atomic_number == 1).count(),
        0,
        "no hydrogens when passivation is off"
    );

    let on = run(true);
    assert!(
        on.atoms_values().filter(|a| a.atomic_number == 1).count() > 0,
        "hydrogens added to saturate the under-coordinated carbons"
    );
}

// ============================================================================
// 8. Tolerance: distinct-but-close sites do not over-merge at the default 0.1 Å.
// ============================================================================

#[test]
fn close_sites_do_not_over_merge() {
    let lattice = cubic(4.0);
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0));
    tile.add_atom(CARBON, DVec3::new(0.2, 0.0, 0.0)); // 0.2 Å > 0.1 Å tolerance

    let target = AtomicStructure::new();
    let tiling = [IVec3::new(1, 0, 0)];
    // x range tight enough that only the origin cell's interior atoms fit.
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(2.5, 2.0, 2.0));

    let (result, _report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
    );

    assert_eq!(
        result.get_num_of_atoms(),
        2,
        "two sites 0.2 Å apart must not weld at the 0.1 Å tolerance"
    );
}

// ============================================================================
// 9. Golden end-to-end: a small slab + a 1D 2-cell reconstruction tile (cut +
//    place + periodic weld + collar weld) produces the expected connectivity.
// ============================================================================

#[test]
fn golden_two_cell_reconstruction() {
    let lattice = cubic(4.0);
    // Substrate: two sub-surface atoms bonded together, each carrying an old
    // surface atom the reconstruction displaces.
    let mut target = AtomicStructure::new();
    let b0 = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    let b1 = target.add_atom(CARBON, DVec3::new(5.0, 1.0, 0.0));
    let s0 = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 3.0));
    let s1 = target.add_atom(CARBON, DVec3::new(5.0, 1.0, 3.0));
    target.add_bond(b0, b1, SINGLE);
    target.add_bond(b0, s0, SINGLE);
    target.add_bond(b1, s1, SINGLE);

    // Cut removes the old surface atom of each cell (sphere around the local
    // surface position, tiled with the patch).
    let cut = GeoNode::sphere(DVec3::new(1.0, 1.0, 3.0), 1.0);

    // Tile: interior I + collar ghost C (onto B) + cross-edge ghost E (the next
    // cell's interior). Bonds I–C and I–E.
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 2.0));
    let c = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    let e = tile.add_atom(CARBON, DVec3::new(5.0, 1.0, 2.0));
    tile.set_atom_patch_ghost(c, true);
    tile.set_atom_patch_ghost(e, true);
    tile.add_bond(i, c, SINGLE);
    tile.add_bond(i, e, SINGLE);

    let tiling = [IVec3::new(1, 0, 0)];
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(8.5, 2.0, 2.0));

    let (result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &cut,
        IVec3::ZERO,
        false,
        0.1,
    );

    // Four real atoms in a closed 4-cycle; the far edge ghost is dropped.
    assert_eq!(result.get_num_of_atoms(), 4, "atom count");
    assert_eq!(result.get_num_of_bonds(), 4, "bond count (closed loop)");
    assert_eq!(num_ghosts(&result), 0);
    for pos in [
        DVec3::new(1.0, 1.0, 2.0),
        DVec3::new(1.0, 1.0, 0.0),
        DVec3::new(5.0, 1.0, 2.0),
        DVec3::new(5.0, 1.0, 0.0),
    ] {
        assert_eq!(
            bonds_at(&result, pos),
            2,
            "every atom in the loop has 2 bonds"
        );
    }
    assert_eq!(report.welded_ghosts, 3);
    assert_eq!(report.orphaned_ghosts, 1);
}

// ============================================================================
// 10. Compatibility stats: applied too high → orphaned > 0; correct depth →
//     zero orphaned + clean coordination; too low → over-coordinated weld.
// ============================================================================

fn collar_tile() -> AtomicStructure {
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 4.0));
    let c = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    tile.set_atom_patch_ghost(c, true);
    tile.add_bond(i, c, SINGLE);
    tile
}

fn one_cell_bounds() -> DAABox {
    box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0))
}

#[test]
fn compatibility_correct_depth_is_clean() {
    let lattice = cubic(4.0);
    let mut target = AtomicStructure::new();
    let b = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    let d = target.add_atom(CARBON, DVec3::new(1.0, 1.0, -4.0));
    target.add_bond(b, d, SINGLE);

    let (_result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &one_cell_bounds(),
        &collar_tile(),
        &[IVec3::new(1, 0, 0)],
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
    );

    assert_eq!(
        report.orphaned_ghosts, 0,
        "the collar found its substrate twin"
    );
    assert_eq!(report.overcoordinated_atoms, 0, "coordination is clean");
}

#[test]
fn compatibility_too_high_orphans_collars() {
    let lattice = cubic(4.0);
    // No substrate atom at the collar position → the collar floats.
    let target = AtomicStructure::new();

    let (_result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &one_cell_bounds(),
        &collar_tile(),
        &[IVec3::new(1, 0, 0)],
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
    );

    assert!(
        report.orphaned_ghosts > 0,
        "the un-welded collar is orphaned"
    );
}

#[test]
fn compatibility_too_low_overcoordinates() {
    let lattice = cubic(4.0);
    // A fully-coordinated substrate carbon: welding the collar onto it adds a
    // fifth bond → over-coordinated.
    let mut target = AtomicStructure::new();
    let b = target.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    for n in [
        DVec3::new(2.0, 1.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(1.0, 2.0, 0.0),
        DVec3::new(1.0, 1.0, 1.0),
    ] {
        let id = target.add_atom(CARBON, n);
        target.add_bond(b, id, SINGLE);
    }

    let (_result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &one_cell_bounds(),
        &collar_tile(),
        &[IVec3::new(1, 0, 0)],
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
    );

    assert_eq!(report.orphaned_ghosts, 0, "the collar still welds");
    assert!(
        report.overcoordinated_atoms > 0,
        "the over-stacked weld is flagged"
    );
}

// ============================================================================
// 11. Default origin reproduces authoring: because the tile keeps its authored
//     coordinates, applying with origin = (0,0,0) lands the tile exactly where
//     it was drawn (no hidden re-anchoring), and the collar welds onto the
//     substrate sitting at that same authored position. This is the behaviour
//     the whole `origin`-as-offset change is for.
// ============================================================================

#[test]
fn default_origin_reproduces_authored_coordinates() {
    let lattice = cubic(4.0);

    // A tile authored at an arbitrary absolute position — deliberately *not*
    // near the lattice origin (cell (3,0,1)-ish), to prove there is no implicit
    // re-anchoring to the origin cell.
    let mut tile = AtomicStructure::new();
    let i = tile.add_atom(CARBON, DVec3::new(13.0, 1.0, 4.0));
    let c = tile.add_atom(CARBON, DVec3::new(13.0, 1.0, 0.0));
    tile.set_atom_patch_ghost(c, true);
    tile.add_bond(i, c, SINGLE);

    // Substrate with an atom exactly at the collar's authored position (plus a
    // deeper neighbour so the welded collar carries an inherited bulk bond).
    let mut target = AtomicStructure::new();
    let b = target.add_atom(CARBON, DVec3::new(13.0, 1.0, 0.0));
    let d = target.add_atom(CARBON, DVec3::new(13.0, 1.0, -4.0));
    target.add_bond(b, d, SINGLE);

    // Region tight in x so only the authored cell's interior atom fits (a
    // single-atom tile fits any cell whose projection lands in the region).
    let bounds = box_bounds(DVec3::new(11.0, -3.0, -8.0), DVec3::new(15.0, 5.0, 8.0));

    let (result, report) = apply_patch_t(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &[IVec3::new(1, 0, 0)],
        &empty_cut(),
        IVec3::ZERO, // default offset → as authored
        false,
        0.1,
    );

    // The interior atom appears exactly where it was drawn (origin 0 = identity).
    assert!(
        find_atom_at(&result, DVec3::new(13.0, 1.0, 4.0), 1e-9).is_some(),
        "the interior atom lands at its authored absolute position"
    );
    // The collar found its substrate twin and welded — nothing left floating.
    assert_eq!(
        report.orphaned_ghosts, 0,
        "the collar welds at the authored position"
    );
    assert_eq!(
        num_ghosts(&result),
        0,
        "no patch-ghosts remain after the weld"
    );
    // The welded collar carries both the inherited bulk bond (to D) and the tile
    // bond (to I): a continuous bulk—collar—interior chain.
    assert_eq!(
        bonds_at(&result, DVec3::new(13.0, 1.0, 0.0)),
        2,
        "the welded collar is bonded to both the bulk and the tile interior"
    );
}

// ============================================================================
// 12. The `origin` offset shifts the reconstruction's phase by whole cells.
//     With a 2-cell tiling period the authored tile lands on even cells; an
//     origin of (1,0,0) shifts the whole pattern onto the odd cells (a genuine
//     one-cell slide — the kind of phase choice that picks which sites pair into
//     dimers). A shift by a full tiling vector would instead be a no-op.
// ============================================================================

#[test]
fn origin_offset_shifts_phase_by_whole_cells() {
    let lattice = cubic(4.0);

    // Tile authored at x = 1; tiling period is 2 cells (8 Å) along x.
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 4.0));
    let c = tile.add_atom(CARBON, DVec3::new(1.0, 1.0, 0.0));
    tile.set_atom_patch_ghost(c, true);

    let target = AtomicStructure::new();
    let tiling = [IVec3::new(2, 0, 0)];
    let bounds = box_bounds(DVec3::new(-6.0, -2.0, -2.0), DVec3::new(14.0, 2.0, 8.0));

    let run = |origin: IVec3| {
        apply_patch_t(
            &target,
            &lattice,
            None,
            &bounds,
            &tile,
            &tiling,
            &empty_cut(),
            origin,
            false,
            0.1,
        )
        .0
    };

    // origin (0,0,0): the authored even-cell tile is present at x = 1.
    let even = run(IVec3::ZERO);
    assert!(
        find_atom_at(&even, DVec3::new(1.0, 1.0, 4.0), 1e-9).is_some(),
        "default origin keeps the tile at its authored even-cell position"
    );

    // origin (1,0,0): the pattern slides one cell — interiors now land on the
    // odd cells (x = 5, -3, …), and nothing sits at the authored x = 1.
    let odd = run(IVec3::new(1, 0, 0));
    assert!(
        find_atom_at(&odd, DVec3::new(5.0, 1.0, 4.0), 1e-9).is_some(),
        "origin (1,0,0) shifts the reconstruction one cell along +x"
    );
    assert!(
        find_atom_at(&odd, DVec3::new(1.0, 1.0, 4.0), 1e-9).is_none(),
        "the authored even-cell position is now empty (phase shifted)"
    );
}

// ============================================================================
// 13. Centre depth is measured along the real normal, not the global AABB. For
//     a slab tilted w.r.t. XYZ the axis-aligned box centre can sit off the slab
//     (the old height bug); the normal-depth midpoint stays on it.
// ============================================================================

#[test]
fn center_depth_is_on_the_tilted_slab_not_the_aabb_centre() {
    // Four atoms on the tilted plane x+y+z=0, chosen so their global AABB centre
    // is (1,0,0) — which is NOT on the plane (1+0+0 = 1 ≠ 0).
    let mut target = AtomicStructure::new();
    for p in [
        DVec3::new(1.0, 0.0, -1.0),
        DVec3::new(0.0, 1.0, -1.0),
        DVec3::new(0.0, -1.0, 1.0),
        DVec3::new(2.0, -1.0, -1.0),
    ] {
        target.add_atom(CARBON, p);
    }
    let normal = DVec3::new(1.0, 1.0, 1.0).normalize();

    // Every atom has x+y+z = 0 → depth along the normal is 0; the midpoint is 0,
    // i.e. exactly on the slab.
    let depths = region_center_depths(&target, &[normal]);
    assert!(
        depths[0].abs() < 1e-9,
        "centre depth is on the slab (~0), got {}",
        depths[0]
    );

    // The old height would have been the AABB centre (1,0,0), whose normal
    // component is 1/sqrt(3) ~ 0.577 — off the slab, where the SDF test fails.
    let aabb_centre = DVec3::new(1.0, 0.0, 0.0);
    assert!(
        aabb_centre.dot(normal).abs() > 0.5,
        "the global AABB centre is off the tilted slab"
    );
}

// ============================================================================
// 14. Symmetric region + symmetric tiling -> symmetric selection (the old
//     corner-anchored footprint biased the selection off-centre).
// ============================================================================

#[test]
fn selection_is_symmetric_for_symmetric_region() {
    let lattice = cubic(4.0);
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    // One interior atom at the cell origin -> selection is symmetric about o=0.
    let interior = vec![DVec3::ZERO];
    let bounds = box_bounds(DVec3::new(-10.0, -10.0, 20.0), DVec3::new(10.0, 10.0, 30.0));
    let free_dirs = vec![DVec3::Z];
    let center_depths = vec![25.0];

    let cells = select_patch_cells(
        IVec3::ZERO,
        &tiling,
        &lattice,
        None,
        &bounds,
        &interior,
        &free_dirs,
        &center_depths,
    );

    assert!(
        cells.len() >= 9,
        "several cells selected, got {}",
        cells.len()
    );
    for c in &cells {
        let mirror = IVec3::new(-c.offset.x, -c.offset.y, -c.offset.z);
        assert!(
            cells.iter().any(|d| d.offset == mirror),
            "selection symmetric: mirror of {:?} must be present",
            c.offset
        );
    }
}

// ============================================================================
// 15. Debug A: project placed atoms to the test plane; no weld/passivation.
// ============================================================================

#[test]
fn debug_project_flattens_atoms_to_test_plane() {
    let lattice = cubic(4.0);
    let mut target = AtomicStructure::new();
    target.add_atom(CARBON, DVec3::new(0.0, 0.0, 0.0)); // centre depth z = 0
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::new(1.0, 0.0, 5.0)); // sticks up at z=5
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    let bounds = box_bounds(DVec3::new(-2.0, -2.0, -2.0), DVec3::new(2.0, 2.0, 8.0));

    let (result, _r) = apply_patch(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        true, // passivate (ignored in project mode)
        0.1,
        true,  // test_height_at_origin
        true,  // debug_project
        false, // debug_frontier
    )
    .expect("apply_patch: tag tables fit within test fixtures");

    // The patch atom (z=5) is flattened onto the test plane (z=0); nothing left
    // at z=5; no hydrogens added (no passivation in project mode).
    assert!(
        find_atom_at(&result, DVec3::new(1.0, 0.0, 0.0), 1e-9).is_some(),
        "patch atom projected onto the test plane z=0"
    );
    assert!(
        find_atom_at(&result, DVec3::new(1.0, 0.0, 5.0), 1e-9).is_none(),
        "nothing left at the original z=5 position"
    );
    assert_eq!(
        result
            .atoms_values()
            .filter(|a| a.atomic_number == 1)
            .count(),
        0,
        "project mode does not passivate"
    );
}

// ============================================================================
// 16. Debug B: frontier overlay places the +/-1 box around the selection, with
//     the excluded neighbours flagged frozen; report unchanged vs. non-debug.
// ============================================================================

#[test]
fn debug_frontier_overlays_excluded_neighbours_frozen() {
    let lattice = cubic(4.0);
    let target = AtomicStructure::new();
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::ZERO); // single interior atom at the cell origin
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    // Tight region: only the origin cell fits.
    let bounds = box_bounds(DVec3::new(-1.0, -1.0, -2.0), DVec3::new(1.0, 1.0, 2.0));

    let plain = apply_patch(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
        true, // test_height_at_origin
        false,
        false,
    )
    .expect("apply_patch: tag tables fit within test fixtures");
    assert_eq!(
        plain.0.get_num_of_atoms(),
        1,
        "only the origin cell is selected"
    );

    let (dbg, report) = apply_patch(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
        true, // test_height_at_origin
        false,
        true, // debug_frontier
    )
    .expect("apply_patch: tag tables fit within test fixtures");
    // 3x3 box around the origin cell = 9 cells; 1 selected (real) + 8 frontier.
    assert_eq!(dbg.get_num_of_atoms(), 9, "selected + 8 frontier tiles");
    assert_eq!(
        dbg.atoms_values().filter(|a| a.is_frozen()).count(),
        8,
        "the excluded neighbour tiles are flagged frozen"
    );
    // Report reflects the normal selection (no ghosts here -> zero), unchanged.
    assert_eq!(report.welded_ghosts, plain.1.welded_ghosts);
    assert_eq!(report.orphaned_ghosts, plain.1.orphaned_ghosts);
}

// ============================================================================
// 17. Origin vs target test height: for a slab offset from the lattice origin
//     along the normal, the origin-height plane (z=0) misses it and selects
//     nothing, while the target-derived height lands on the slab.
// ============================================================================

#[test]
fn origin_vs_target_test_height_for_offset_slab() {
    let lattice = cubic(4.0);
    // A thin target slab parked at z ~ 10, nowhere near the origin plane z=0.
    let mut target = AtomicStructure::new();
    target.add_atom(CARBON, DVec3::new(0.0, 0.0, 10.0));
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::new(0.0, 0.0, 12.0)); // reconstruction above the slab
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    // Bounds hug the slab (z in [8,13]); the origin plane z=0 is outside them.
    let bounds = box_bounds(DVec3::new(-1.0, -1.0, 8.0), DVec3::new(1.0, 1.0, 13.0));

    // Origin height (z=0) → projected atoms land at z=0, outside the region →
    // no cell selected; only the original target atom remains.
    let (origin_ver, _r1) = apply_patch(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
        true, // test_height_at_origin
        false,
        false,
    )
    .expect("apply_patch: tag tables fit within test fixtures");
    assert_eq!(
        origin_ver.get_num_of_atoms(),
        1,
        "origin-height plane (z=0) misses the offset slab → no tile placed"
    );

    // Target-derived height (z=10) → projected atoms land on the slab → selected.
    let (target_ver, _r2) = apply_patch(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
        false, // derive height from the target
        false,
        false,
    )
    .expect("apply_patch: tag tables fit within test fixtures");
    assert!(
        target_ver.get_num_of_atoms() > 1,
        "target-derived height (z=10) lands on the slab → tile placed"
    );
}

// ============================================================================
// 18. Frontier debug with an empty selection: falls back to the -1..+1 block
//     around the origin so the user still sees where the (rejected) tiles would
//     have gone, plus placed_cells == 0 (the "nothing tiled" signal).
// ============================================================================

#[test]
fn debug_frontier_shows_block_when_nothing_selected() {
    let lattice = cubic(4.0);
    // Offset target + origin height → zero cells selected.
    let mut target = AtomicStructure::new();
    target.add_atom(CARBON, DVec3::new(0.0, 0.0, 10.0));
    let mut tile = AtomicStructure::new();
    tile.add_atom(CARBON, DVec3::new(0.0, 0.0, 12.0));
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    let bounds = box_bounds(DVec3::new(-1.0, -1.0, 8.0), DVec3::new(1.0, 1.0, 13.0));

    let (dbg, report) = apply_patch(
        &target,
        &lattice,
        None,
        &bounds,
        &tile,
        &tiling,
        &empty_cut(),
        IVec3::ZERO,
        false,
        0.1,
        true,  // origin height → nothing selected for the offset target
        false, // debug_project
        true,  // debug_frontier
    )
    .expect("apply_patch: tag tables fit within test fixtures");

    assert_eq!(
        report.placed_cells, 0,
        "origin height selects nothing for the offset target"
    );
    // 3×3 = 9 frontier cells (the -1..+1 block), all frozen since none selected.
    assert_eq!(
        dbg.atoms_values().filter(|a| a.is_frozen()).count(),
        9,
        "the -1..+1 frontier block is shown so the user sees where tiles would go"
    );
}

// ============================================================================
// 19. Region membership epsilon: a projected test point a hair (0.05 Å) past the
//     region boundary is still admitted (within the 0.1 Å threshold), so a test
//     plane that lands *on* the boundary doesn't fail on floating-point sign.
// ============================================================================

#[test]
fn region_inclusion_admits_boundary_within_epsilon() {
    let lattice = cubic(4.0);
    let tiling = [IVec3::new(1, 0, 0)];
    // Region = a sphere of radius 5 at the origin (SDF = |p| − 5).
    let region = GeoNode::sphere(DVec3::ZERO, 5.0);
    // One interior atom whose projection (y,z → 0) lands at (5.05, 0, 0): 0.05 Å
    // *outside* the sphere — within the 0.1 Å membership epsilon. A strict `<= 0`
    // test would reject the origin cell ("boundary right at the test plane").
    let interior = vec![DVec3::new(5.05, 0.0, 7.0)];
    let bounds = box_bounds(DVec3::new(-8.0, -8.0, -2.0), DVec3::new(8.0, 8.0, 10.0));
    let free_dirs = vec![DVec3::Y, DVec3::Z];
    let center_depths = vec![0.0, 0.0]; // origin-height test plane

    let cells = select_patch_cells(
        IVec3::ZERO,
        &tiling,
        &lattice,
        Some(&region),
        &bounds,
        &interior,
        &free_dirs,
        &center_depths,
    );

    assert!(
        cells.iter().any(|c| c.offset == IVec3::ZERO),
        "a point 0.05 Å past the boundary is admitted by the membership epsilon"
    );
    // One cell over → projects to (9.05, 0, 0), SDF +4.05 ≫ 0.1 → still rejected.
    assert!(
        !cells.iter().any(|c| c.offset == IVec3::new(1, 0, 0)),
        "a point well outside is still rejected"
    );
}
