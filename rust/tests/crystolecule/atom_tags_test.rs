//! Phase 1 tests for per-atom **tags** — core storage, interning, slot
//! reclamation, cross-structure merge remapping, weld unioning, and the
//! `Atom` struct-size invariant. See `doc/design_atom_tags.md` §Phase 1.

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    Atom, AtomicStructure, MAX_TAGS, TagError,
};
use rust_lib_flutter_cad::crystolecule::weld::weld_coincident_atoms;

const WELD_TOL: f64 = 0.1;

/// Adds a carbon at `pos` and returns its id.
fn carbon(s: &mut AtomicStructure, pos: DVec3) -> u32 {
    s.add_atom(6, pos)
}

// ============================================================================
// Basic accessors: add / remove / has / clear, bit order, derived query,
// trimming, empty rejection.
// ============================================================================

#[test]
fn add_has_remove_clear_roundtrip() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);

    assert!(!s.atom_has_tag(a, "surface"));
    s.add_atom_tag(a, "surface").expect("intern ok");
    s.add_atom_tag(a, "active-site").expect("intern ok");
    assert!(s.atom_has_tag(a, "surface"));
    assert!(s.atom_has_tag(a, "active-site"));

    // Re-tagging is idempotent (no error, still one bit).
    s.add_atom_tag(a, "surface").expect("re-tag ok");
    assert_eq!(s.atom_tags(a).len(), 2);

    s.remove_atom_tag(a, "surface");
    assert!(!s.atom_has_tag(a, "surface"));
    assert!(s.atom_has_tag(a, "active-site"));

    // Removing an absent name / tag is a no-op.
    s.remove_atom_tag(a, "surface");
    s.remove_atom_tag(a, "never-interned");
    assert!(s.atom_has_tag(a, "active-site"));

    s.clear_atom_tags(a);
    assert!(s.atom_tags(a).is_empty());
}

#[test]
fn atom_tags_returned_in_bit_order() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);
    // Interning order fixes bit order: first→bit0, second→bit1, ...
    s.add_atom_tag(a, "zebra").unwrap();
    s.add_atom_tag(a, "apple").unwrap();
    s.add_atom_tag(a, "mango").unwrap();
    // Not alphabetical — bit order (= interning order).
    assert_eq!(s.atom_tags(a), vec!["zebra", "apple", "mango"]);
}

#[test]
fn atoms_with_tag_finds_exactly_the_tagged_ids() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::new(0.0, 0.0, 0.0));
    let b = carbon(&mut s, DVec3::new(2.0, 0.0, 0.0));
    let c = carbon(&mut s, DVec3::new(4.0, 0.0, 0.0));
    s.add_atom_tag(a, "keep").unwrap();
    s.add_atom_tag(c, "keep").unwrap();

    let mut found = s.atoms_with_tag("keep");
    found.sort();
    assert_eq!(found, vec![a, c]);
    assert!(s.atoms_with_tag("nope").is_empty());
    let _ = b;
}

#[test]
fn tag_names_are_trimmed_and_matched_exactly() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);
    s.add_atom_tag(a, "  surface  ").unwrap();
    // Stored trimmed; queries trim too, so all of these hit the same tag.
    assert!(s.atom_has_tag(a, "surface"));
    assert!(s.atom_has_tag(a, "  surface"));
    assert_eq!(s.tag_names(), &["surface".to_string()]);
    // Case-sensitive: a different-case name is a different tag.
    assert!(!s.atom_has_tag(a, "Surface"));
}

#[test]
fn empty_or_whitespace_name_is_rejected() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);
    assert_eq!(s.add_atom_tag(a, ""), Err(TagError::EmptyName));
    assert_eq!(s.add_atom_tag(a, "   "), Err(TagError::EmptyName));
    assert_eq!(s.intern_tag("\t \n"), Err(TagError::EmptyName));
    assert!(s.tag_names().is_empty());
}

// ============================================================================
// Interning: idempotent, the 33rd live name errors, dead slots reclaim.
// ============================================================================

#[test]
fn interning_is_idempotent() {
    let mut s = AtomicStructure::new();
    let i0 = s.intern_tag("foo").unwrap();
    let i1 = s.intern_tag("foo").unwrap();
    let i2 = s.intern_tag(" foo ").unwrap(); // trims to the same name
    assert_eq!(i0, i1);
    assert_eq!(i0, i2);
    assert_eq!(s.tag_index("foo"), Some(i0));
    assert_eq!(s.tag_index("bar"), None);
}

#[test]
fn thirty_third_live_name_errors() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);
    // 32 distinct live names, all carried by `a`.
    for i in 0..MAX_TAGS {
        s.add_atom_tag(a, &format!("t{i}")).expect("32 fit");
    }
    assert_eq!(s.tag_names().len(), MAX_TAGS);
    // The 33rd distinct name has nowhere to go — every slot is live.
    assert_eq!(s.add_atom_tag(a, "overflow"), Err(TagError::LimitReached));
    assert_eq!(s.intern_tag("overflow"), Err(TagError::LimitReached));
    // An already-interned name still succeeds even at the limit.
    assert!(s.add_atom_tag(a, "t5").is_ok());
}

#[test]
fn dead_slot_is_reclaimed_and_old_name_forgotten() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);
    for i in 0..MAX_TAGS {
        s.add_atom_tag(a, &format!("t{i}")).expect("32 fit");
    }
    // Free bit 5: remove it from the only carrier → slot 5 goes dead, but its
    // name is still in the table.
    s.remove_atom_tag(a, "t5");
    assert_eq!(s.tag_index("t5"), Some(5));

    // Interning a new name now reclaims slot 5 instead of erroring.
    let reclaimed = s.intern_tag("fresh").expect("dead slot reclaimed");
    assert_eq!(reclaimed, 5);
    // The reclaimed slot's old name is gone; the new name owns bit 5.
    assert_eq!(s.tag_index("t5"), None);
    assert_eq!(s.tag_index("fresh"), Some(5));
    // Reclamation never touched any atom's mask: `a` carries neither now.
    assert!(!s.atom_has_tag(a, "t5"));
    assert!(!s.atom_has_tag(a, "fresh"));
}

// ============================================================================
// add_atomic_structure: name-level merge under the target's table.
// ============================================================================

#[test]
fn merge_disjoint_tables_lands_tags_on_remapped_ids() {
    let mut target = AtomicStructure::new();
    let ta = carbon(&mut target, DVec3::new(0.0, 0.0, 0.0));
    target.add_atom_tag(ta, "A").unwrap();

    let mut other = AtomicStructure::new();
    let oa = carbon(&mut other, DVec3::new(10.0, 0.0, 0.0));
    other.add_atom_tag(oa, "B").unwrap();

    let id_map = target.add_atomic_structure(&other).expect("merge ok");
    let new_oa = id_map[&oa];

    assert!(target.atom_has_tag(ta, "A"));
    assert!(target.atom_has_tag(new_oa, "B"));
    assert!(!target.atom_has_tag(new_oa, "A"));
    assert!(!target.atom_has_tag(ta, "B"));
}

#[test]
fn merge_translates_colliding_bits_by_name() {
    // Target: "X" at bit 0, "A" at bit 1. Other: "A" at bit 0.
    // The merge must land other's "A" on the target's bit-1 slot, by name.
    let mut target = AtomicStructure::new();
    let tx = carbon(&mut target, DVec3::new(0.0, 0.0, 0.0));
    target.add_atom_tag(tx, "X").unwrap(); // bit 0
    target.intern_tag("A").unwrap(); // bit 1 (not carried by tx)
    assert_eq!(target.tag_index("A"), Some(1));

    let mut other = AtomicStructure::new();
    let oa = carbon(&mut other, DVec3::new(9.0, 0.0, 0.0));
    other.add_atom_tag(oa, "A").unwrap(); // bit 0 in `other`
    assert_eq!(other.tag_index("A"), Some(0));

    let id_map = target.add_atomic_structure(&other).expect("merge ok");
    let new_oa = id_map[&oa];

    // By name the tag survived; by raw bits it moved from 0 → 1.
    assert!(target.atom_has_tag(new_oa, "A"));
    assert_eq!(target.atom_tag_bits(new_oa), 1u32 << 1);
    assert_eq!(target.atom_tags(new_oa), vec!["A"]);
}

#[test]
fn merge_over_thirty_two_live_names_errors() {
    let mut target = AtomicStructure::new();
    let ta = carbon(&mut target, DVec3::ZERO);
    for i in 0..MAX_TAGS {
        target.add_atom_tag(ta, &format!("t{i}")).unwrap();
    }

    let mut other = AtomicStructure::new();
    let oa = carbon(&mut other, DVec3::new(10.0, 0.0, 0.0));
    other.add_atom_tag(oa, "one-too-many").unwrap();

    assert_eq!(
        target.add_atomic_structure(&other),
        Err(TagError::LimitReached)
    );
}

#[test]
fn merge_identical_tables_uses_identity_and_preserves_names() {
    // Both tables are exactly ["A"] → the fast path (identity remap) applies.
    let mut target = AtomicStructure::new();
    let ta = carbon(&mut target, DVec3::new(0.0, 0.0, 0.0));
    target.add_atom_tag(ta, "A").unwrap();

    let mut other = AtomicStructure::new();
    let oa = carbon(&mut other, DVec3::new(10.0, 0.0, 0.0));
    other.add_atom_tag(oa, "A").unwrap();

    let id_map = target.add_atomic_structure(&other).expect("merge ok");
    let new_oa = id_map[&oa];
    assert!(target.atom_has_tag(new_oa, "A"));
    assert!(target.atom_has_tag(ta, "A"));
    assert_eq!(target.tag_names(), &["A".to_string()]);
}

// ============================================================================
// weld_coincident_atoms: survivor's mask is the OR of the fused masks.
// ============================================================================

#[test]
fn weld_ors_tags_of_fused_atoms() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::new(0.0, 0.0, 0.0)); // survivor (lowest id)
    let b = carbon(&mut s, DVec3::new(0.0, 0.0, 0.0)); // coincident with a
    s.add_atom_tag(a, "left").unwrap();
    s.add_atom_tag(b, "right").unwrap();

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert!(s.get_atom(b).is_none(), "b welded away");
    assert!(s.atom_has_tag(a, "left"));
    assert!(s.atom_has_tag(a, "right"));
    let mut names = s.atom_tags(a);
    names.sort();
    assert_eq!(names, vec!["left", "right"]);
}

#[test]
fn weld_tagged_with_untagged_keeps_the_tag() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::new(0.0, 0.0, 0.0)); // survivor, untagged
    let b = carbon(&mut s, DVec3::new(0.0, 0.0, 0.0)); // tagged, coincident
    s.add_atom_tag(b, "keeper").unwrap();

    weld_coincident_atoms(&mut s, WELD_TOL);

    assert!(s.get_atom(b).is_none());
    assert!(s.atom_has_tag(a, "keeper"));
}

// ============================================================================
// Maintenance: clone preserves tags; delete leaves others' tags intact.
// ============================================================================

#[test]
fn clone_preserves_tags() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::ZERO);
    s.add_atom_tag(a, "surface").unwrap();
    s.add_atom_tag(a, "active").unwrap();

    let cloned = s.clone();
    assert!(cloned.atom_has_tag(a, "surface"));
    assert!(cloned.atom_has_tag(a, "active"));
    assert_eq!(cloned.tag_names(), s.tag_names());
}

#[test]
fn deleting_a_tagged_atom_leaves_other_tags_intact() {
    let mut s = AtomicStructure::new();
    let a = carbon(&mut s, DVec3::new(0.0, 0.0, 0.0));
    let b = carbon(&mut s, DVec3::new(2.0, 0.0, 0.0));
    s.add_atom_tag(a, "gone").unwrap();
    s.add_atom_tag(b, "stays").unwrap();

    s.delete_atom(a);

    assert!(s.get_atom(a).is_none());
    assert!(s.atom_has_tag(b, "stays"));
    // The name table is unaffected by deletion (no external map to maintain).
    assert_eq!(s.tag_index("stays"), Some(1));
}

// ============================================================================
// Struct-size invariant — locks the "tags cost zero extra memory" claim.
// ============================================================================

#[test]
fn atom_is_still_one_cache_line() {
    assert_eq!(
        std::mem::size_of::<Atom>(),
        64,
        "tag_bits must fit in Atom's existing padding — Atom stays 64 bytes"
    );
}
