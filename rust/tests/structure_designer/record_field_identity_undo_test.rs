//! Phase R4 of `doc/design_record_field_identity.md` — undo/redo round-trips
//! field **identity** (the `FieldId` list and the `next_field_id` allocator
//! floor), and a seeded permutation property test pins the wire-preservation
//! invariant across random edit sequences.
//!
//! The undo command [`UpdateRecordTypeDefCommand`] stores the exact pre-update
//! `RecordField` list (ids included) plus `next_field_id`, and the identity-aware
//! [`RecordFieldEdit`] list applied. Undo restores the field list/counter
//! verbatim; redo replays the same edit — so a *new* field (`id: None`) is
//! re-allocated from the **restored** counter and reproduces its original id
//! deterministically. These tests mirror the `next_param_id` undo discipline
//! (`doc/design_parameter_wire_stability.md`): allocate-then-bump, floor-on-load,
//! never recycle, restore-on-undo.
//!
//! The four explicit tests cover rename / reorder / add / delete; the property
//! test (`property_field_edits_preserve_wires_and_round_trip_under_undo`) crosses
//! all five edit kinds (incl. retype) and asserts, at every step:
//!   * a field's input-pin wire is present **iff** its `FieldId` persisted (the
//!     repair layer preserves by id, independent of type — an incompatible
//!     retype *keeps* the wire and is surfaced as a validation error instead, see
//!     `record_field_rename_wire_loss_test::guard_incompatible_retype_*`);
//!   * the network is valid **iff** no wired field's pin type rejects its source
//!     (the "pin type stays compatible" axis of the §5 R4 invariant);
//!   * undo restores the previous def state and redo reproduces the edited def
//!     state **byte-identically, ids and counter included**.

use glam::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    FieldId, NodeTypeRegistry, RecordFieldEdit, RecordTypeDef,
};
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

fn named_def(name: &str, fields: &[(&str, DataType)]) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name.to_string(),
        fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    )
}

fn field_id(designer: &StructureDesigner, def: &str, field: &str) -> FieldId {
    designer.node_type_registry.record_type_defs[def]
        .fields
        .iter()
        .find(|f| f.name == field)
        .unwrap_or_else(|| panic!("field '{}' not found in def '{}'", field, def))
        .id
}

fn existing(id: FieldId, name: &str, ty: DataType) -> RecordFieldEdit {
    RecordFieldEdit {
        id: Some(id),
        name: name.to_string(),
        data_type: ty,
    }
}

fn new_field(name: &str, ty: DataType) -> RecordFieldEdit {
    RecordFieldEdit {
        id: None,
        name: name.to_string(),
        data_type: ty,
    }
}

/// The def state that must round-trip under undo/redo: authored field order with
/// ids + names + types, plus the allocator floor.
fn def_state(designer: &StructureDesigner, name: &str) -> (Vec<(u64, String, DataType)>, u64) {
    let def = &designer.node_type_registry.record_type_defs[name];
    (
        def.fields
            .iter()
            .map(|f| (f.id.0, f.name.clone(), f.data_type.clone()))
            .collect(),
        def.next_field_id,
    )
}

fn set_int(designer: &mut StructureDesigner, network: &str, node_id: u64, value: i32) {
    let registry = &mut designer.node_type_registry;
    let net = registry.node_networks.get_mut(network).unwrap();
    net.nodes.get_mut(&node_id).unwrap().data = Box::new(IntData { value });
}

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

/// Arg index of the pin named `field` on `node_id` (authored field order).
fn field_index(designer: &StructureDesigner, network: &str, node_id: u64, field: &str) -> usize {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.custom_node_type
        .as_ref()
        .expect("record_construct must have a cached custom_node_type")
        .parameters
        .iter()
        .position(|p| p.name == field)
        .unwrap_or_else(|| panic!("field '{}' not found among pins", field))
}

/// Incoming-wire count on `node_id`'s `arg_index`-th argument.
fn wires_on(designer: &StructureDesigner, network: &str, node_id: u64, arg_index: usize) -> usize {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    net.nodes.get(&node_id).unwrap().arguments[arg_index]
        .incoming_wires
        .len()
}

/// The (single) source node feeding `node_id`'s `arg_index`-th argument.
fn wire_source(
    designer: &StructureDesigner,
    network: &str,
    node_id: u64,
    arg_index: usize,
) -> Option<u64> {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    net.nodes.get(&node_id).unwrap().arguments[arg_index]
        .incoming_wires
        .first()
        .map(|w| w.source_node_id)
}

// ============================================================================
// Rename — ids + counter + wire round-trip
// ============================================================================

#[test]
fn rename_round_trips_ids_counter_and_wire() {
    let mut designer = setup("Main");
    designer
        .add_record_type_def(named_def(
            "Pair",
            &[("a", DataType::Int), ("b", DataType::Int)],
        ))
        .unwrap();

    let va = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_int(&mut designer, "Main", va, 7);
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Pair");
    let a_idx = field_index(&designer, "Main", rc, "a");
    designer.connect_nodes(va, 0, rc, a_idx);

    let s0 = def_state(&designer, "Pair");
    assert_eq!(s0.1, 2, "counter floors at 2 for two fields");

    // Rename a -> aa by identity.
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

    let s1 = def_state(&designer, "Pair");
    assert_eq!(
        s1,
        (
            vec![
                (id_a.0, "aa".to_string(), DataType::Int),
                (id_b.0, "b".to_string(), DataType::Int),
            ],
            2
        ),
        "rename keeps both ids and the counter; only the name changed"
    );
    assert_eq!(
        wires_on(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "aa")
        ),
        1,
        "wire follows the renamed field"
    );

    // Undo restores the pre-rename def + the wire under the old name.
    assert!(designer.undo());
    assert_eq!(
        def_state(&designer, "Pair"),
        s0,
        "undo restores pre-rename def"
    );
    assert_eq!(
        wires_on(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "a")
        ),
        1,
        "undo restores the wire under the old field name"
    );

    // Redo reproduces the edited def exactly (ids + counter included).
    assert!(designer.redo());
    assert_eq!(
        def_state(&designer, "Pair"),
        s1,
        "redo reproduces the renamed def"
    );
    assert_eq!(
        wires_on(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "aa")
        ),
        1,
        "redo restores the wire under the new field name"
    );
}

// ============================================================================
// Reorder — ids + counter + wire identity round-trip
// ============================================================================

#[test]
fn reorder_round_trips_ids_counter_and_wire() {
    let mut designer = setup("Main");
    designer
        .add_record_type_def(named_def(
            "Triple",
            &[
                ("a", DataType::Int),
                ("b", DataType::Int),
                ("c", DataType::Int),
            ],
        ))
        .unwrap();

    let va = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_int(&mut designer, "Main", va, 7);
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Triple");
    designer.connect_nodes(va, 0, rc, field_index(&designer, "Main", rc, "a"));

    let id_a = field_id(&designer, "Triple", "a");
    let id_b = field_id(&designer, "Triple", "b");
    let id_c = field_id(&designer, "Triple", "c");
    let s0 = def_state(&designer, "Triple");

    // Reorder to [c, a, b] — authored order changes, ids stay put.
    designer
        .update_record_type_def_with_ids(
            "Triple",
            vec![
                existing(id_c, "c", DataType::Int),
                existing(id_a, "a", DataType::Int),
                existing(id_b, "b", DataType::Int),
            ],
        )
        .unwrap();

    let s1 = def_state(&designer, "Triple");
    assert_eq!(
        s1,
        (
            vec![
                (id_c.0, "c".to_string(), DataType::Int),
                (id_a.0, "a".to_string(), DataType::Int),
                (id_b.0, "b".to_string(), DataType::Int),
            ],
            3
        ),
        "reorder permutes fields but ids and counter are unchanged"
    );
    // The wire still feeds field `a` by identity (now at authored index 1).
    assert_eq!(
        wire_source(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "a")
        ),
        Some(va),
        "wire follows field a across the reorder"
    );

    assert!(designer.undo());
    assert_eq!(
        def_state(&designer, "Triple"),
        s0,
        "undo restores original order"
    );
    assert_eq!(
        wire_source(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "a")
        ),
        Some(va),
        "wire still on field a after undo"
    );

    assert!(designer.redo());
    assert_eq!(
        def_state(&designer, "Triple"),
        s1,
        "redo reproduces the reorder"
    );
    assert_eq!(
        wire_source(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "a")
        ),
        Some(va),
        "wire still on field a after redo"
    );
}

// ============================================================================
// Add — new id re-allocated deterministically across undo/redo
// ============================================================================

#[test]
fn add_round_trips_and_reproduces_new_id_deterministically() {
    let mut designer = setup("Main");
    designer
        .add_record_type_def(named_def(
            "Pair",
            &[("a", DataType::Int), ("b", DataType::Int)],
        ))
        .unwrap();

    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    let s0 = def_state(&designer, "Pair");
    assert_eq!(s0.1, 2);

    // Add a third field `c` (id: None → allocate from the counter).
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                existing(id_a, "a", DataType::Int),
                existing(id_b, "b", DataType::Int),
                new_field("c", DataType::Int),
            ],
        )
        .unwrap();

    let s1 = def_state(&designer, "Pair");
    let id_c = field_id(&designer, "Pair", "c");
    assert_eq!(id_c.0, 2, "new field got the next id from the counter");
    assert_eq!(s1.1, 3, "counter advanced past the new id");

    // Undo removes `c` and restores the counter floor to 2.
    assert!(designer.undo());
    assert_eq!(
        def_state(&designer, "Pair"),
        s0,
        "undo removes the added field + restores counter"
    );

    // Redo re-allocates `c` from the restored counter → the SAME id (2). This is
    // the load-bearing R4 guarantee: ids reproduce deterministically because the
    // counter is restored on undo before the edit is replayed.
    assert!(designer.redo());
    assert_eq!(
        def_state(&designer, "Pair"),
        s1,
        "redo reproduces the added field with the identical id and counter"
    );
}

// ============================================================================
// Delete — counter floor is never lowered; wire restored on undo
// ============================================================================

#[test]
fn delete_round_trips_and_keeps_counter_floor() {
    let mut designer = setup("Main");
    designer
        .add_record_type_def(named_def(
            "Triple",
            &[
                ("a", DataType::Int),
                ("b", DataType::Int),
                ("c", DataType::Int),
            ],
        ))
        .unwrap();

    let vb = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_int(&mut designer, "Main", vb, 9);
    let rc = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "Triple");
    designer.connect_nodes(vb, 0, rc, field_index(&designer, "Main", rc, "b"));

    let id_a = field_id(&designer, "Triple", "a");
    let id_c = field_id(&designer, "Triple", "c");
    let s0 = def_state(&designer, "Triple");
    assert_eq!(s0.1, 3);

    // Delete `b` (omit its id), keep a and c by identity.
    designer
        .update_record_type_def_with_ids(
            "Triple",
            vec![
                existing(id_a, "a", DataType::Int),
                existing(id_c, "c", DataType::Int),
            ],
        )
        .unwrap();

    let s1 = def_state(&designer, "Triple");
    assert_eq!(
        s1,
        (
            vec![
                (id_a.0, "a".to_string(), DataType::Int),
                (id_c.0, "c".to_string(), DataType::Int),
            ],
            3
        ),
        "delete keeps surviving ids and does NOT lower the counter floor"
    );

    // Undo restores the deleted field AND the wire that fed it.
    assert!(designer.undo());
    assert_eq!(
        def_state(&designer, "Triple"),
        s0,
        "undo restores the deleted field + counter"
    );
    assert_eq!(
        wires_on(
            &designer,
            "Main",
            rc,
            field_index(&designer, "Main", rc, "b")
        ),
        1,
        "undo restores the wire feeding the previously-deleted field b"
    );

    assert!(designer.redo());
    assert_eq!(
        def_state(&designer, "Triple"),
        s1,
        "redo re-deletes the field"
    );
}

// ============================================================================
// Seeded permutation property test
// ============================================================================

/// One field of the live test model: its current identity (None until the def
/// assigns one), name, pin type, and the source node feeding its input pin (None
/// = unwired).
struct FieldModel {
    id: Option<FieldId>,
    name: String,
    ty: DataType,
    source: Option<u64>,
}

/// `Int` source fits an `Int`/`Float` pin but not a `Vec3` pin — the only pin
/// type in the retype rotation that rejects the source.
fn pin_accepts_int_source(ty: &DataType) -> bool {
    *ty != DataType::Vec3
}

fn property_run(seed: u64) {
    let mut designer = setup("Main");
    designer
        .add_record_type_def(named_def(
            "R",
            &[
                ("f0", DataType::Int),
                ("f1", DataType::Int),
                ("f2", DataType::Int),
            ],
        ))
        .unwrap();
    let rc = designer.add_node("record_construct", DVec2::new(400.0, 0.0));
    set_record_construct(&mut designer, "Main", rc, "R");

    // Wire each of the three initial fields from its own distinct Int source.
    let mut model: Vec<FieldModel> = Vec::new();
    for i in 0..3i32 {
        let name = format!("f{}", i);
        let src = designer.add_node("int", DVec2::new(0.0, f64::from(i) * 80.0));
        set_int(&mut designer, "Main", src, i);
        designer.connect_nodes(src, 0, rc, field_index(&designer, "Main", rc, &name));
        model.push(FieldModel {
            id: Some(field_id(&designer, "R", &name)),
            name,
            ty: DataType::Int,
            source: Some(src),
        });
    }
    designer.validate_active_network();

    let mut name_ctr = 100u64;
    // SplitMix64-flavoured LCG, mirrors invariants_test::property_run.
    let mut state = seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1);
    let next = |state: &mut u64| -> u64 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *state >> 33
    };

    let mut prev_def = def_state(&designer, "R");

    for step in 0..24u64 {
        let choice = next(&mut state) % 5;
        match choice {
            0 => {
                // rename a random field
                let idx = (next(&mut state) as usize) % model.len();
                name_ctr += 1;
                model[idx].name = format!("g{}", name_ctr);
            }
            1 => {
                // reorder: swap two random fields
                if model.len() >= 2 {
                    let i = (next(&mut state) as usize) % model.len();
                    let j = (next(&mut state) as usize) % model.len();
                    model.swap(i, j);
                }
            }
            2 => {
                // retype a random field, cycling Int -> Float -> Vec3 -> Int
                let idx = (next(&mut state) as usize) % model.len();
                model[idx].ty = match model[idx].ty {
                    DataType::Int => DataType::Float,
                    DataType::Float => DataType::Vec3,
                    _ => DataType::Int,
                };
            }
            3 => {
                // add a new, unwired field
                name_ctr += 1;
                model.push(FieldModel {
                    id: None,
                    name: format!("g{}", name_ctr),
                    ty: DataType::Int,
                    source: None,
                });
            }
            _ => {
                // delete a random field (keep at least one)
                if model.len() > 1 {
                    let idx = (next(&mut state) as usize) % model.len();
                    model.remove(idx);
                }
            }
        }

        // Commit the whole field list with per-row identity.
        let edits: Vec<RecordFieldEdit> = model
            .iter()
            .map(|f| RecordFieldEdit {
                id: f.id,
                name: f.name.clone(),
                data_type: f.ty.clone(),
            })
            .collect();
        designer
            .update_record_type_def_with_ids("R", edits)
            .unwrap_or_else(|e| panic!("seed={} step={}: update failed: {:?}", seed, step, e));

        // Re-sync model ids from the def (newly-added fields just got an id).
        for f in &mut model {
            f.id = Some(field_id(&designer, "R", &f.name));
        }

        // --- Wire oracle: pin present iff the field's identity persisted. ---
        let net = &designer.node_type_registry.node_networks["Main"];
        let nt = net
            .nodes
            .get(&rc)
            .unwrap()
            .custom_node_type
            .as_ref()
            .unwrap();
        assert_eq!(
            nt.parameters.len(),
            model.len(),
            "seed={} step={}: pin count tracks the field list",
            seed,
            step
        );
        for f in &model {
            let idx = field_index(&designer, "Main", rc, &f.name);
            match f.source {
                Some(src) => {
                    assert_eq!(
                        wires_on(&designer, "Main", rc, idx),
                        1,
                        "seed={} step={}: field '{}' (persisted id) must keep its wire",
                        seed,
                        step,
                        f.name
                    );
                    assert_eq!(
                        wire_source(&designer, "Main", rc, idx),
                        Some(src),
                        "seed={} step={}: field '{}' must still read its OWN source (identity)",
                        seed,
                        step,
                        f.name
                    );
                }
                None => assert_eq!(
                    wires_on(&designer, "Main", rc, idx),
                    0,
                    "seed={} step={}: freshly-added field '{}' must be unwired",
                    seed,
                    step,
                    f.name
                ),
            }
        }

        // --- Validity oracle: invalid iff some wired field rejects its source. ---
        designer.validate_active_network();
        let expect_valid = model
            .iter()
            .all(|f| f.source.is_none() || pin_accepts_int_source(&f.ty));
        assert_eq!(
            designer.node_type_registry.node_networks["Main"].valid, expect_valid,
            "seed={} step={}: validity must track type compatibility of wired fields",
            seed, step
        );

        // --- Undo/redo round-trips the def state byte-identically. ---
        let cur_def = def_state(&designer, "R");
        assert!(
            designer.undo(),
            "seed={} step={}: undo available",
            seed,
            step
        );
        assert_eq!(
            def_state(&designer, "R"),
            prev_def,
            "seed={} step={}: undo restores the prior def state",
            seed,
            step
        );
        assert!(
            designer.redo(),
            "seed={} step={}: redo available",
            seed,
            step
        );
        assert_eq!(
            def_state(&designer, "R"),
            cur_def,
            "seed={} step={}: redo reproduces the edited def (ids + counter)",
            seed,
            step
        );

        prev_def = cur_def;
    }
}

#[test]
fn property_field_edits_preserve_wires_and_round_trip_under_undo() {
    for seed in [1u64, 7, 42, 1234, 99999] {
        property_run(seed);
    }
}
