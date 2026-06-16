//! Tests for `weld_coincident_atoms` (Phase 1 of the surface-reconstruction
//! patch feature — see `doc/design_surface_patches.md` §3).

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{BOND_DOUBLE, BOND_SINGLE};
use rust_lib_flutter_cad::crystolecule::weld::weld_coincident_atoms;

const WELD_TOL: f64 = 0.1;

/// Returns the sorted list of (partner_id, bond_order) for an atom.
fn bonds_of(structure: &AtomicStructure, atom_id: u32) -> Vec<(u32, u8)> {
    let atom = structure.get_atom(atom_id).expect("atom exists");
    let mut out: Vec<(u32, u8)> = atom
        .bonds
        .iter()
        .map(|b| (b.other_atom_id(), b.bond_order()))
        .collect();
    out.sort();
    out
}

// ============================================================================
// 1. Two coincident atoms fuse; bond lists union; partner ids rewritten.
// ============================================================================

#[test]
fn two_coincident_atoms_fuse_and_union_bonds() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 1 (survivor)
    let p1 = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0)); // id 2
    let b = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 3 (coincident with a)
    let p2 = s.add_atom(6, DVec3::new(-5.0, 0.0, 0.0)); // id 4
    s.add_bond(a, p1, BOND_SINGLE);
    s.add_bond(b, p2, BOND_SINGLE);

    weld_coincident_atoms(&mut s, WELD_TOL);

    // a and b collapse into one survivor (a, the lowest id); p1, p2 remain.
    assert_eq!(s.get_num_of_atoms(), 3);
    assert_eq!(s.get_num_of_bonds(), 2);
    assert!(s.get_atom(b).is_none(), "b should have been welded away");

    // The survivor inherits both bonds; b's partner id was rewritten to a.
    assert_eq!(bonds_of(&s, a), vec![(p1, BOND_SINGLE), (p2, BOND_SINGLE)]);
    assert_eq!(bonds_of(&s, p1), vec![(a, BOND_SINGLE)]);
    assert_eq!(bonds_of(&s, p2), vec![(a, BOND_SINGLE)]);
}

// ============================================================================
// 2. Atoms farther apart than tolerance do NOT merge.
// ============================================================================

#[test]
fn atoms_beyond_tolerance_do_not_merge() {
    let mut s = AtomicStructure::new();
    // 0.5 Å apart — well above the 0.1 Å tolerance, well below any bond length.
    s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    s.add_atom(6, DVec3::new(0.5, 0.0, 0.0));

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert_eq!(
        s.get_num_of_atoms(),
        2,
        "distinct sites must not over-merge"
    );
}

// ============================================================================
// 3. Patch-ghost survivorship.
// ============================================================================

#[test]
fn real_and_patch_ghost_yields_real_survivor() {
    let mut s = AtomicStructure::new();
    // Make the lowest-id atom the patch-ghost so we prove the flag is cleared
    // even when the survivor itself started out as a ghost.
    let ghost = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 1 (survivor)
    let real = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 2
    s.set_atom_patch_ghost(ghost, true);

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert_eq!(s.get_num_of_atoms(), 1);
    assert!(
        !s.get_atom(ghost).unwrap().is_patch_ghost(),
        "a real atom in the cluster must make the survivor real"
    );
    let _ = real;
}

#[test]
fn all_patch_ghost_keeps_patch_ghost_survivor() {
    let mut s = AtomicStructure::new();
    let g1 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let g2 = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    s.set_atom_patch_ghost(g1, true);
    s.set_atom_patch_ghost(g2, true);

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert_eq!(s.get_num_of_atoms(), 1);
    assert!(
        s.get_atom(g1).unwrap().is_patch_ghost(),
        "a cluster of only patch-ghosts stays a patch-ghost"
    );
}

// ============================================================================
// 4. Duplicate bond dedups; conflicting bond order panics.
// ============================================================================

#[test]
fn duplicate_bond_to_same_partner_dedups() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 1 (survivor)
    let b = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 2 (coincident)
    let p = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0)); // id 3
    s.add_bond(a, p, BOND_SINGLE);
    s.add_bond(b, p, BOND_SINGLE);

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert_eq!(s.get_num_of_atoms(), 2);
    assert_eq!(
        s.get_num_of_bonds(),
        1,
        "the two identical bonds to p must collapse into one"
    );
    assert_eq!(bonds_of(&s, a), vec![(p, BOND_SINGLE)]);
}

#[test]
#[should_panic(expected = "conflicting bond order")]
fn conflicting_bond_order_panics() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let b = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    let p = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0));
    s.add_bond(a, p, BOND_SINGLE);
    s.add_bond(b, p, BOND_DOUBLE);

    weld_coincident_atoms(&mut s, WELD_TOL);
}

// ============================================================================
// 5. Bulk-bond inheritance: a collar welds onto a bulk atom.
// ============================================================================

#[test]
fn collar_inherits_bulk_bonds() {
    let mut s = AtomicStructure::new();
    let bulk = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 1 (survivor)
    let bulk_neighbor = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0)); // id 2 (outward bulk bond)
    let collar = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 3 (coincident patch-ghost)
    let interior = s.add_atom(6, DVec3::new(-5.0, 0.0, 0.0)); // id 4 (collar's inward bond)
    s.add_bond(bulk, bulk_neighbor, BOND_SINGLE);
    s.add_bond(collar, interior, BOND_SINGLE);
    s.set_atom_patch_ghost(collar, true);

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert_eq!(s.get_num_of_atoms(), 3);
    // Survivor is real (bulk was real) and carries BOTH the bulk's outward bond
    // and the collar's inward bond.
    assert!(!s.get_atom(bulk).unwrap().is_patch_ghost());
    assert_eq!(
        bonds_of(&s, bulk),
        vec![(bulk_neighbor, BOND_SINGLE), (interior, BOND_SINGLE)]
    );
}

// ============================================================================
// 6. Three-way coincident cluster collapses to one survivor.
// ============================================================================

#[test]
fn three_way_cluster_collapses() {
    let mut s = AtomicStructure::new();
    let a = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 1 (survivor)
    let b = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 2
    let c = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // id 3
    let pa = s.add_atom(6, DVec3::new(5.0, 0.0, 0.0)); // id 4
    let pb = s.add_atom(6, DVec3::new(0.0, 5.0, 0.0)); // id 5
    let pc = s.add_atom(6, DVec3::new(0.0, 0.0, 5.0)); // id 6
    s.add_bond(a, pa, BOND_SINGLE);
    s.add_bond(b, pb, BOND_SINGLE);
    s.add_bond(c, pc, BOND_SINGLE);

    weld_coincident_atoms(&mut s, WELD_TOL);

    // a, b, c → one survivor; the three partners remain.
    assert_eq!(s.get_num_of_atoms(), 4);
    assert_eq!(s.get_num_of_bonds(), 3);
    assert!(s.get_atom(b).is_none());
    assert!(s.get_atom(c).is_none());
    assert_eq!(
        bonds_of(&s, a),
        vec![(pa, BOND_SINGLE), (pb, BOND_SINGLE), (pc, BOND_SINGLE)]
    );
}

// ============================================================================
// 7. Flag accessor round-trip: bit 6 toggles independently of bits 0–5.
// ============================================================================

#[test]
fn patch_ghost_flag_toggles_bit_6_only() {
    let mut s = AtomicStructure::new();
    let id = s.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    // Set some other flags first.
    s.set_atom_selected(id, true);
    s.set_atom_frozen(id, true);

    let before = s.get_atom(id).unwrap().flags;
    assert!(!s.get_atom(id).unwrap().is_patch_ghost());

    s.set_atom_patch_ghost(id, true);
    let after_set = s.get_atom(id).unwrap().flags;
    assert!(s.get_atom(id).unwrap().is_patch_ghost());
    // Only bit 6 changed.
    assert_eq!(after_set ^ before, 1 << 6);
    // Unrelated flags are untouched.
    assert!(s.get_atom(id).unwrap().is_selected());
    assert!(s.get_atom(id).unwrap().is_frozen());

    s.set_atom_patch_ghost(id, false);
    let after_clear = s.get_atom(id).unwrap().flags;
    assert!(!s.get_atom(id).unwrap().is_patch_ghost());
    assert_eq!(
        after_clear, before,
        "clearing bit 6 restores the original flags"
    );
}
