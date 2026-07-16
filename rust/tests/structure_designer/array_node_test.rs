//! Phase 3 tests for the `array` literal node. See
//! `doc/design_array_node_and_field_hints.md` Part B.
//!
//! The `array` node is a one-node array literal: pick an element type, then
//! author the elements as stored literals. It has **no input pins** (Decision
//! 1), its `element_type` must be **literal-capable** (Decision 2), and stale
//! literals are **preserved, never silently dropped** — mismatches surface as
//! localized per-element eval errors.

use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_data::{DragDirection, NoData, NodeData};
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::node_type::{
    NodeType, OutputPinDefinition, no_data_loader, no_data_saver,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    FieldId, NodeTypeRegistry, RecordFieldEdit, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::array::ArrayData;
use rust_lib_flutter_cad::structure_designer::nodes::atom_replace::AtomReplaceData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::{
    TextValue, edit_network, serialize_network,
};
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Add an `array` node carrying `data`, refreshing its cached pin layout.
fn add_array_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    data: ArrayData,
    x: f64,
) -> u64 {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node_id = network.add_node("array", DVec2::new(x, 0.0), 0, Box::new(data));
    let node = network.nodes.get_mut(&node_id).unwrap();
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
    node_id
}

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

/// Evaluate a standalone `array` node holding `elements` of `element_type`.
fn eval_array(element_type: DataType, elements: Vec<TextValue>) -> NetworkResult {
    let mut designer = setup_designer_with_network("Main");
    let data = ArrayData {
        element_type,
        elements,
    };
    let node_id = add_array_node(&mut designer, "Main", data, 0.0);
    evaluate(&designer, "Main", node_id)
}

fn expect_array(result: NetworkResult) -> Vec<NetworkResult> {
    match result {
        NetworkResult::Array(items) => items,
        NetworkResult::Error(e) => panic!("Expected Array, got Error: {}", e),
        other => panic!("Expected Array, got {:?}", other.infer_data_type()),
    }
}

/// `NetworkResult` implements neither `PartialEq` nor `Debug`, so render the
/// value shapes these tests exercise into a comparable string. Field order is
/// preserved verbatim (not re-sorted) so the canonical-order assertions below
/// are meaningful.
fn render(result: &NetworkResult) -> String {
    match result {
        NetworkResult::None => "None".to_string(),
        NetworkResult::Bool(b) => format!("Bool({})", b),
        NetworkResult::Int(i) => format!("Int({})", i),
        NetworkResult::Float(f) => format!("Float({})", f),
        NetworkResult::String(s) => format!("String({:?})", s),
        NetworkResult::IVec2(v) => format!("IVec2({}, {})", v.x, v.y),
        NetworkResult::IVec3(v) => format!("IVec3({}, {}, {})", v.x, v.y, v.z),
        NetworkResult::Vec2(v) => format!("Vec2({}, {})", v.x, v.y),
        NetworkResult::Vec3(v) => format!("Vec3({}, {}, {})", v.x, v.y, v.z),
        NetworkResult::IMat3(m) => format!("IMat3({:?})", m),
        NetworkResult::Mat3(m) => format!("Mat3({:?})", m.to_cols_array()),
        NetworkResult::Record(fields) => format!(
            "Record{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{}: {}", name, render(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        NetworkResult::Array(items) => format!(
            "[{}]",
            items.iter().map(render).collect::<Vec<_>>().join(", ")
        ),
        NetworkResult::Error(e) => format!("Error({})", e),
        other => format!("<{:?}>", other.infer_data_type()),
    }
}

fn render_all(results: &[NetworkResult]) -> Vec<String> {
    results.iter().map(render).collect()
}

fn expect_error(result: NetworkResult) -> String {
    match result {
        NetworkResult::Error(e) => e,
        other => panic!("Expected Error, got {:?}", other.infer_data_type()),
    }
}

fn set_props(data: &mut ArrayData, props: Vec<(&str, TextValue)>) -> Result<(), String> {
    let map: HashMap<String, TextValue> =
        props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    data.set_text_properties(&map)
}

fn props_map(data: &ArrayData) -> HashMap<String, TextValue> {
    data.get_text_properties().into_iter().collect()
}

fn obj(entries: Vec<(&str, TextValue)>) -> TextValue {
    TextValue::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn element_mapping_type() -> DataType {
    DataType::Record(RecordType::Named("ElementMapping".to_string()))
}

// ============================================================================
// Node type & defaults
// ============================================================================

#[test]
fn array_node_is_registered_with_no_input_pins() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("array").expect("array registered");
    // Decision 1: literal-only — the node has no input pins at all, which is
    // what makes every element edit a pure node-data mutation.
    let pin_names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert!(
        nt.parameters.is_empty(),
        "array must have no input pins, found {:?}",
        pin_names
    );
}

#[test]
fn array_default_is_empty_int_array() {
    let data = ArrayData::default();
    assert_eq!(data.element_type, DataType::Int);
    assert!(data.elements.is_empty());
}

#[test]
fn array_output_pin_is_array_of_element_type() {
    let mut designer = setup_designer_with_network("Main");
    let data = ArrayData {
        element_type: DataType::Vec3,
        elements: vec![],
    };
    let node_id = add_array_node(&mut designer, "Main", data, 0.0);
    let network = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    let nt = node.custom_node_type.as_ref().expect("custom type cached");
    assert!(nt.parameters.is_empty());
    assert_eq!(*nt.output_type(), DataType::Array(Box::new(DataType::Vec3)));
}

#[test]
fn array_subtitle_tracks_count_and_element_type() {
    let data = ArrayData {
        element_type: DataType::Int,
        elements: vec![TextValue::Int(1), TextValue::Int(2), TextValue::Int(3)],
    };
    assert_eq!(
        data.get_subtitle(&std::collections::HashSet::new()),
        Some("3 × Int".to_string())
    );
}

// ============================================================================
// Eval — simple element types
// ============================================================================

#[test]
fn array_eval_empty_list_is_valid_empty_array() {
    let items = expect_array(eval_array(DataType::Int, vec![]));
    assert!(items.is_empty());
}

#[test]
fn array_eval_each_simple_type() {
    // One case per `APISimpleParamType` member.
    let cases: Vec<(DataType, TextValue, NetworkResult)> = vec![
        (
            DataType::Bool,
            TextValue::Bool(true),
            NetworkResult::Bool(true),
        ),
        (DataType::Int, TextValue::Int(7), NetworkResult::Int(7)),
        (
            DataType::Float,
            TextValue::Float(1.5),
            NetworkResult::Float(1.5),
        ),
        (
            DataType::String,
            TextValue::String("hi".to_string()),
            NetworkResult::String("hi".to_string()),
        ),
        (
            DataType::IVec2,
            TextValue::IVec2(IVec2::new(1, 2)),
            NetworkResult::IVec2(IVec2::new(1, 2)),
        ),
        (
            DataType::IVec3,
            TextValue::IVec3(IVec3::new(1, 2, 3)),
            NetworkResult::IVec3(IVec3::new(1, 2, 3)),
        ),
        (
            DataType::Vec2,
            TextValue::Vec2(DVec2::new(1.0, 2.0)),
            NetworkResult::Vec2(DVec2::new(1.0, 2.0)),
        ),
        (
            DataType::Vec3,
            TextValue::Vec3(DVec3::new(1.0, 2.0, 3.0)),
            NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0)),
        ),
        (
            DataType::IMat3,
            TextValue::IMat3([[1, 0, 0], [0, 1, 0], [0, 0, 1]]),
            NetworkResult::IMat3([[1, 0, 0], [0, 1, 0], [0, 0, 1]]),
        ),
    ];

    for (element_type, literal, expected) in cases {
        let items = expect_array(eval_array(element_type.clone(), vec![literal]));
        assert_eq!(items.len(), 1, "one element for {}", element_type);
        assert_eq!(
            render(&items[0]),
            render(&expected),
            "element value for {}",
            element_type
        );
    }

    // Mat3 goes through a row-major -> column-major DMat3 conversion, so
    // compare the emitted variant rather than hand-building a DMat3.
    let items = expect_array(eval_array(
        DataType::Mat3,
        vec![TextValue::Mat3([
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ])],
    ));
    assert!(matches!(items[0], NetworkResult::Mat3(_)));
}

#[test]
fn array_eval_multiple_elements_keep_order() {
    let items = expect_array(eval_array(
        DataType::Int,
        vec![TextValue::Int(3), TextValue::Int(1), TextValue::Int(2)],
    ));
    assert_eq!(render_all(&items), vec!["Int(3)", "Int(1)", "Int(2)"]);
}

#[test]
fn array_eval_whole_number_literal_coerces_into_float_element() {
    // Same literal-coercion path `record_construct::eval` applies to an
    // unwired field: a whole-number `Int` literal coerces into a `Float`.
    let items = expect_array(eval_array(
        DataType::Float,
        vec![TextValue::Int(2), TextValue::Float(0.5)],
    ));
    assert_eq!(render_all(&items), vec!["Float(2)", "Float(0.5)"]);
}

#[test]
fn array_eval_uncoercible_simple_literal_is_localized_error() {
    let err = expect_error(eval_array(
        DataType::Int,
        vec![
            TextValue::Int(1),
            TextValue::String("not an int".to_string()),
        ],
    ));
    assert!(
        err.contains("array[1]"),
        "error must localize the element index, got: {}",
        err
    );
}

// ============================================================================
// Eval — record element types
// ============================================================================

#[test]
fn array_eval_record_elements_land_typed_and_canonical_ordered() {
    // `ElementMapping = { from: Int, to: Int }` is a built-in def.
    let items = expect_array(eval_array(
        element_mapping_type(),
        vec![
            // Authored in non-canonical order on purpose.
            obj(vec![("to", TextValue::Int(7)), ("from", TextValue::Int(6))]),
            obj(vec![("from", TextValue::Int(1)), ("to", TextValue::Int(2))]),
        ],
    ));
    assert_eq!(items.len(), 2);
    // Fields land typed, and — despite the authored order above —
    // `NetworkResult::record` re-sorts them into canonical (sorted-by-name)
    // order, which `render` preserves verbatim.
    assert_eq!(
        render_all(&items),
        vec![
            "Record{from: Int(6), to: Int(7)}",
            "Record{from: Int(1), to: Int(2)}",
        ]
    );
}

#[test]
fn array_eval_unset_required_field_is_localized_error() {
    let err = expect_error(eval_array(
        element_mapping_type(),
        vec![
            obj(vec![("from", TextValue::Int(6)), ("to", TextValue::Int(7))]),
            // `to` missing — required (not Optional), so this fails.
            obj(vec![("from", TextValue::Int(1))]),
        ],
    ));
    assert!(
        err.contains("array[1]") && err.contains("to"),
        "error must name the element index and field, got: {}",
        err
    );
    assert!(err.contains("unset"), "got: {}", err);
}

#[test]
fn array_eval_uncoercible_record_field_literal_is_localized_error() {
    let err = expect_error(eval_array(
        element_mapping_type(),
        vec![obj(vec![
            ("from", TextValue::String("carbon".to_string())),
            ("to", TextValue::Int(7)),
        ])],
    ));
    assert!(
        err.contains("array[0]") && err.contains("from"),
        "error must name the element index and field, got: {}",
        err
    );
}

#[test]
fn array_eval_non_object_element_for_record_type_is_localized_error() {
    let err = expect_error(eval_array(element_mapping_type(), vec![TextValue::Int(6)]));
    assert!(err.contains("array[0]"), "got: {}", err);
}

#[test]
fn array_eval_unset_optional_field_becomes_explicit_none() {
    // A user def with one required and one Optional field.
    let mut designer = setup_designer_with_network("Main");
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "Tweak",
            vec![
                ("n".to_string(), DataType::Int),
                (
                    "label".to_string(),
                    DataType::Optional(Box::new(DataType::String)),
                ),
            ],
        ))
        .expect("def added");

    let data = ArrayData {
        element_type: DataType::Record(RecordType::Named("Tweak".to_string())),
        // `label` deliberately absent = unset.
        elements: vec![obj(vec![("n", TextValue::Int(4))])],
    };
    let node_id = add_array_node(&mut designer, "Main", data, 0.0);
    let items = expect_array(evaluate(&designer, "Main", node_id));

    // The emit-all-fields invariant: the record carries BOTH fields, the unset
    // Optional one as an explicit `None`.
    assert_eq!(render(&items[0]), "Record{label: None, n: Int(4)}");
}

#[test]
fn array_eval_set_optional_field_carries_its_value() {
    let mut designer = setup_designer_with_network("Main");
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "Tweak",
            vec![
                ("n".to_string(), DataType::Int),
                (
                    "label".to_string(),
                    DataType::Optional(Box::new(DataType::String)),
                ),
            ],
        ))
        .expect("def added");

    let data = ArrayData {
        element_type: DataType::Record(RecordType::Named("Tweak".to_string())),
        elements: vec![obj(vec![
            ("n", TextValue::Int(4)),
            ("label", TextValue::String("hi".to_string())),
        ])],
    };
    let node_id = add_array_node(&mut designer, "Main", data, 0.0);
    let items = expect_array(evaluate(&designer, "Main", node_id));

    assert_eq!(
        render(&items[0]),
        "Record{label: String(\"hi\"), n: Int(4)}"
    );
}

// ============================================================================
// End-to-end: array -> atom_replace.rules
// ============================================================================

/// A 2-carbon molecule; `atom_replace` should turn both into nitrogen.
fn two_carbon_molecule() -> NetworkResult {
    let mut atoms = AtomicStructure::new();
    atoms.add_atom(6, DVec3::new(0.0, 0.0, 0.0));
    atoms.add_atom(6, DVec3::new(1.5, 0.0, 0.0));
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

#[test]
fn array_of_element_mapping_drives_atom_replace_rules() {
    // The headline end-to-end this node exists for: one `array` node replaces
    // N `record_construct` nodes + a `sequence` collector.
    let mut designer = setup_designer_with_network("Main");

    let molecule_id = {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap();
        network.add_node(
            "value",
            DVec2::ZERO,
            0,
            Box::new(ValueData {
                value: two_carbon_molecule(),
            }),
        )
    };

    let rules_id = add_array_node(
        &mut designer,
        "Main",
        ArrayData {
            element_type: element_mapping_type(),
            elements: vec![obj(vec![
                ("from", TextValue::Int(6)),
                ("to", TextValue::Int(7)),
            ])],
        },
        100.0,
    );

    let replace_id = designer.add_node("atom_replace", DVec2::new(200.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap();
        network.set_node_network_data(
            replace_id,
            Box::new(AtomReplaceData {
                replacements: vec![],
            }),
        );
    }

    // molecule -> atom_replace.molecule (pin 0), array -> atom_replace.rules (pin 1)
    designer.connect_nodes(molecule_id, 0, replace_id, 0);
    designer.connect_nodes(rules_id, 0, replace_id, 1);

    // The wire itself is the pin-compatibility assertion for
    // `Array[Record(Named(ElementMapping))]` — an incompatible output would
    // have been refused, leaving `rules` unwired and the stored (empty) rule
    // list driving eval.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get("Main")
            .unwrap();
        let node = network.nodes.get(&replace_id).unwrap();
        assert_eq!(
            node.arguments[1].incoming_wires.len(),
            1,
            "array output must wire into atom_replace.rules"
        );
    }

    let atoms = match evaluate(&designer, "Main", replace_id) {
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Error(e) => panic!("atom_replace errored: {}", e),
        other => panic!("unexpected result {:?}", other.infer_data_type()),
    };

    let mut elements: Vec<i16> = atoms
        .atom_ids()
        .map(|&id| atoms.get_atom(id).unwrap().atomic_number)
        .collect();
    elements.sort();
    assert_eq!(
        elements,
        vec![7, 7],
        "both carbons should have been replaced by nitrogen"
    );
}

// ============================================================================
// element_type guard (Decision 2)
// ============================================================================

#[test]
fn array_set_text_properties_rejects_non_literal_capable_element_types() {
    // Structural types, Function/Iter/Unit, nested arrays, and anonymous
    // records have no literal form — all rejected without needing a registry.
    let rejected = vec![
        ("structural Blueprint", DataType::Blueprint),
        ("structural Molecule", DataType::Molecule),
        ("structural Structure", DataType::Structure),
        ("abstract HasAtoms", DataType::HasAtoms),
        ("Unit", DataType::Unit),
        ("IMat2", DataType::IMat2),
        ("nested array", DataType::Array(Box::new(DataType::Int))),
        ("iterator", DataType::Iterator(Box::new(DataType::Int))),
        (
            "anonymous record",
            DataType::Record(RecordType::Anonymous(vec![(
                "a".to_string(),
                DataType::Int,
            )])),
        ),
    ];

    for (label, data_type) in rejected {
        let mut data = ArrayData::default();
        let result = set_props(
            &mut data,
            vec![("element_type", TextValue::DataType(data_type))],
        );
        assert!(
            result.is_err(),
            "{} must be rejected as an array element_type",
            label
        );
        assert!(
            result.unwrap_err().contains("literal-capable"),
            "{}: error should explain the literal-capable rule",
            label
        );
        // Rejected edits leave the node untouched.
        assert_eq!(data.element_type, DataType::Int, "{}", label);
    }
}

#[test]
fn array_set_text_properties_accepts_literal_capable_element_types() {
    for data_type in [
        DataType::Bool,
        DataType::Int,
        DataType::Float,
        DataType::String,
        DataType::IVec2,
        DataType::IVec3,
        DataType::Vec2,
        DataType::Vec3,
        DataType::IMat3,
        DataType::Mat3,
        element_mapping_type(),
    ] {
        let mut data = ArrayData::default();
        set_props(
            &mut data,
            vec![("element_type", TextValue::DataType(data_type.clone()))],
        )
        .unwrap_or_else(|e| panic!("{} should be accepted: {}", data_type, e));
        assert_eq!(data.element_type, data_type);
    }
}

#[test]
fn array_set_text_properties_rejects_non_array_elements() {
    let mut data = ArrayData::default();
    let result = set_props(&mut data, vec![("elements", TextValue::Int(3))]);
    assert!(result.is_err(), "elements must be an array");
}

// ============================================================================
// Stale literals are preserved, never silently dropped
// ============================================================================

#[test]
fn array_retype_preserves_stale_literals_and_localizes_eval_errors() {
    let mut data = ArrayData {
        element_type: DataType::Int,
        elements: vec![TextValue::Int(1), TextValue::Int(2)],
    };

    // Retype Int -> Vec3 with elements present.
    set_props(
        &mut data,
        vec![("element_type", TextValue::DataType(DataType::Vec3))],
    )
    .expect("Vec3 is literal-capable");

    // Data preserved verbatim — no silent drop.
    assert_eq!(
        data.elements,
        vec![TextValue::Int(1), TextValue::Int(2)],
        "stale literals must be preserved verbatim"
    );

    // ...and the mismatch surfaces as a localized eval error.
    let err = expect_error(eval_array(data.element_type.clone(), data.elements.clone()));
    assert!(
        err.contains("array[0]"),
        "eval error must localize the element, got: {}",
        err
    );
}

// ============================================================================
// Def-field rename cascade
// ============================================================================

fn field_id(designer: &StructureDesigner, def_name: &str, field_name: &str) -> FieldId {
    designer
        .node_type_registry
        .lookup_record_type_def(def_name)
        .expect("def exists")
        .fields
        .iter()
        .find(|f| f.name == field_name)
        .expect("field exists")
        .id
}

/// The test that fails if the rename cascade misses `ArrayData`: without the
/// re-key, a field rename silently unsets that field in every array element.
#[test]
fn array_record_elements_survive_a_def_field_rename() {
    let mut designer = setup_designer_with_network("Main");
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "Pair",
            vec![
                ("a".to_string(), DataType::Int),
                ("b".to_string(), DataType::Int),
            ],
        ))
        .expect("def added");

    let node_id = add_array_node(
        &mut designer,
        "Main",
        ArrayData {
            element_type: DataType::Record(RecordType::Named("Pair".to_string())),
            elements: vec![
                obj(vec![("a", TextValue::Int(1)), ("b", TextValue::Int(2))]),
                obj(vec![("a", TextValue::Int(3)), ("b", TextValue::Int(4))]),
            ],
        },
        0.0,
    );

    // Precondition: evaluates cleanly before the rename.
    let before = expect_array(evaluate(&designer, "Main", node_id));
    assert_eq!(before.len(), 2);

    // Rename field `a` -> `aa`, by identity (what the schema editor sends).
    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                RecordFieldEdit {
                    id: Some(id_a),
                    name: "aa".to_string(),
                    data_type: DataType::Int,
                    hint: None,
                },
                RecordFieldEdit {
                    id: Some(id_b),
                    name: "b".to_string(),
                    data_type: DataType::Int,
                    hint: None,
                },
            ],
        )
        .expect("rename accepted");

    // The element entries must have been re-keyed `a` -> `aa`, values intact.
    let network = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let data = network
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ArrayData>()
        .expect("array data");
    assert_eq!(
        data.elements[0],
        obj(vec![("aa", TextValue::Int(1)), ("b", TextValue::Int(2))]),
        "element 0 entries must be re-keyed, values intact"
    );
    assert_eq!(
        data.elements[1],
        obj(vec![("aa", TextValue::Int(3)), ("b", TextValue::Int(4))]),
    );

    // ...and eval is unchanged apart from the new field name — in particular
    // no field silently went unset (which would be an error for a required
    // field, or a quiet `None` for an Optional one).
    let after = expect_array(evaluate(&designer, "Main", node_id));
    assert_eq!(
        render_all(&after),
        vec![
            "Record{aa: Int(1), b: Int(2)}",
            "Record{aa: Int(3), b: Int(4)}",
        ]
    );
}

#[test]
fn array_elements_of_another_def_are_untouched_by_a_rename() {
    let mut designer = setup_designer_with_network("Main");
    for def in ["Pair", "Other"] {
        designer
            .node_type_registry
            .add_record_type_def(RecordTypeDef::from_named_fields(
                def,
                vec![
                    ("a".to_string(), DataType::Int),
                    ("b".to_string(), DataType::Int),
                ],
            ))
            .expect("def added");
    }

    // An `array` of `Other` — its `a` entry must NOT be re-keyed when `Pair.a`
    // is renamed.
    let node_id = add_array_node(
        &mut designer,
        "Main",
        ArrayData {
            element_type: DataType::Record(RecordType::Named("Other".to_string())),
            elements: vec![obj(vec![
                ("a", TextValue::Int(1)),
                ("b", TextValue::Int(2)),
            ])],
        },
        0.0,
    );

    let id_a = field_id(&designer, "Pair", "a");
    let id_b = field_id(&designer, "Pair", "b");
    designer
        .update_record_type_def_with_ids(
            "Pair",
            vec![
                RecordFieldEdit {
                    id: Some(id_a),
                    name: "aa".to_string(),
                    data_type: DataType::Int,
                    hint: None,
                },
                RecordFieldEdit {
                    id: Some(id_b),
                    name: "b".to_string(),
                    data_type: DataType::Int,
                    hint: None,
                },
            ],
        )
        .expect("rename accepted");

    let network = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .unwrap();
    let data = network
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ArrayData>()
        .unwrap();
    assert_eq!(
        data.elements[0],
        obj(vec![("a", TextValue::Int(1)), ("b", TextValue::Int(2))]),
        "an array of a different def must be untouched"
    );
}

// ============================================================================
// Text format — authoring through the real parser / serializer
// ============================================================================
//
// The tests below go through the actual text-format string surface
// (`edit_network` / `serialize_network`), not just `get/set_text_properties`.
// That is the AI-integration path (the atomcad skill edits networks through the
// text format).
//
// KNOWN GAP (pre-existing, NOT specific to `array`): the text-format parser
// only ever hands a **single bare identifier** to `DataType::from_string`
// (`text_format/parser.rs`, the `Token::Identifier` arm of the property-value
// parser). So a `DataType`-valued property can only be authored when the type's
// spelling is one identifier — `Int`, `Float`, `Vec3`. A record element type
// cannot be authored at all: `Record(ElementMapping)` is a lex error (`(` after
// an identifier), and the bare `ElementMapping` fails `from_string` and is then
// silently reinterpreted as a *node reference*, leaving `element_type` at its
// default. `serialize_network` emits the `Record(ElementMapping)` spelling, so
// a record-typed `array` also does not survive a serialize -> parse round trip.
// `sequence` / `array_at` / `collect` / `map` have exactly the same limitation
// (verified against `sequence`), so this is a shared text-format shortcoming
// rather than anything this node introduces. The design doc's "a rule set is
// one statement" claim therefore holds only once that parser gap is closed.

/// A standalone network (not owned by the registry) so `edit_network` can take
/// `&mut network` and `&registry` at once — the same shape the existing
/// `text_format_test.rs` cases use.
fn standalone_network() -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    })
}

fn array_data_of(network: &NodeNetwork) -> ArrayData {
    let node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "array")
        .expect("an array node was authored");
    node.data
        .as_any_ref()
        .downcast_ref::<ArrayData>()
        .expect("array data")
        .clone()
}

/// Author `code` into a fresh network and return the `array` node's data.
fn author_array_from_text(code: &str) -> ArrayData {
    let registry = NodeTypeRegistry::new();
    let mut network = standalone_network();
    let result = edit_network(&mut network, &registry, code, true);
    assert!(
        result.success,
        "authoring must succeed: {:?}",
        result.errors
    );
    array_data_of(&network)
}

#[test]
fn array_is_authorable_from_the_text_format() {
    // A whole element list in one statement, for a simple element type.
    let data = author_array_from_text("xs = array { element_type: Int, elements: [1, 2, 3] }");
    assert_eq!(data.element_type, DataType::Int);
    assert_eq!(
        data.elements,
        vec![TextValue::Int(1), TextValue::Int(2), TextValue::Int(3)]
    );

    // Record-shaped elements parse fine on their own — it is only the
    // `element_type` spelling that the parser cannot express (see KNOWN GAP).
    let data = author_array_from_text(
        "xs = array { element_type: Float, elements: [{ from: 6, to: 7 }] }",
    );
    assert_eq!(
        data.elements,
        vec![obj(vec![
            ("from", TextValue::Int(6)),
            ("to", TextValue::Int(7))
        ])],
        "object element literals must survive the parser"
    );
}

#[test]
fn array_text_format_string_roundtrip() {
    // Serialize an authored `array` back out and re-author it: the second
    // parse must reproduce the same data (the serializer emits what the parser
    // accepts). Simple element type — see KNOWN GAP for record element types.
    let registry = NodeTypeRegistry::new();
    let mut network = standalone_network();
    let original = ArrayData {
        element_type: DataType::Vec3,
        elements: vec![
            TextValue::Vec3(DVec3::new(1.0, 2.0, 3.0)),
            TextValue::Vec3(DVec3::new(-0.5, 0.0, 4.25)),
        ],
    };
    network.add_node("array", DVec2::ZERO, 0, Box::new(original.clone()));

    let text = serialize_network(&network, &registry, Some("test"));
    assert!(
        text.contains("array"),
        "the array node must serialize: {}",
        text
    );

    let reparsed = author_array_from_text(&text);
    assert_eq!(reparsed.element_type, original.element_type);
    assert_eq!(
        reparsed.elements, original.elements,
        "serialize -> parse must be lossless; serialized text was:\n{}",
        text
    );
}

/// Pins the KNOWN GAP above so it is visible and starts failing the day the
/// text-format parser learns to parse a full `DataType` expression (at which
/// point this test should become the positive round-trip it wants to be).
#[test]
fn array_record_element_type_is_not_yet_authorable_from_the_text_format() {
    let registry = NodeTypeRegistry::new();

    // `Record(ElementMapping)` — the serializer's own spelling — is a lex error.
    let mut network = standalone_network();
    let result = edit_network(
        &mut network,
        &registry,
        "rules = array { element_type: Record(ElementMapping) }",
        true,
    );
    assert!(
        !result.success,
        "TODO: parser cannot lex a parenthesized DataType in property position"
    );

    // The bare name fails `DataType::from_string` and is silently taken for a
    // node reference, so `element_type` keeps its default.
    let data = author_array_from_text("rules = array { element_type: ElementMapping }");
    assert_eq!(
        data.element_type,
        DataType::Int,
        "TODO: a bare record name is silently swallowed as a node ref"
    );
}

#[test]
fn array_text_format_rejects_a_non_literal_capable_element_type() {
    let registry = NodeTypeRegistry::new();
    let mut network = standalone_network();
    let result = edit_network(
        &mut network,
        &registry,
        "bad = array { element_type: Molecule }",
        true,
    );
    assert!(
        !result.success || !result.errors.is_empty(),
        "a structural element_type must be rejected by the text format"
    );
}

// ============================================================================
// Text properties round-trip (TextValue level)
// ============================================================================

/// Round-trip `data`'s text properties through get -> set and assert equality.
fn assert_text_roundtrip(data: &ArrayData) {
    let props = props_map(data);
    let mut restored = ArrayData::default();
    restored
        .set_text_properties(&props)
        .expect("round-trip must be accepted");
    assert_eq!(restored.element_type, data.element_type);
    assert_eq!(restored.elements, data.elements);
}

#[test]
fn array_text_format_roundtrips_primitives() {
    assert_text_roundtrip(&ArrayData {
        element_type: DataType::Float,
        elements: vec![
            TextValue::Float(1.5),
            TextValue::Float(-0.25),
            TextValue::Int(3),
        ],
    });
    assert_text_roundtrip(&ArrayData {
        element_type: DataType::Vec3,
        elements: vec![TextValue::Vec3(DVec3::new(1.0, 2.0, 3.0))],
    });
    assert_text_roundtrip(&ArrayData {
        element_type: DataType::Int,
        elements: vec![],
    });
}

#[test]
fn array_text_format_roundtrips_exotic_strings() {
    assert_text_roundtrip(&ArrayData {
        element_type: DataType::String,
        elements: vec![
            TextValue::String("with \"quotes\"".to_string()),
            TextValue::String("non-ASCII: αβγ — ✓".to_string()),
            TextValue::String(String::new()),
            TextValue::String("line\nbreak".to_string()),
        ],
    });
}

#[test]
fn array_text_format_roundtrips_record_objects() {
    assert_text_roundtrip(&ArrayData {
        element_type: element_mapping_type(),
        elements: vec![
            obj(vec![("from", TextValue::Int(6)), ("to", TextValue::Int(7))]),
            obj(vec![("from", TextValue::Int(1)), ("to", TextValue::Int(2))]),
        ],
    });
}

#[test]
fn array_text_format_distinguishes_unset_from_set_optional_fields() {
    // Unset = the entry is absent; set-to-a-value = the entry is present. The
    // round-trip must not conflate them.
    let data = ArrayData {
        element_type: DataType::Record(RecordType::Named("Tweak".to_string())),
        elements: vec![
            obj(vec![("n", TextValue::Int(1))]),
            obj(vec![
                ("n", TextValue::Int(2)),
                ("label", TextValue::String("set".to_string())),
            ]),
        ],
    };
    assert_text_roundtrip(&data);

    let props = props_map(&data);
    let mut restored = ArrayData::default();
    restored.set_text_properties(&props).unwrap();
    let first = restored.elements[0].as_object().unwrap();
    assert!(
        !first.iter().any(|(k, _)| k == "label"),
        "an unset Optional field must stay absent, not materialize as an entry"
    );
    let second = restored.elements[1].as_object().unwrap();
    assert!(second.iter().any(|(k, _)| k == "label"));
}

// ============================================================================
// Serde round-trip (.cnnd)
// ============================================================================

#[test]
fn array_data_serde_roundtrip() {
    let data = ArrayData {
        element_type: element_mapping_type(),
        elements: vec![obj(vec![
            ("from", TextValue::Int(6)),
            ("to", TextValue::Int(7)),
        ])],
    };
    let json = serde_json::to_string(&data).expect("serialize");
    let restored: ArrayData = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.element_type, data.element_type);
    assert_eq!(restored.elements, data.elements);
}

// ============================================================================
// Drag-aware add
// ============================================================================

#[test]
fn array_adapts_to_a_literal_capable_array_input_drag() {
    let registry = NodeTypeRegistry::new();
    let data = ArrayData::default();

    // Dragging backwards from `atom_replace.rules` (Array[ElementMapping]).
    let adapted = data
        .adapt_for_drag_source(
            &DataType::Array(Box::new(element_mapping_type())),
            DragDirection::FromInput,
            &registry,
        )
        .expect("array should surface for an Array[ElementMapping] consumer pin");
    let adapted = adapted
        .as_any_ref()
        .downcast_ref::<ArrayData>()
        .expect("ArrayData");
    assert_eq!(adapted.element_type, element_mapping_type());
    assert!(adapted.elements.is_empty());
}

#[test]
fn array_does_not_adapt_to_non_literal_capable_or_from_output_drags() {
    let registry = NodeTypeRegistry::new();
    let data = ArrayData::default();

    // Array of a structural type — no literal form.
    assert!(
        data.adapt_for_drag_source(
            &DataType::Array(Box::new(DataType::Molecule)),
            DragDirection::FromInput,
            &registry,
        )
        .is_none(),
        "Array[Molecule] has no literal form"
    );

    // Not an array at all — nothing to peel.
    assert!(
        data.adapt_for_drag_source(&DataType::Int, DragDirection::FromInput, &registry)
            .is_none(),
        "a scalar consumer pin is not fed by `array`"
    );

    // FromOutput never adapts: the node has no input pins.
    assert!(
        data.adapt_for_drag_source(&DataType::Int, DragDirection::FromOutput, &registry)
            .is_none(),
        "array has no input pins, so FromOutput must never adapt"
    );
}
