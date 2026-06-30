//! Regression tests for issue #377 — "renaming a record parameter field drops
//! wires from delayed closure inputs to such field inputs, non-locally!".
//!
//! Design: `doc/design_record_field_identity.md` (these are the Phase R2
//! [RED-FIRST] tests). They are written against the current `id: None` code and
//! are **expected to FAIL** until that doc's fix lands (`build_node_type_for_\
//! schema_with_defs` stamping the field's stable `FieldId` onto the pin, so
//! `set_custom_node_type`'s id-first matching preserves the wire across a
//! rename).
//!
//! ## The bug
//!
//! A `record_construct` exposes one input pin per field of its chosen def, built
//! with `id: None` (`nodes/record_construct.rs`). Wire preservation in
//! `NodeNetwork::set_custom_node_type` prefers an id match and falls back to a
//! name match; with `id: None` only the name match is available. Renaming a
//! field (via `update_record_type_def`, which runs `repair_all_networks`) makes
//! the new pin's name match no old pin, so the wire feeding it is dropped — at
//! top level, and recursively inside every consumer network.
//!
//! These tests cover the wire drop on `record_construct`'s **input** pins. They
//! deliberately do NOT touch record-value structural compatibility (which is, and
//! stays, name-based) — see the design doc §2.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::RecordTypeDef;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// `Pair { a: Int, b: Int }` — authored order `a`, then `b`.
fn pair_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Pair".to_string(),
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
        ],
    )
}

fn set_int(designer: &mut StructureDesigner, network: &str, node_id: u64, value: i32) {
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(network).unwrap();
    net.nodes.get_mut(&node_id).unwrap().data = Box::new(IntData { value });
}

/// Make `node_id` a `record_construct` bound to `schema`, refreshing its cached
/// pin layout from the registry (so its per-field input pins exist).
fn set_record_construct(
    designer: &mut StructureDesigner,
    network: &str,
    node_id: u64,
    schema: &str,
) {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(network).unwrap();
    let node = net.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(RecordConstructData {
        schema: schema.to_string(),
        literal_values: HashMap::new(),
    });
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Incoming-wire count on `node_id`'s `arg_index`-th argument in `network`.
fn wires_on(designer: &StructureDesigner, network: &str, node_id: u64, arg_index: usize) -> usize {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.arguments
        .get(arg_index)
        .map(|a| a.incoming_wires.len())
        .unwrap_or(0)
}

/// The arg index of the field named `field` on `node_id` (authored field order).
fn field_index(designer: &StructureDesigner, network: &str, node_id: u64, field: &str) -> usize {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    let nt = node
        .custom_node_type
        .as_ref()
        .expect("record_construct must have a cached custom_node_type");
    nt.parameters
        .iter()
        .position(|p| p.name == field)
        .unwrap_or_else(|| panic!("field '{}' not found among pins", field))
}

// ============================================================================
// [RED-FIRST] Top-level rename
// ============================================================================

/// A wire feeding a `record_construct` field must survive that field being
/// renamed. RED today: the wire on field `a` is dropped when `a → aa`.
#[test]
fn top_level_field_rename_preserves_input_wire() {
    let mut designer = setup("Main");
    designer.add_record_type_def(pair_def()).unwrap();

    let va = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_int(&mut designer, "Main", va, 7);
    let vb = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_int(&mut designer, "Main", vb, 9);
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Pair");

    let a_idx = field_index(&designer, "Main", rc, "a");
    let b_idx = field_index(&designer, "Main", rc, "b");
    designer.connect_nodes(va, 0, rc, a_idx);
    designer.connect_nodes(vb, 0, rc, b_idx);

    // Precondition: both fields wired.
    assert_eq!(
        wires_on(&designer, "Main", rc, a_idx),
        1,
        "precondition: a wired"
    );
    assert_eq!(
        wires_on(&designer, "Main", rc, b_idx),
        1,
        "precondition: b wired"
    );

    // Rename field `a` -> `aa` (authored order preserved).
    designer
        .update_record_type_def(
            "Pair",
            vec![
                ("aa".to_string(), DataType::Int),
                ("b".to_string(), DataType::Int),
            ],
        )
        .unwrap();

    // The wire that fed `a` must now feed `aa` — same field, just relabeled.
    let aa_idx = field_index(&designer, "Main", rc, "aa");
    assert_eq!(
        wires_on(&designer, "Main", rc, aa_idx),
        1,
        "wire feeding the renamed field (a -> aa) must be preserved, not dropped"
    );
    // The untouched field keeps its wire too.
    let b_idx2 = field_index(&designer, "Main", rc, "b");
    assert_eq!(
        wires_on(&designer, "Main", rc, b_idx2),
        1,
        "untouched field b stays wired"
    );
}

// ============================================================================
// [RED-FIRST] Non-local: rename drops a wire in a network we never edited
// ============================================================================

/// `mkpair` (a separate network) builds a `Pair` from its parameter. Renaming a
/// `Pair` field must NOT disturb wires inside `mkpair` — we never edit `mkpair`,
/// only the type def. RED today: the wire inside `mkpair` is dropped non-locally.
#[test]
fn rename_does_not_drop_wire_in_other_network() {
    let mut designer = setup("Main");
    designer.add_record_type_def(pair_def()).unwrap();

    // Build the consumer network `mkpair`: param -> record_construct(Pair).a/.b
    designer.add_node_network("mkpair");
    designer.set_active_node_network_name(Some("mkpair".to_string()));
    let p = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "mkpair", rc, "Pair");
    let a_idx = field_index(&designer, "mkpair", rc, "a");
    let b_idx = field_index(&designer, "mkpair", rc, "b");
    designer.connect_nodes(p, 0, rc, a_idx);
    designer.connect_nodes(p, 0, rc, b_idx);
    assert_eq!(
        wires_on(&designer, "mkpair", rc, a_idx),
        1,
        "precondition: mkpair a wired"
    );

    // Switch back to Main — we are NOT touching mkpair from here on.
    designer.set_active_node_network_name(Some("Main".to_string()));

    // Rename field `a` -> `aa` on the Pair type def.
    designer
        .update_record_type_def(
            "Pair",
            vec![
                ("aa".to_string(), DataType::Int),
                ("b".to_string(), DataType::Int),
            ],
        )
        .unwrap();

    // The wire inside the untouched `mkpair` network must survive.
    let aa_idx = field_index(&designer, "mkpair", rc, "aa");
    assert_eq!(
        wires_on(&designer, "mkpair", rc, aa_idx),
        1,
        "non-local wire (inside untouched mkpair) feeding renamed field must be preserved"
    );
}

// ============================================================================
// [RED-FIRST] Name swap must follow identity, not name (insidious mis-wire)
// ============================================================================

/// Swapping two field names in one commit must route each wire to the field it
/// was on (by identity). RED today: name-matching silently re-pairs the wires by
/// the swapped names, so the values end up on the WRONG fields (mis-wired, not
/// merely dropped).
#[test]
fn field_name_swap_preserves_wires_by_identity() {
    let mut designer = setup("Main");
    designer.add_record_type_def(pair_def()).unwrap();

    let va = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_int(&mut designer, "Main", va, 7); // -> field a
    let vb = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_int(&mut designer, "Main", vb, 9); // -> field b
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Pair");
    let a_idx = field_index(&designer, "Main", rc, "a");
    let b_idx = field_index(&designer, "Main", rc, "b");
    designer.connect_nodes(va, 0, rc, a_idx);
    designer.connect_nodes(vb, 0, rc, b_idx);

    // Swap names: the field that was `a` becomes `b`, and the field that was `b`
    // becomes `a` (authored order unchanged). By identity, va must stay on the
    // first authored field, vb on the second.
    designer
        .update_record_type_def(
            "Pair",
            vec![
                ("b".to_string(), DataType::Int), // first authored field, formerly `a`
                ("a".to_string(), DataType::Int), // second authored field, formerly `b`
            ],
        )
        .unwrap();

    // First authored field must still be fed by va; second by vb.
    let first_field_wires = wires_on(&designer, "Main", rc, 0);
    let second_field_wires = wires_on(&designer, "Main", rc, 1);
    assert_eq!(
        first_field_wires, 1,
        "first authored field keeps its (va) wire"
    );
    assert_eq!(
        second_field_wires, 1,
        "second authored field keeps its (vb) wire"
    );

    // And specifically the SOURCE on the first field must still be `va`.
    let net = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let first_arg = &net.nodes.get(&rc).unwrap().arguments[0];
    let src = first_arg
        .incoming_wires
        .first()
        .and_then(|w| w.as_legacy_pair())
        .map(|(src, _)| src);
    assert_eq!(
        src,
        Some(va),
        "first authored field must still be fed by va (identity), not vb (swapped name)"
    );
}

// ============================================================================
// [GUARD] Deleting a field drops ITS wire while siblings survive (green now &
// after) — guards the fix against resurrecting a deleted field's wire.
// ============================================================================

/// Deleting field `a` must drop `a`'s wire while `b`'s wire survives. Green
/// today (name-match: `a` absent → dropped, `b` present → kept). Must stay green
/// after the FieldId fix (the deleted field's id is absent, so its wire drops;
/// the fix must NOT migrate the orphaned wire onto a surviving/new field).
#[test]
fn guard_field_delete_drops_only_its_wire() {
    let mut designer = setup("Main");
    designer.add_record_type_def(pair_def()).unwrap();

    let va = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_int(&mut designer, "Main", va, 7);
    let vb = designer.add_node("int", DVec2::new(0.0, 100.0));
    set_int(&mut designer, "Main", vb, 9);
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Pair");
    let a_idx = field_index(&designer, "Main", rc, "a");
    let b_idx = field_index(&designer, "Main", rc, "b");
    designer.connect_nodes(va, 0, rc, a_idx);
    designer.connect_nodes(vb, 0, rc, b_idx);

    // Delete field `a`, keep `b`.
    designer
        .update_record_type_def("Pair", vec![("b".to_string(), DataType::Int)])
        .unwrap();

    // Only one pin remains (`b`), and it must still be fed by vb.
    let net = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let nt = net
        .nodes
        .get(&rc)
        .unwrap()
        .custom_node_type
        .as_ref()
        .unwrap();
    assert_eq!(
        nt.parameters.len(),
        1,
        "only field b remains after deleting a"
    );
    assert_eq!(nt.parameters[0].name, "b");

    let b_idx2 = field_index(&designer, "Main", rc, "b");
    assert_eq!(
        wires_on(&designer, "Main", rc, b_idx2),
        1,
        "surviving field b keeps its wire"
    );
    let first_arg = &net.nodes.get(&rc).unwrap().arguments[b_idx2];
    let src = first_arg
        .incoming_wires
        .first()
        .and_then(|w| w.as_legacy_pair())
        .map(|(src, _)| src);
    assert_eq!(
        src,
        Some(vb),
        "field b must still be fed by vb, not the deleted field's source"
    );
}
