//! Regression tests for issue #377 — "renaming a record parameter field drops
//! wires from delayed closure inputs to such field inputs, non-locally!".
//!
//! Design: `doc/design_record_field_identity.md` (Phase R2). These were written
//! [RED-FIRST] against the `id: None` code and **observed failing**; they pass
//! once the field's stable `FieldId` is stamped onto the pin
//! (`build_node_type_for_schema_with_defs`) and the schema editor commits an
//! identity-aware edit (`update_record_type_def_with_ids`), so
//! `set_custom_node_type`'s id-first matching preserves the wire across a rename
//! / reorder — at top level, in another network, and inside HOF bodies.
//!
//! The identity-aware entry point [`StructureDesigner::update_record_type_def_with_ids`]
//! is exactly what the Flutter schema editor calls (it echoes each row's
//! `FieldId`). The legacy name-tuple [`StructureDesigner::update_record_type_def`]
//! deliberately still reads a rename as delete+add (covered by
//! `record_types_phase3_test::field_rename_disconnects_old_pin_wires`).
//!
//! These tests cover the wire drop on `record_construct`'s **input** pins. They
//! deliberately do NOT touch record-value structural compatibility (which is, and
//! stays, name-based) — see the design doc §2.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_network::{IncomingWire, SourcePin};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    FieldId, NodeTypeRegistry, RecordFieldEdit, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
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

/// The `FieldId` of the def's field named `field` (panics if absent).
fn field_id(designer: &StructureDesigner, def: &str, field: &str) -> FieldId {
    designer.node_type_registry.record_type_defs[def]
        .fields
        .iter()
        .find(|f| f.name == field)
        .unwrap_or_else(|| panic!("field '{}' not found in def '{}'", field, def))
        .id
}

/// An edit row for an existing field (carries its identity).
fn existing(id: FieldId, name: &str, ty: DataType) -> RecordFieldEdit {
    RecordFieldEdit {
        id: Some(id),
        name: name.to_string(),
        data_type: ty,
        hint: None,
    }
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
/// renamed. RED before the fix: the wire on field `a` was dropped when `a → aa`.
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

    // Rename field `a` -> `aa` (authored order preserved), by identity.
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "aa", DataType::Int),
                existing(id_b, "b", DataType::Int),
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
/// only the type def. RED before the fix: the wire inside `mkpair` was dropped
/// non-locally.
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

    // Rename field `a` -> `aa` on the Pair type def (by identity).
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "aa", DataType::Int),
                existing(id_b, "b", DataType::Int),
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
// [RED-FIRST] #377 headline: rename drops a wire inside a closure (HOF) body
// ============================================================================

/// The exact reported case: a `record_construct` lives inside a `map` body, with
/// the body's `element` zone-input wired into one of its fields. Renaming that
/// field (an edit made on the *type def*, not in the body) must preserve the body
/// wire. RED before the fix: the body wire was dropped non-locally, in a body the
/// user never opened. Exercises the `repair_zone_body` recursion.
#[test]
fn rename_preserves_wire_inside_hof_body() {
    let mut designer = setup("Main");
    designer.add_record_type_def(pair_def()).unwrap();

    // map(input=Int, output=Record(Pair)) at top level.
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let net = registry.node_networks.get_mut("Main").unwrap();
        net.nodes.get_mut(&map_id).unwrap().data = Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int, // not load-bearing for this wire test
        });
    }

    // Add a record_construct(Pair) INTO the map body, then populate its pin
    // layout (2 fields → 2 input pins).
    let field_count = designer.node_type_registry.record_type_defs["Pair"]
        .fields
        .len();
    let rc_body_id = {
        let registry = &mut designer.node_type_registry;
        let body = registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .expect("map node must own a zone body");
        let id = body.add_node(
            "record_construct",
            DVec2::new(50.0, 0.0),
            field_count,
            Box::new(RecordConstructData {
                schema: "Pair".to_string(),
                literal_values: HashMap::new(),
            }),
        );
        // Re-fetch through the registry to populate the body node's pin cache.
        let node = registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
        id
    };

    // Wire the map's `element` zone-input pin into field `a` (param index 0 in
    // authored order) of the body record_construct.
    {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        body.nodes.get_mut(&rc_body_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: map_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
    }

    // Precondition: the body wire exists.
    let body_wires = |d: &StructureDesigner| {
        d.node_type_registry.node_networks["Main"].nodes[&map_id]
            .zone
            .as_ref()
            .unwrap()
            .nodes[&rc_body_id]
            .arguments[0]
            .incoming_wires
            .len()
    };
    assert_eq!(
        body_wires(&designer),
        1,
        "precondition: body field `a` wired"
    );

    // Rename `a` -> `aa` on the def (from Main — the body is never opened).
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "aa", DataType::Int),
                existing(id_b, "b", DataType::Int),
            ],
        )
        .unwrap();

    // The body wire (now feeding `aa`, still pin index 0) must survive.
    assert_eq!(
        body_wires(&designer),
        1,
        "wire inside the HOF body feeding the renamed field must be preserved (non-local)"
    );
    // And the body record_construct's pin 0 is indeed the renamed field.
    let body_pin0 = designer.node_type_registry.node_networks["Main"].nodes[&map_id]
        .zone
        .as_ref()
        .unwrap()
        .nodes[&rc_body_id]
        .custom_node_type
        .as_ref()
        .expect("body record_construct keeps a cached custom_node_type")
        .parameters[0]
        .name
        .clone();
    assert_eq!(body_pin0, "aa", "body pin 0 is the renamed field");
}

// ============================================================================
// [RED-FIRST] Name swap must follow identity, not name (insidious mis-wire)
// ============================================================================

/// Swapping two field names in one commit must route each wire to the field it
/// was on (by identity). RED before the fix: name-matching silently re-paired the
/// wires by the swapped names, so the values ended up on the WRONG fields
/// (mis-wired, not merely dropped).
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

    // Swap names by identity: the field with id_a (authored first, fed by va) is
    // renamed to `b`; the field with id_b (authored second, fed by vb) becomes
    // `a`. Authored order is unchanged, so va must stay on the first authored
    // field, vb on the second.
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "b", DataType::Int), // first authored field, formerly `a`
                existing(id_b, "a", DataType::Int), // second authored field, formerly `b`
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
// [RED-FIRST] `literal_values` defaults follow a rename (top-level + body)
// ============================================================================

/// A stored literal default (no wire) is keyed by field name. A rename must
/// re-key it so the default survives — at top level AND inside an HOF body
/// (`rekey_record_construct_literals` walks every network via
/// `walk_all_nodes_mut`). RED before the fix: the literal was orphaned/lost.
#[test]
fn literal_values_follow_rename() {
    let mut designer = setup("Main");
    designer.add_record_type_def(pair_def()).unwrap();

    // Top-level record_construct with a stored literal on field `a` (no wire).
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Pair");
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap();
        let rc_data = net
            .nodes
            .get_mut(&rc)
            .unwrap()
            .data
            .as_any_mut()
            .downcast_mut::<RecordConstructData>()
            .unwrap();
        rc_data
            .literal_values
            .insert("a".to_string(), TextValue::Int(42));
    }

    // A second record_construct(Pair) with a literal on `a`, inside a map body.
    let map_id = designer.add_node("map", DVec2::new(400.0, 0.0));
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap();
        net.nodes.get_mut(&map_id).unwrap().data = Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        });
    }
    let field_count = designer.node_type_registry.record_type_defs["Pair"]
        .fields
        .len();
    let rc_body_id = {
        let registry = &mut designer.node_type_registry;
        let body = registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap();
        let mut data = RecordConstructData {
            schema: "Pair".to_string(),
            literal_values: HashMap::new(),
        };
        data.literal_values
            .insert("a".to_string(), TextValue::Int(7));
        body.add_node(
            "record_construct",
            DVec2::new(50.0, 0.0),
            field_count,
            Box::new(data),
        )
    };

    // Rename `a` -> `aa`.
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "aa", DataType::Int),
                existing(id_b, "b", DataType::Int),
            ],
        )
        .unwrap();

    // Top-level literal moved from `a` to `aa`.
    let top_literals = &designer.node_type_registry.node_networks["Main"].nodes[&rc]
        .data
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap()
        .literal_values;
    assert!(
        !top_literals.contains_key("a"),
        "old literal key `a` must be gone after rename"
    );
    assert_eq!(
        top_literals.get("aa").and_then(|v| v.as_int()),
        Some(42),
        "top-level literal default must follow the rename to `aa`"
    );

    // Body literal moved from `a` to `aa`.
    let body_literals = &designer.node_type_registry.node_networks["Main"].nodes[&map_id]
        .zone
        .as_ref()
        .unwrap()
        .nodes[&rc_body_id]
        .data
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap()
        .literal_values;
    assert!(
        !body_literals.contains_key("a"),
        "old body literal key `a` must be gone after rename"
    );
    assert_eq!(
        body_literals.get("aa").and_then(|v| v.as_int()),
        Some(7),
        "body literal default must follow the rename to `aa` (non-local)"
    );
}

// ============================================================================
// [GUARD] Deleting a field drops ITS wire while siblings survive
// ============================================================================

/// Deleting field `a` must drop `a`'s wire while `b`'s wire survives. Guards the
/// fix against resurrecting a deleted field's wire (the deleted field's id is
/// absent from the edit, so its wire drops; the fix must NOT migrate the orphaned
/// wire onto a surviving/new field).
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

    // Delete field `a` (omit its id from the edit), keep `b` by identity.
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids("Pair", vec![existing(id_b, "b", DataType::Int)])
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

// ============================================================================
// [GUARD] An incompatible retype preserves the wire but flags a type error
// ============================================================================

/// Retyping a wired field to an incompatible type must NOT drop the wire at the
/// repair layer — it stays in `arguments` and the network goes invalid (the type
/// mismatch is a blocking validation error), exactly as for any other mistyped
/// wire. Guards against the fix accidentally dropping a type-mismatched wire (the
/// §3 table behaviour) while proving the wire is preserved by id.
#[test]
fn guard_incompatible_retype_preserves_wire_but_flags_error() {
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
    designer.validate_active_network();

    // Retype field `a` from Int to Vec3 — an Int source no longer fits.
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "a", DataType::Vec3),
                existing(id_b, "b", DataType::Int),
            ],
        )
        .unwrap();
    designer.validate_active_network();

    // Wire preserved by id (NOT dropped at the repair layer).
    let a_idx2 = field_index(&designer, "Main", rc, "a");
    assert_eq!(
        wires_on(&designer, "Main", rc, a_idx2),
        1,
        "incompatible retype must keep the wire in arguments (preserved by id)"
    );
    // The pin really did retype.
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
    assert_eq!(nt.parameters[a_idx2].data_type, DataType::Vec3);
    // ...and the network is now invalid (the type mismatch is surfaced).
    assert!(
        !net.valid,
        "an Int source feeding a Vec3 field must make the network invalid"
    );
}
