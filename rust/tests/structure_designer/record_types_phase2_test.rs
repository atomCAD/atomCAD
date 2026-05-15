//! Phase 2 tests for record types (see `doc/design_record_types.md`).
//!
//! Phase 2 covers: top-level def storage, add/delete/rename/update operations
//! with namespace-collision and cycle checks, the `DataType` rename walker,
//! `repair_node_network` extension on schema change/deletion, on-load
//! validation, and undo/redo for the four new commands.
//!
//! Phase 3 nodes (`record_construct` / `record_destructure` / `product`) do
//! not exist yet, so the bare-name `schema` / `target` property rewrite is
//! intentionally not tested here — the rename walker is wired up early but
//! has no node-data downcast for those types until Phase 3.

use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, RecordTypeDefError, validate_record_type_defs,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeTypeRegistryNetworks, node_network_to_serializable,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::undo::{UndoContext, UndoStack};

// Helpers --------------------------------------------------------------------

fn point_def() -> RecordTypeDef {
    RecordTypeDef {
        name: "Point".to_string(),
        fields: vec![
            ("x".to_string(), DataType::Int),
            ("y".to_string(), DataType::Int),
        ],
    }
}

fn box_def() -> RecordTypeDef {
    // Box references Point; cycle test for the dependency graph.
    RecordTypeDef {
        name: "Box".to_string(),
        fields: vec![(
            "p".to_string(),
            DataType::Record(RecordType::Named("Point".to_string())),
        )],
    }
}

// ---------------------------------------------------------------------------
// add_record_type_def: success + name collisions + duplicate-field rejection.
// ---------------------------------------------------------------------------

#[test]
fn add_record_type_def_inserts_and_succeeds() {
    let mut registry = NodeTypeRegistry::new();
    assert!(registry.add_record_type_def(point_def()).is_ok());
    assert!(registry.record_type_defs.contains_key("Point"));
}

#[test]
fn add_record_type_def_rejects_duplicate_name_against_existing_def() {
    let mut registry = NodeTypeRegistry::new();
    registry.add_record_type_def(point_def()).unwrap();
    let err = registry.add_record_type_def(point_def()).unwrap_err();
    assert!(matches!(err, RecordTypeDefError::NameCollision(_)));
}

#[test]
fn add_record_type_def_rejects_collision_with_node_network() {
    let mut registry = NodeTypeRegistry::new();
    let net = NodeNetwork::new(NodeType {
        name: "Point".to_string(),
        description: String::new(),
        summary: None,
        category:
            rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory::Custom,
        parameters: Vec::new(),
        output_pins: OutputPinDefinition::single(DataType::None),
        node_data_creator: || {
            Box::new(rust_lib_flutter_cad::structure_designer::node_data::CustomNodeData::default())
        },
        node_data_saver:
            rust_lib_flutter_cad::structure_designer::node_type::generic_node_data_saver::<
                rust_lib_flutter_cad::structure_designer::node_data::CustomNodeData,
            >,
        node_data_loader:
            rust_lib_flutter_cad::structure_designer::node_type::generic_node_data_loader::<
                rust_lib_flutter_cad::structure_designer::node_data::CustomNodeData,
            >,
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
    });
    registry.add_node_network(net);
    let err = registry.add_record_type_def(point_def()).unwrap_err();
    assert!(matches!(err, RecordTypeDefError::NameCollision(_)));
}

#[test]
fn add_record_type_def_rejects_collision_with_built_in_type() {
    // "cuboid" is a built-in node type registered by NodeTypeRegistry::new.
    let mut registry = NodeTypeRegistry::new();
    let err = registry
        .add_record_type_def(RecordTypeDef {
            name: "cuboid".to_string(),
            fields: vec![],
        })
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::NameCollision(_)));
}

#[test]
fn add_record_type_def_rejects_duplicate_field_names() {
    let mut registry = NodeTypeRegistry::new();
    let bad = RecordTypeDef {
        name: "Bad".to_string(),
        fields: vec![
            ("x".to_string(), DataType::Int),
            ("x".to_string(), DataType::Float),
        ],
    };
    let err = registry.add_record_type_def(bad).unwrap_err();
    assert!(matches!(err, RecordTypeDefError::DuplicateField(_, _)));
}

// ---------------------------------------------------------------------------
// Cycle rejection: direct, mutual, transitive, and through Array nesting.
// ---------------------------------------------------------------------------

#[test]
fn cycle_rejected_direct_self_reference() {
    let mut registry = NodeTypeRegistry::new();
    let tree = RecordTypeDef {
        name: "Tree".to_string(),
        fields: vec![(
            "children".to_string(),
            DataType::Array(Box::new(DataType::Record(RecordType::Named(
                "Tree".to_string(),
            )))),
        )],
    };
    let err = registry.add_record_type_def(tree).unwrap_err();
    assert!(matches!(err, RecordTypeDefError::CycleDetected { .. }));
}

#[test]
fn cycle_rejected_mutual_recursion() {
    let mut registry = NodeTypeRegistry::new();
    // First insert A with no cycle.
    registry
        .add_record_type_def(RecordTypeDef {
            name: "A".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();
    // B references A — fine.
    registry
        .add_record_type_def(RecordTypeDef {
            name: "B".to_string(),
            fields: vec![(
                "a".to_string(),
                DataType::Record(RecordType::Named("A".to_string())),
            )],
        })
        .unwrap();
    // Now updating A to reference B would close the cycle.
    let err = registry
        .update_record_type_def(
            "A",
            vec![(
                "b".to_string(),
                DataType::Record(RecordType::Named("B".to_string())),
            )],
        )
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::CycleDetected { .. }));
}

#[test]
fn cycle_rejected_transitive_chain() {
    let mut registry = NodeTypeRegistry::new();
    // A -> B -> C, all acyclic.
    registry
        .add_record_type_def(RecordTypeDef {
            name: "C".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();
    registry
        .add_record_type_def(RecordTypeDef {
            name: "B".to_string(),
            fields: vec![(
                "c".to_string(),
                DataType::Record(RecordType::Named("C".to_string())),
            )],
        })
        .unwrap();
    registry
        .add_record_type_def(RecordTypeDef {
            name: "A".to_string(),
            fields: vec![(
                "b".to_string(),
                DataType::Record(RecordType::Named("B".to_string())),
            )],
        })
        .unwrap();
    // Closing the cycle by updating C to reference A is rejected.
    let err = registry
        .update_record_type_def(
            "C",
            vec![(
                "a".to_string(),
                DataType::Record(RecordType::Named("A".to_string())),
            )],
        )
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::CycleDetected { .. }));
}

// ---------------------------------------------------------------------------
// Field update is visible at every reference site without an explicit walk.
// ---------------------------------------------------------------------------

#[test]
fn field_update_is_visible_through_resolve_fields() {
    let mut registry = NodeTypeRegistry::new();
    registry.add_record_type_def(point_def()).unwrap();

    // Resolve fields before update.
    let pt_named = RecordType::Named("Point".to_string());
    let before = pt_named
        .resolve_fields(&registry)
        .expect("Point exists")
        .into_owned();
    assert_eq!(
        before.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["x", "y"]
    );

    // Update the def: add field z.
    registry
        .update_record_type_def(
            "Point",
            vec![
                ("x".to_string(), DataType::Int),
                ("y".to_string(), DataType::Int),
                ("z".to_string(), DataType::Int),
            ],
        )
        .unwrap();

    let after = pt_named
        .resolve_fields(&registry)
        .expect("Point still exists")
        .into_owned();
    // Returned canonical (sorted) — same as before the update for these names,
    // but the new `z` is now present.
    assert_eq!(
        after.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["x", "y", "z"]
    );
}

// ---------------------------------------------------------------------------
// Rename walker: every DataType reference site is rewritten.
// ---------------------------------------------------------------------------

/// Build a minimal designer that exercises the major DataType reference sites:
/// - Custom-network parameter type
/// - Custom-network return-node output type (via the network's
///   `node_type.output_pins`, set directly on the node type)
/// - Built-in node's data-stored DataType (`array_at.element_type`)
/// - Nested inside `Array[Record(Old)]` and inside another record def.
fn build_designer_referencing(record_name: &str) -> StructureDesigner {
    use rust_lib_flutter_cad::structure_designer::node_type::Parameter;

    let mut designer = StructureDesigner::new();
    designer.add_node_network("Net1");
    designer.set_active_node_network_name(Some("Net1".to_string()));

    // Inject a record-typed parameter directly on the network's node_type
    // (mirrors what the parameter-validation pass writes after a parameter
    // node is added). This avoids needing to fully model parameter-node
    // wiring inside this test.
    {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("Net1")
            .unwrap();
        net.node_type.parameters.push(Parameter {
            id: Some(0),
            name: "p".to_string(),
            data_type: DataType::Record(RecordType::Named(record_name.to_string())),
        });
        // Output pin is `Array[Record(record_name)]` — exercises the nested
        // rewrite path through Array.
        net.node_type.output_pins = vec![
            rust_lib_flutter_cad::structure_designer::node_type::OutputPinDefinition::fixed(
                "result",
                DataType::Array(Box::new(DataType::Record(RecordType::Named(
                    record_name.to_string(),
                )))),
            ),
        ];
    }

    // Add a built-in `array_at` node and set its element_type to
    // `Record(record_name)`.
    let id = designer.add_node("array_at", glam::DVec2::ZERO);
    {
        use rust_lib_flutter_cad::structure_designer::nodes::array_at::ArrayAtData;
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut("Net1")
            .unwrap();
        let node = net.nodes.get_mut(&id).unwrap();
        if let Some(d) = node.data.as_any_mut().downcast_mut::<ArrayAtData>() {
            d.element_type = DataType::Record(RecordType::Named(record_name.to_string()));
        } else {
            panic!("expected ArrayAtData");
        }
    }

    designer
}

#[test]
fn rename_walker_rewrites_parameter_pin_and_array_at_and_record_def_field() {
    let mut designer = build_designer_referencing("Old");
    // Add a record def `Old` and another `Box = { p: Record(Old) }` to test
    // that nested record-def field types also see the rename.
    designer
        .add_record_type_def(RecordTypeDef {
            name: "Old".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();
    designer
        .add_record_type_def(RecordTypeDef {
            name: "Box".to_string(),
            fields: vec![(
                "p".to_string(),
                DataType::Record(RecordType::Named("Old".to_string())),
            )],
        })
        .unwrap();

    // Perform the rename.
    designer.rename_record_type_def("Old", "New").unwrap();

    // 1. Custom-network parameter type rewritten.
    let net = designer
        .node_type_registry
        .node_networks
        .get("Net1")
        .unwrap();
    match &net.node_type.parameters[0].data_type {
        DataType::Record(RecordType::Named(n)) => assert_eq!(n, "New"),
        other => panic!("expected Record(Named(New)), got {:?}", other),
    }

    // 2. Output pin `Array[Record(New)]`.
    use rust_lib_flutter_cad::structure_designer::node_type::PinOutputType;
    match &net.node_type.output_pins[0].data_type {
        PinOutputType::Fixed(DataType::Array(inner)) => match inner.as_ref() {
            DataType::Record(RecordType::Named(n)) => assert_eq!(n, "New"),
            other => panic!("expected Record(Named(New)), got {:?}", other),
        },
        other => panic!("expected Fixed(Array[Record(New)]), got {:?}", other),
    }

    // 3. Built-in `array_at.element_type` rewritten.
    use rust_lib_flutter_cad::structure_designer::nodes::array_at::ArrayAtData;
    let array_at_id = *net
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "array_at")
        .map(|(id, _)| id)
        .unwrap();
    let node = net.nodes.get(&array_at_id).unwrap();
    match node.data.as_any_ref().downcast_ref::<ArrayAtData>() {
        Some(d) => match &d.element_type {
            DataType::Record(RecordType::Named(n)) => assert_eq!(n, "New"),
            other => panic!("expected Record(Named(New)), got {:?}", other),
        },
        None => panic!("expected ArrayAtData"),
    }

    // 4. `Box.fields[0]` (another record def's field type) rewritten.
    let box_def = designer
        .node_type_registry
        .record_type_defs
        .get("Box")
        .unwrap();
    match &box_def.fields[0].1 {
        DataType::Record(RecordType::Named(n)) => assert_eq!(n, "New"),
        other => panic!("expected Record(Named(New)), got {:?}", other),
    }

    // 5. The renamed def itself is now under the new key with name=new.
    let new_def = designer
        .node_type_registry
        .record_type_defs
        .get("New")
        .unwrap();
    assert_eq!(new_def.name, "New");
    assert!(
        !designer
            .node_type_registry
            .record_type_defs
            .contains_key("Old")
    );
}

#[test]
fn rename_walker_does_not_touch_unrelated_networks() {
    // A network that doesn't reference the renamed def should be untouched.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Untouched");
    designer.set_active_node_network_name(Some("Untouched".to_string()));
    designer
        .add_record_type_def(RecordTypeDef {
            name: "Old".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();

    // Snapshot the unrelated network before and after the rename.
    let before = serde_json::to_value(snapshot_one_network(
        &mut designer.node_type_registry,
        "Untouched",
    ))
    .unwrap();
    designer.rename_record_type_def("Old", "New").unwrap();
    let after = serde_json::to_value(snapshot_one_network(
        &mut designer.node_type_registry,
        "Untouched",
    ))
    .unwrap();
    assert_eq!(before, after, "untouched network should be byte-identical");
}

fn snapshot_one_network(registry: &mut NodeTypeRegistry, name: &str)
-> rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::SerializableNodeNetwork{
    let mut net = registry.node_networks.remove(name).unwrap();
    let snap = node_network_to_serializable(&mut net, &registry.built_in_node_types, None).unwrap();
    registry.node_networks.insert(name.to_string(), net);
    snap
}

// ---------------------------------------------------------------------------
// Repair on schema deletion: dangling refs surface and (for Phase 2) the
// registry survives. Wire-disconnect specifics are exercised in Phase 3 once
// `record_construct` / `record_destructure` ship and produce typed pins.
// ---------------------------------------------------------------------------

#[test]
fn delete_record_type_def_makes_named_dangling() {
    let mut registry = NodeTypeRegistry::new();
    registry.add_record_type_def(point_def()).unwrap();
    let pt_named = RecordType::Named("Point".to_string());
    assert!(pt_named.resolve_fields(&registry).is_some());

    registry.delete_record_type_def("Point");
    assert!(pt_named.resolve_fields(&registry).is_none(), "dangling");
}

// ---------------------------------------------------------------------------
// Serialization: records round-trip through .cnnd JSON; pre-record files load
// via #[serde(default)] without a version bump.
// ---------------------------------------------------------------------------

#[test]
fn serialize_record_type_defs_emits_sorted_by_name() {
    let mut registry = NodeTypeRegistry::new();
    registry.add_record_type_def(point_def()).unwrap();
    registry.add_record_type_def(box_def()).unwrap();

    let container = SerializableNodeTypeRegistryNetworks {
        node_networks: Vec::new(),
        version: 3,
        direct_editing_mode: false,
        cli_access_rules: std::collections::HashMap::new(),
        record_type_defs: {
            let mut v: Vec<RecordTypeDef> = registry.record_type_defs.values().cloned().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        },
    };
    let json = serde_json::to_value(&container).unwrap();
    let arr = json["record_type_defs"].as_array().unwrap();
    assert_eq!(arr[0]["name"].as_str(), Some("Box"));
    assert_eq!(arr[1]["name"].as_str(), Some("Point"));
}

#[test]
fn deserialize_pre_record_file_produces_empty_record_type_defs() {
    // Simulate a .cnnd JSON without the `record_type_defs` field.
    let json = serde_json::json!({
        "node_networks": [],
        "version": 3,
    });
    let container: SerializableNodeTypeRegistryNetworks = serde_json::from_value(json).unwrap();
    assert!(container.record_type_defs.is_empty());
}

#[test]
fn validate_record_type_defs_flags_dangling_reference() {
    let mut registry = NodeTypeRegistry::new();
    // Inject a def whose field references a missing record (would not pass
    // through the normal add path — simulates a hand-edited file).
    registry.record_type_defs.insert(
        "Bad".to_string(),
        RecordTypeDef {
            name: "Bad".to_string(),
            fields: vec![(
                "p".to_string(),
                DataType::Record(RecordType::Named("Missing".to_string())),
            )],
        },
    );
    let errors = validate_record_type_defs(&registry);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("dangling") && e.contains("Bad") && e.contains("Missing"))
    );
}

#[test]
fn validate_record_type_defs_flags_cycle_in_loaded_registry() {
    // Inject a self-referential def (bypasses the add-time cycle check).
    let mut registry = NodeTypeRegistry::new();
    registry.record_type_defs.insert(
        "Loop".to_string(),
        RecordTypeDef {
            name: "Loop".to_string(),
            fields: vec![(
                "self".to_string(),
                DataType::Record(RecordType::Named("Loop".to_string())),
            )],
        },
    );
    let errors = validate_record_type_defs(&registry);
    assert!(errors.iter().any(|e| e.contains("references itself")));
}

// ---------------------------------------------------------------------------
// Undo (do → undo → redo equality on JSON snapshots of the registry).
// ---------------------------------------------------------------------------

fn registry_snapshot(designer: &mut StructureDesigner) -> serde_json::Value {
    let defs: Vec<RecordTypeDef> = {
        let mut v: Vec<RecordTypeDef> = designer
            .node_type_registry
            .record_type_defs
            .values()
            .cloned()
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    };
    let mut net_names: Vec<String> = designer
        .node_type_registry
        .node_networks
        .keys()
        .cloned()
        .collect();
    net_names.sort();
    let mut nets = Vec::new();
    for n in net_names {
        let net = designer
            .node_type_registry
            .node_networks
            .get_mut(&n)
            .unwrap();
        nets.push((
            n,
            node_network_to_serializable(
                net,
                &designer.node_type_registry.built_in_node_types,
                None,
            )
            .unwrap(),
        ));
    }
    let container = SerializableNodeTypeRegistryNetworks {
        node_networks: nets,
        version: 3,
        direct_editing_mode: false,
        cli_access_rules: std::collections::HashMap::new(),
        record_type_defs: defs,
    };
    serde_json::to_value(&container).unwrap()
}

fn run_undo_redo_cycle(designer: &mut StructureDesigner) {
    let mut stack = std::mem::take(&mut designer.undo_stack);
    {
        let mut ctx = UndoContext {
            node_type_registry: &mut designer.node_type_registry,
            active_network_name: &mut designer.active_node_network_name,
        };
        stack.undo(&mut ctx);
    }
    designer.undo_stack = stack;
}

fn run_redo_cycle(designer: &mut StructureDesigner) {
    let mut stack = std::mem::take(&mut designer.undo_stack);
    {
        let mut ctx = UndoContext {
            node_type_registry: &mut designer.node_type_registry,
            active_network_name: &mut designer.active_node_network_name,
        };
        stack.redo(&mut ctx);
    }
    designer.undo_stack = stack;
}

#[test]
fn add_record_type_def_undo_redo_round_trip() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));

    let before = registry_snapshot(&mut designer);
    designer.add_record_type_def(point_def()).unwrap();
    let after_do = registry_snapshot(&mut designer);
    assert_ne!(before, after_do);

    run_undo_redo_cycle(&mut designer);
    let after_undo = registry_snapshot(&mut designer);
    assert_eq!(before, after_undo, "undo should restore pre-add state");

    run_redo_cycle(&mut designer);
    let after_redo = registry_snapshot(&mut designer);
    assert_eq!(
        after_do, after_redo,
        "redo should re-establish post-add state"
    );
}

#[test]
fn rename_record_type_def_undo_redo_round_trip() {
    let mut designer = build_designer_referencing("Old");
    designer
        .add_record_type_def(RecordTypeDef {
            name: "Old".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();

    let before = registry_snapshot(&mut designer);
    designer.rename_record_type_def("Old", "New").unwrap();
    let after_do = registry_snapshot(&mut designer);
    assert_ne!(before, after_do);

    run_undo_redo_cycle(&mut designer);
    let after_undo = registry_snapshot(&mut designer);
    assert_eq!(
        before, after_undo,
        "undo should restore Old name everywhere"
    );

    run_redo_cycle(&mut designer);
    let after_redo = registry_snapshot(&mut designer);
    assert_eq!(after_do, after_redo, "redo should re-rename to New");
}

#[test]
fn delete_record_type_def_undo_redo_round_trip() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer.add_record_type_def(point_def()).unwrap();

    let before = registry_snapshot(&mut designer);
    designer.delete_record_type_def("Point").unwrap();
    let after_do = registry_snapshot(&mut designer);
    assert_ne!(before, after_do);

    run_undo_redo_cycle(&mut designer);
    let after_undo = registry_snapshot(&mut designer);
    assert_eq!(before, after_undo, "undo should restore the deleted def");

    run_redo_cycle(&mut designer);
    let after_redo = registry_snapshot(&mut designer);
    assert_eq!(after_do, after_redo, "redo should re-delete");
}

#[test]
fn update_record_type_def_undo_redo_round_trip() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer.add_record_type_def(point_def()).unwrap();

    let before = registry_snapshot(&mut designer);
    designer
        .update_record_type_def(
            "Point",
            vec![
                ("x".to_string(), DataType::Float),
                ("y".to_string(), DataType::Float),
            ],
        )
        .unwrap();
    let after_do = registry_snapshot(&mut designer);
    assert_ne!(before, after_do);

    run_undo_redo_cycle(&mut designer);
    let after_undo = registry_snapshot(&mut designer);
    assert_eq!(before, after_undo, "undo should restore old field types");

    run_redo_cycle(&mut designer);
    let after_redo = registry_snapshot(&mut designer);
    assert_eq!(after_do, after_redo, "redo should re-apply new field types");
}

// ---------------------------------------------------------------------------
// DataType <-> String round-trip for Record(Name) syntax. The `Display` impl
// emits `Record(Foo)` so the string round-trips through `from_string` without
// colliding with bare-identifier node references in the text-format parser.
// ---------------------------------------------------------------------------

#[test]
fn record_named_round_trips_through_data_type_string() {
    let src = DataType::Record(RecordType::Named("Foo".to_string()));
    let s = src.to_string();
    assert_eq!(s, "Record(Foo)");
    let parsed = DataType::from_string(&s).unwrap();
    assert_eq!(parsed, src);
}

#[test]
fn record_named_inside_array_round_trips_through_data_type_string() {
    let src = DataType::Array(Box::new(DataType::Record(RecordType::Named(
        "Foo".to_string(),
    ))));
    let s = src.to_string();
    let parsed = DataType::from_string(&s).unwrap();
    assert_eq!(parsed, src);
}

// Silence import warnings for items kept for cross-test re-use even when only
// some tests in this file consume them.
#[allow(dead_code)]
fn _suppress_unused_import_warnings(_: NodeType, _: NodeNetwork, _: UndoStack) {}
