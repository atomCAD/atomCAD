//! `StructureDesigner::rename_node` — the GUI's write path into `custom_name`
//! (`doc/design_node_names_in_ui.md` Phase 1, D3).
//!
//! The contract these tests pin: the name the GUI writes is the name the text
//! format prints and the AI reads back, the rename is an id-keyed relabel with
//! no structural consequence, and every rejection leaves the stored name alone.

use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;
use glam::f64::DVec2;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn text_of(designer: &StructureDesigner, name: &str) -> String {
    let network = designer.node_type_registry.node_networks.get(name).unwrap();
    serialize_network(network, &designer.node_type_registry, Some(name))
}

fn name_of(designer: &StructureDesigner, scope_path: &[u64], node_id: u64) -> String {
    designer
        .get_scope_network(scope_path)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .custom_name
        .clone()
        .unwrap()
}

/// main: `float` wired into `sphere.radius` (pin 1), so the sphere's argument spells
/// the float's name and a rename has to move both sides at once.
fn setup_wired_pair() -> (StructureDesigner, u64, u64) {
    let mut designer = setup_designer_with_network("main");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 1);
    (designer, float_id, sphere_id)
}

// ===== The contract with the AI =====

#[test]
fn rename_moves_the_statement_and_every_reference_and_replace_is_a_no_op() {
    let (mut designer, float_id, _sphere_id) = setup_wired_pair();
    let old_name = name_of(&designer, &[], float_id);
    let before = text_of(&designer, "main");
    assert!(
        before.contains(&format!("{old_name} = float")),
        "precondition: the float is named `{old_name}` in the text:\n{before}"
    );

    designer.rename_node(&[], float_id, "chassis").unwrap();

    let after = text_of(&designer, "main");
    assert!(
        after.contains("chassis = float"),
        "the renamed node's own statement:\n{after}"
    );
    assert!(
        !after.contains(&old_name),
        "the old name must be gone from the text entirely:\n{after}"
    );
    // The downstream `sphere` references the node by name, so its argument had
    // to follow the rename: one statement name + one reference.
    assert_eq!(
        after.matches("chassis").count(),
        2,
        "one statement name + one downstream reference:\n{after}"
    );

    // A `--replace` of that text is a no-op: the wire survived the rename by
    // id, and the text the AI reads round-trips.
    let outcome = designer.ai_text_edit(&after, true);
    assert!(
        outcome.result.success,
        "--replace of the renamed text failed: {:?}",
        outcome.result.errors
    );
    assert_eq!(text_of(&designer, "main"), after);
}

/// A name the text format must backtick-quote is a legal node name (the
/// relaxed-name rules), so the strip must accept it and `query` → `--replace`
/// must still round-trip.
#[test]
fn a_name_the_text_format_must_quote_round_trips() {
    let (mut designer, float_id, _) = setup_wired_pair();
    designer.rename_node(&[], float_id, "x.shape").unwrap();

    let text = text_of(&designer, "main");
    assert!(
        text.contains("`x.shape` = float"),
        "a dotted name must be written quoted:\n{text}"
    );

    let outcome = designer.ai_text_edit(&text, true);
    assert!(
        outcome.result.success,
        "--replace failed: {:?}",
        outcome.result.errors
    );
    assert_eq!(text_of(&designer, "main"), text);
}

// ===== Rejections =====

#[test]
fn rejects_invalid_names_without_touching_the_stored_name() {
    let (mut designer, float_id, _) = setup_wired_pair();
    let stored = name_of(&designer, &[], float_id);
    let undo_depth_before = designer.undo_stack.history_len();

    for (candidate, expected_reason) in [
        ("", "empty"),
        ("   ", "empty"), // trimmed first, so whitespace-only reads as Empty
        ("bad`name", "backtick"),
        ("a/b", "slash"),
    ] {
        let err = designer
            .rename_node(&[], float_id, candidate)
            .unwrap_err()
            .to_lowercase();
        assert!(
            err.contains(expected_reason),
            "renaming to {candidate:?} should report `{expected_reason}`, got: {err}"
        );
    }

    assert_eq!(name_of(&designer, &[], float_id), stored);
    assert_eq!(
        designer.undo_stack.history_len(),
        undo_depth_before,
        "a rejected rename must push no undo entry"
    );
}

#[test]
fn rejects_a_name_another_node_in_the_same_scope_holds() {
    let (mut designer, float_id, sphere_id) = setup_wired_pair();
    let sphere_name = name_of(&designer, &[], sphere_id);
    let stored = name_of(&designer, &[], float_id);
    let undo_depth_before = designer.undo_stack.history_len();

    let err = designer
        .rename_node(&[], float_id, &sphere_name)
        .unwrap_err();
    assert!(
        err.contains("already used"),
        "expected a uniqueness reason, got: {err}"
    );
    assert_eq!(name_of(&designer, &[], float_id), stored);
    assert_eq!(designer.undo_stack.history_len(), undo_depth_before);
}

// ===== Scope =====

#[test]
fn uniqueness_is_per_scope_and_a_body_node_renames_by_scope_path() {
    let mut designer = setup_designer_with_network("main");
    let map_a = designer.add_node("map", DVec2::new(0.0, 0.0));
    let map_b = designer.add_node("map", DVec2::new(400.0, 0.0));
    let top = designer.add_node("int", DVec2::new(0.0, 300.0));
    let in_a = designer.add_node_scoped(&[map_a], "int", DVec2::ZERO, None);
    let in_b = designer.add_node_scoped(&[map_b], "int", DVec2::ZERO, None);

    // A body node renames through its scope path.
    designer.rename_node(&[map_a], in_a, "shared").unwrap();
    assert_eq!(name_of(&designer, &[map_a], in_a), "shared");

    // The same name is free in a sibling body …
    designer.rename_node(&[map_b], in_b, "shared").unwrap();
    assert_eq!(name_of(&designer, &[map_b], in_b), "shared");

    // … and in the parent scope.
    designer.rename_node(&[], top, "shared").unwrap();
    assert_eq!(name_of(&designer, &[], top), "shared");

    // But not twice within one scope.
    let also_in_a = designer.add_node_scoped(&[map_a], "int", DVec2::new(0.0, 200.0), None);
    assert!(designer.rename_node(&[map_a], also_in_a, "shared").is_err());
}

// ===== No-op =====

#[test]
fn renaming_to_the_same_name_is_a_no_op() {
    let (mut designer, float_id, _) = setup_wired_pair();
    let stored = name_of(&designer, &[], float_id);
    designer.set_dirty(false);
    let undo_depth_before = designer.undo_stack.history_len();

    designer.rename_node(&[], float_id, &stored).unwrap();
    assert_eq!(
        designer.undo_stack.history_len(),
        undo_depth_before,
        "a no-op rename must push no undo entry"
    );
    assert!(
        !designer.is_dirty,
        "a no-op rename must not dirty the project"
    );

    // Surrounding whitespace is trimmed before the comparison, so a padded
    // spelling of the same name is the same no-op — not an EdgeWhitespace
    // rejection.
    designer
        .rename_node(&[], float_id, &format!("  {stored}  "))
        .unwrap();
    assert_eq!(designer.undo_stack.history_len(), undo_depth_before);
    assert!(!designer.is_dirty);
    assert_eq!(name_of(&designer, &[], float_id), stored);
}

#[test]
fn a_real_rename_dirties_the_project() {
    let (mut designer, float_id, _) = setup_wired_pair();
    designer.set_dirty(false);
    designer.rename_node(&[], float_id, "chassis").unwrap();
    assert!(designer.is_dirty);
}

// ===== The D1 invariant survives a rename =====

#[test]
fn stored_names_stay_the_names_the_serializer_prints() {
    use atomcad_structure_designer::text_format::unique_node_names;

    let (mut designer, float_id, _) = setup_wired_pair();
    designer.rename_node(&[], float_id, "x.shape").unwrap();

    let network = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    for (id, printed) in unique_node_names(network) {
        assert_eq!(
            network.nodes[&id].custom_name.as_deref(),
            Some(printed.as_str()),
            "`unique_node_names` must rename nothing after a GUI rename"
        );
    }
}
