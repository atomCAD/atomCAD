//! Tests for atom tags in the `atom_edit` node (design_atom_tags.md Phase 4):
//! `add_tag_recorded` / `remove_tag_recorded` / `clear_tags_recorded`, name-based
//! undo/redo deltas (robust to slot reclamation), and base-atom promotion that
//! carries the base atom's tag names onto the new diff override.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::UNCHANGED_ATOMIC_NUMBER;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, BaseAtomPromotionInfo, with_atom_edit_undo,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashSet;

// =============================================================================
// Helpers
// =============================================================================

fn setup_atom_edit() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    let node_id = designer.add_node("atom_edit", DVec2::ZERO);
    designer.select_node(node_id);
    designer.undo_stack.clear();
    designer
}

fn get_data_mut(designer: &mut StructureDesigner) -> &mut AtomEditData {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node_id = network.active_node_id.unwrap();
    let data = network.get_node_network_data_mut(node_id).unwrap();
    data.as_any_mut().downcast_mut::<AtomEditData>().unwrap()
}

/// Sorted owned tag names of a diff atom (bit order is not stable across
/// reclamation, so tests compare sets).
fn tags_of(designer: &mut StructureDesigner, atom_id: u32) -> Vec<String> {
    let mut t: Vec<String> = get_data_mut(designer)
        .diff
        .atom_tags(atom_id)
        .iter()
        .map(|s| s.to_string())
        .collect();
    t.sort();
    t
}

fn tag_set(designer: &mut StructureDesigner, atom_id: u32) -> HashSet<String> {
    get_data_mut(designer)
        .diff
        .atom_tags(atom_id)
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// =============================================================================
// Recorded-method deltas
// =============================================================================

#[test]
fn add_tag_recorded_produces_name_delta() {
    let mut data = AtomEditData::new();
    let id = data.add_atom_to_diff(6, DVec3::ZERO);
    data.begin_recording();
    data.add_tag_recorded(id, "surface").unwrap();
    let rec = data.end_recording().unwrap();

    assert_eq!(rec.atom_deltas.len(), 1);
    let d = &rec.atom_deltas[0];
    assert_eq!(d.atom_id, id);
    // The delta records tag *names*, not raw bits.
    assert!(d.before.as_ref().unwrap().tags.is_empty());
    assert_eq!(d.after.as_ref().unwrap().tags, vec!["surface".to_string()]);
    // Position / element unchanged by a tag edit.
    assert_eq!(d.before.as_ref().unwrap().atomic_number, 6);
    assert_eq!(d.after.as_ref().unwrap().atomic_number, 6);
    assert!(data.diff.atom_has_tag(id, "surface"));
}

#[test]
fn tag_names_are_trimmed_and_empty_rejected() {
    let mut data = AtomEditData::new();
    let id = data.add_atom_to_diff(6, DVec3::ZERO);
    data.add_tag_recorded(id, "  surface  ").unwrap();
    assert!(data.diff.atom_has_tag(id, "surface"));
    // Empty / whitespace-only name is rejected (edit not applied).
    assert!(data.add_tag_recorded(id, "   ").is_err());
}

#[test]
fn remove_and_clear_no_op_when_unchanged() {
    let mut data = AtomEditData::new();
    let id = data.add_atom_to_diff(6, DVec3::ZERO);
    data.begin_recording();
    // Removing an absent tag and clearing an untagged atom are no-ops → no delta.
    data.remove_tag_recorded(id, "nope");
    data.clear_tags_recorded(id);
    let rec = data.end_recording().unwrap();
    assert!(rec.atom_deltas.is_empty());
}

// =============================================================================
// Undo / redo of tag edits on diff atoms
// =============================================================================

#[test]
fn tag_diff_atom_undoes_and_redoes() {
    let mut designer = setup_atom_edit();
    let id = get_data_mut(&mut designer).add_atom_to_diff(6, DVec3::ZERO);

    with_atom_edit_undo(&mut designer, "Tag", |sd| {
        get_data_mut(sd).add_tag_recorded(id, "surface").unwrap();
    });
    assert_eq!(tags_of(&mut designer, id), vec!["surface".to_string()]);

    designer.undo();
    assert!(tags_of(&mut designer, id).is_empty());
    // The atom itself survives — only its tag set was reverted.
    assert!(get_data_mut(&mut designer).diff.get_atom(id).is_some());

    designer.redo();
    assert_eq!(tags_of(&mut designer, id), vec!["surface".to_string()]);
}

#[test]
fn untag_and_clear_undo_redo() {
    let mut designer = setup_atom_edit();
    let id = get_data_mut(&mut designer).add_atom_to_diff(6, DVec3::ZERO);
    // Seed two tags (committed, separate step).
    with_atom_edit_undo(&mut designer, "Tag two", |sd| {
        let data = get_data_mut(sd);
        data.add_tag_recorded(id, "a").unwrap();
        data.add_tag_recorded(id, "b").unwrap();
    });
    assert_eq!(
        tags_of(&mut designer, id),
        vec!["a".to_string(), "b".to_string()]
    );

    // Untag one.
    with_atom_edit_undo(&mut designer, "Untag a", |sd| {
        get_data_mut(sd).remove_tag_recorded(id, "a");
    });
    assert_eq!(tags_of(&mut designer, id), vec!["b".to_string()]);
    designer.undo();
    assert_eq!(
        tags_of(&mut designer, id),
        vec!["a".to_string(), "b".to_string()]
    );
    designer.redo();
    assert_eq!(tags_of(&mut designer, id), vec!["b".to_string()]);

    // Clear all (empty-name = all-tags analog at the data layer).
    with_atom_edit_undo(&mut designer, "Clear", |sd| {
        get_data_mut(sd).clear_tags_recorded(id);
    });
    assert!(tags_of(&mut designer, id).is_empty());
    designer.undo();
    assert_eq!(tags_of(&mut designer, id), vec!["b".to_string()]);
}

/// The critical name-vs-bits test: fill all 32 slots, free one (dead slot),
/// then intern a new name that *reclaims that exact bit*. Undo/redo must restore
/// the correct **names** on the atom — a raw-bit delta would restore the wrong
/// tag once the bit's meaning was reassigned.
#[test]
fn reclamation_undo_stress_restores_names_not_bits() {
    let mut designer = setup_atom_edit();
    let id = get_data_mut(&mut designer).add_atom_to_diff(6, DVec3::ZERO);

    // Step 1: fill all 32 tag slots on the atom.
    with_atom_edit_undo(&mut designer, "Tag 32", |sd| {
        let data = get_data_mut(sd);
        for i in 0..32 {
            data.add_tag_recorded(id, &format!("t{i}")).unwrap();
        }
    });
    assert_eq!(tag_set(&mut designer, id).len(), 32);

    // Step 2: untag t5 — its bit goes dead but the table stays full.
    with_atom_edit_undo(&mut designer, "Untag t5", |sd| {
        get_data_mut(sd).remove_tag_recorded(id, "t5");
    });

    // Step 3: a new name reclaims the dead slot (table is full → reclamation).
    with_atom_edit_undo(&mut designer, "Tag reclaimer", |sd| {
        get_data_mut(sd).add_tag_recorded(id, "reclaimer").unwrap();
    });
    let after = tag_set(&mut designer, id);
    assert!(after.contains("reclaimer"));
    assert!(!after.contains("t5"));
    assert!(after.contains("t0") && after.contains("t31"));
    assert_eq!(after.len(), 32);

    // Undo step 3: reclaimer gone; t5 still gone.
    designer.undo();
    let s = tag_set(&mut designer, id);
    assert!(!s.contains("reclaimer"));
    assert!(!s.contains("t5"));
    assert_eq!(s.len(), 31);

    // Undo step 2: t5 restored by *name* (re-reclaiming the now-dead slot).
    designer.undo();
    let s = tag_set(&mut designer, id);
    assert!(s.contains("t5"));
    assert!(!s.contains("reclaimer"));
    assert_eq!(s.len(), 32);

    // Undo step 1: all tags gone.
    designer.undo();
    assert!(tag_set(&mut designer, id).is_empty());

    // Redo everything → back to the reclaimed state, names intact.
    designer.redo();
    designer.redo();
    designer.redo();
    let f = tag_set(&mut designer, id);
    assert!(f.contains("reclaimer"));
    assert!(!f.contains("t5"));
    assert_eq!(f.len(), 32);
}

// =============================================================================
// Base-atom promotion carries tags (full override, §Diff semantics)
// =============================================================================

#[test]
fn promote_base_atom_metadata_carries_flags_and_tags() {
    let mut data = AtomEditData::new();
    let id = data.add_atom_to_diff(6, DVec3::ZERO);
    data.begin_recording();
    // frozen flag (bit 2) + two upstream tags.
    data.promote_base_atom_metadata(1 << 2, &["surface".to_string(), "active".to_string()], id);
    let rec = data.end_recording().unwrap();

    assert!(data.diff.get_atom(id).unwrap().is_frozen());
    assert!(data.diff.atom_has_tag(id, "surface"));
    assert!(data.diff.atom_has_tag(id, "active"));
    // Flag + tag copies were recorded (so the promotion is undoable).
    assert!(!rec.atom_deltas.is_empty());
}

/// Replacing a base atom that is already referenced by a bond-tool UNCHANGED
/// marker promotes it to a real (non-marker) override that carries the base
/// atom's pre-existing tags — and no tag ever lingers on a marker.
#[test]
fn replace_promotes_unchanged_marker_carrying_base_tags() {
    let mut data = AtomEditData::new();

    // A bond tool left an UNCHANGED marker at the base atom's position.
    let marker_id = data
        .diff
        .add_atom(UNCHANGED_ATOMIC_NUMBER, DVec3::new(1.0, 0.0, 0.0));
    // The marker itself carries no tags (invariant).
    assert!(data.diff.atom_tags(marker_id).is_empty());

    data.selection.selected_base_atoms.insert(42);
    data.apply_replace(
        14, // Silicon
        &[BaseAtomPromotionInfo {
            base_id: 42,
            atomic_number: 6,
            position: DVec3::new(1.0, 0.0, 0.0),
            existing_diff_id: Some(marker_id),
            flags: 0,
            tags: vec!["surface".to_string()],
        }],
    );

    let atom = data.diff.get_atom(marker_id).unwrap();
    // Promoted to a real element (no longer an UNCHANGED marker).
    assert_eq!(atom.atomic_number, 14);
    assert_ne!(atom.atomic_number, UNCHANGED_ATOMIC_NUMBER);
    // The base atom's upstream tag was carried onto the override.
    assert!(data.diff.atom_has_tag(marker_id, "surface"));
}

#[test]
fn transform_promotes_base_atom_carrying_tags() {
    use rust_lib_flutter_cad::util::transform::Transform;

    let mut data = AtomEditData::new();
    data.selection.selected_base_atoms.insert(7);
    data.selection.selection_transform = Some(Transform::default());

    let relative = Transform::new(DVec3::new(1.0, 0.0, 0.0), glam::f64::DQuat::IDENTITY);
    data.apply_transform(
        &relative,
        &[BaseAtomPromotionInfo {
            base_id: 7,
            atomic_number: 6,
            position: DVec3::ZERO,
            existing_diff_id: None,
            flags: 0,
            tags: vec!["frame".to_string()],
        }],
    );

    // A fresh diff atom was created; it carries the base tag.
    let (new_id, atom) = data.diff.iter_atoms().next().unwrap();
    assert!((atom.position - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-10);
    assert!(data.diff.atom_has_tag(*new_id, "frame"));
}
