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
    apply_patch, select_patch_cells,
};
use rust_lib_flutter_cad::util::daabox::DAABox;

const CARBON: i16 = 6;
const SINGLE: u8 = 1;

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

    let (result, report) = apply_patch(
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

    let (result, _report) = apply_patch(
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

    let (result, _report) = apply_patch(
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

    let (result, report) = apply_patch(
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

#[test]
fn containment_2d_normal_is_free() {
    let lattice = cubic(4.0);
    let tiling = [IVec3::new(1, 0, 0), IVec3::new(0, 1, 0)];
    // The footprint sits at z=0; the region is far away in z. Because z is the
    // free (normal) axis, cells are still selected by their x–y footprint.
    let bounds = box_bounds(DVec3::new(-0.5, -0.5, 20.0), DVec3::new(8.5, 8.5, 30.0));

    let cells = select_patch_cells(IVec3::ZERO, &tiling, &lattice, None, &bounds);

    assert_eq!(cells.len(), 4, "the 2×2 block whose x–y footprint fits");
    for c in [
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(1, 1, 0),
    ] {
        assert!(cells.contains(&c), "cell {c:?} should be placed");
    }
}

#[test]
fn containment_1d_transverse_free() {
    let lattice = cubic(4.0);
    let tiling = [IVec3::new(1, 0, 0)];
    // Both transverse axes (y, z) are free; only the x footprint gates.
    let bounds = box_bounds(DVec3::new(-0.5, 20.0, 20.0), DVec3::new(8.5, 30.0, 30.0));

    let cells = select_patch_cells(IVec3::ZERO, &tiling, &lattice, None, &bounds);

    assert_eq!(cells.len(), 2);
    assert!(cells.contains(&IVec3::new(0, 0, 0)));
    assert!(cells.contains(&IVec3::new(1, 0, 0)));
}

#[test]
fn containment_3d_has_no_free_axis() {
    let lattice = cubic(4.0);
    let tiling = [
        IVec3::new(1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, 0, 1),
    ];

    // Tight box around the origin cell → exactly one cell.
    let tight = box_bounds(DVec3::new(-0.5, -0.5, -0.5), DVec3::new(4.5, 4.5, 4.5));
    let cells = select_patch_cells(IVec3::ZERO, &tiling, &lattice, None, &tight);
    assert_eq!(cells, vec![IVec3::ZERO]);

    // With no free axis, z is a periodic direction requiring containment: a
    // region offset in z does NOT place the origin-plane cell (unlike the 2D
    // case above, where z is free and the origin cell is placed regardless).
    let far_z = box_bounds(DVec3::new(-0.5, -0.5, 20.0), DVec3::new(4.5, 4.5, 30.0));
    let far = select_patch_cells(IVec3::ZERO, &tiling, &lattice, None, &far_z);
    assert!(
        !far.contains(&IVec3::ZERO),
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
    target.add_atom(CARBON, DVec3::new(6.0, 1.0, 1.0)); // inside cut @ cell +1 (not placed)

    // A purely-removing patch: empty tile, cut covering one cell's surface.
    let tile = AtomicStructure::new();
    let cut = GeoNode::sphere(DVec3::new(2.0, 1.0, 1.0), 1.5);

    let tiling = [IVec3::new(1, 0, 0)];
    // Only cell 0 is selected.
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let (result, _report) = apply_patch(
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

    assert_eq!(result.get_num_of_atoms(), 1);
    assert!(
        find_atom_at(&result, DVec3::new(6.0, 1.0, 1.0), 1e-6).is_some(),
        "the atom in the un-reconstructed cell survives"
    );
    assert!(
        find_atom_at(&result, DVec3::new(2.0, 1.0, 1.0), 1e-6).is_none(),
        "the atom in the reconstructed cell is cut"
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
        apply_patch(
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
    let bounds = box_bounds(DVec3::new(-0.5, -2.0, -2.0), DVec3::new(4.5, 2.0, 2.0));

    let (result, _report) = apply_patch(
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

    let (result, report) = apply_patch(
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

    let (_result, report) = apply_patch(
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

    let (_result, report) = apply_patch(
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

    let (_result, report) = apply_patch(
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
