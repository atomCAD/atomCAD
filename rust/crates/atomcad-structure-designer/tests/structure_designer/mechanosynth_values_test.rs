//! Phase 1 of `doc/design_mechanosynth_editor.md`: build scripts as network
//! values.
//!
//! The `mechanosynth` node's own behaviour is in `mechanosynth_test.rs`; what
//! is tested here is everything the conversion added around it — the
//! `OpLibrary` data type, the `BuildStep` record and its conversion, the
//! `ops_library` / `build_script` / `export_build_script` nodes, and the
//! **Convert to nodes** migration.

use atomcad_crystolecule::mechanosynth::{
    NO_LAYER, NO_SITE, Step, load_build_script, parse_build_script,
};
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{NetworkResult, dmat3_to_rows};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::build_step::{
    BUILD_STEP_RECORD, build_step_record, step_from_record, steps_from_array,
};
use atomcad_structure_designer::nodes::export_build_script::ExportBuildScriptData;
use atomcad_structure_designer::nodes::mechanosynth::MechanosynthData;
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::nodes::value::ValueData;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{edit_network, serialize_network};
use atomcad_test_support::fixture_path_str;
use glam::f64::{DMat3, DVec2, DVec3};
use std::collections::HashMap;
use tempfile::tempdir;

const NET: &str = "test";

// ============================================================================
// Helpers
// ============================================================================

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
}

/// Evaluates with the side-effect flag set, the way the Execute action does.
fn execute_pin(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.execute = true;
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

fn with_data<T: NodeData + 'static, F: FnOnce(&mut T)>(
    designer: &mut StructureDesigner,
    node_id: u64,
    f: F,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).expect("node exists");
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<T>()
        .expect("node data type matches");
    f(data);
}

fn node_data_of<T: NodeData + Clone + 'static>(designer: &StructureDesigner, node_id: u64) -> T {
    designer
        .node_type_registry
        .node_networks
        .get(NET)
        .unwrap()
        .nodes
        .get(&node_id)
        .expect("node exists")
        .data
        .as_any_ref()
        .downcast_ref::<T>()
        .expect("node data type matches")
        .clone()
}

fn add_ops_library(designer: &mut StructureDesigner, name: &str) -> u64 {
    let node_id = designer.add_node("ops_library", DVec2::new(-400.0, -100.0));
    with_data::<OpsLibraryData, _>(designer, node_id, |data| {
        data.file = Some(fixture(name));
        data.reload_missing(None);
    });
    node_id
}

fn add_build_script(designer: &mut StructureDesigner, name: &str) -> u64 {
    let node_id = designer.add_node("build_script", DVec2::new(-400.0, 100.0));
    with_data::<BuildScriptData, _>(designer, node_id, |data| {
        data.file = Some(fixture(name));
        data.reload_missing(None);
    });
    node_id
}

fn add_value_node(designer: &mut StructureDesigner, value: NetworkResult) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    network.add_node("value", DVec2::ZERO, 0, Box::new(ValueData { value }))
}

fn expect_error(result: NetworkResult) -> String {
    match result {
        NetworkResult::Error(message) => message,
        other => panic!("expected an error, got {}", other.to_display_string()),
    }
}

/// Authors `code` into the active network and returns the serialization.
fn author_and_serialize(designer: &mut StructureDesigner, code: &str) -> String {
    let registry = &mut designer.node_type_registry;
    let mut network = registry.node_networks.remove(NET).expect("test network");
    let result = edit_network(&mut network, registry, code, true);
    assert!(
        result.success,
        "authoring must succeed: {:?}",
        result.errors
    );
    let text = serialize_network(&network, registry, None);
    registry.node_networks.insert(NET.to_string(), network);
    text
}

// ============================================================================
// The `OpLibrary` data type
// ============================================================================

#[test]
fn ops_library_declares_the_op_library_type_and_only_that_type_accepts_it() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry
        .get_node_type("ops_library")
        .expect("ops_library is registered");
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(node_type.output_type(), &DataType::OpLibrary);

    // `OpLibrary` is its own type: it converts to nothing and nothing converts
    // to it. That is what keeps it opaque.
    assert!(DataType::can_be_converted_to(
        &DataType::OpLibrary,
        &DataType::OpLibrary,
        &registry
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::OpLibrary,
        &DataType::HasAtoms,
        &registry
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::OpLibrary,
        &DataType::String,
        &registry
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::String,
        &DataType::OpLibrary,
        &registry
    ));
}

#[test]
fn an_op_library_wire_is_accepted_by_the_ops_pin_and_refused_by_the_base_pin() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    let ops_id = add_ops_library(&mut designer, "methylate_ops.json");

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    // Pin 1 is `ops`, pin 0 is `base: HasAtoms`.
    assert!(
        network.can_connect_nodes(ops_id, 0, node_id, 1, registry, &[], &[]),
        "an OpLibrary source belongs on the ops pin"
    );
    assert!(
        !network.can_connect_nodes(ops_id, 0, node_id, 0, registry, &[], &[]),
        "an OpLibrary must not wire into a HasAtoms pin"
    );

    // And it really does connect, so the gate above is not vacuous.
    designer.connect_nodes(ops_id, 0, node_id, 1);
    let network = designer.node_type_registry.node_networks.get(NET).unwrap();
    assert!(
        network.nodes[&node_id].arguments[1]
            .argument_output_pins()
            .contains_key(&ops_id)
    );
}

#[test]
fn an_op_library_value_reports_its_type_and_reads_out_its_contents() {
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "methylate_ops.json");

    let value = evaluate_pin(&designer, ops_id, 0);
    assert_eq!(value.infer_data_type(), Some(DataType::OpLibrary));

    let readout = value.to_display_string();
    assert!(readout.contains("methylate_ops.json"), "{readout}");
    assert!(
        readout.contains("3"),
        "the op count belongs in it: {readout}"
    );

    // The detailed readout names the operations, which is the only way to see
    // what a library holds from inside the network.
    let detailed = value.to_detailed_string();
    for op in ["habst", "gm_methylate", "hdon"] {
        assert!(detailed.contains(op), "{detailed}");
    }
}

// ============================================================================
// The `BuildStep` record
// ============================================================================

#[test]
fn the_build_step_record_type_is_registered_with_the_documented_fields() {
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def(BUILD_STEP_RECORD)
        .expect("BuildStep is a built-in record type");
    let fields: Vec<(&str, &DataType)> = def
        .fields
        .iter()
        .map(|f| (f.name.as_str(), &f.data_type))
        .collect();
    assert_eq!(
        fields,
        vec![
            ("op", &DataType::String),
            ("t", &DataType::Vec3),
            ("r", &DataType::Mat3),
            ("note", &DataType::String),
            ("method", &DataType::String),
            ("phase", &DataType::String),
            ("layer", &DataType::Int),
            ("site", &DataType::Int),
        ]
    );

    // And `build_script` declares an array of it, so `array_concat`, `map` and
    // the rest type-check against a loaded block.
    let node_type = registry
        .get_node_type("build_script")
        .expect("build_script is registered");
    assert_eq!(
        node_type.output_type(),
        &DataType::Array(Box::new(DataType::Record(RecordType::Named(
            BUILD_STEP_RECORD.to_string()
        ))))
    );
}

#[test]
fn a_step_stating_only_op_and_t_reads_out_with_the_documented_defaults() {
    let script = parse_build_script(
        r#"{ "format": "atomcad-msbuild/1",
             "steps": [ { "op": "habst", "t": [1.0, 2.0, 3.0] } ] }"#,
        "build.json",
    )
    .expect("parses");
    let record = build_step_record(&script.steps[0]);

    // `NetworkResult` has no `PartialEq`, so each field is matched on its
    // variant — which is also what a consumer would do.
    let field = |name: &str| record.extract_record_field(name).expect(name).clone();
    assert!(matches!(field("op"), NetworkResult::String(op) if op == "habst"));
    assert!(matches!(field("t"), NetworkResult::Vec3(t) if t == DVec3::new(1.0, 2.0, 3.0)));
    assert!(matches!(field("r"), NetworkResult::Mat3(r) if r == DMat3::IDENTITY));
    for empty in ["note", "method", "phase"] {
        assert!(
            matches!(field(empty), NetworkResult::String(text) if text.is_empty()),
            "{empty} should default to the empty string"
        );
    }
    assert!(matches!(field("layer"), NetworkResult::Int(layer) if layer == NO_LAYER));
    assert!(matches!(field("site"), NetworkResult::Int(site) if site == NO_SITE));
}

#[test]
fn build_step_to_step_and_back_is_the_identity() {
    let rotation = DMat3::from_cols(
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(-1.0, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
    );
    let steps = vec![
        // Everything at its default, including the identity rotation.
        Step::new("habst", DVec3::new(1.0, 2.0, 3.0)),
        // And everything stated.
        Step {
            op: "dimerize".to_string(),
            t: DVec3::new(4.0, 5.0, 6.0),
            r: rotation,
            note: Some("a note".to_string()),
            method: "probe".to_string(),
            phase: "layer1".to_string(),
            layer: 1,
            site: 0,
        },
    ];

    for (index, step) in steps.iter().enumerate() {
        let round_tripped = step_from_record(&build_step_record(step), index + 1)
            .expect("a record this module wrote converts back");
        assert_eq!(&round_tripped, step);
    }

    // And the whole array at once, which is what the nodes actually call.
    let array = NetworkResult::Array(steps.iter().map(build_step_record).collect());
    assert_eq!(steps_from_array(&array).expect("converts"), steps);
}

#[test]
fn a_step_record_missing_a_required_field_names_the_step_and_the_field() {
    let no_op = NetworkResult::record(vec![("t".to_string(), NetworkResult::Vec3(DVec3::ZERO))]);
    let message = step_from_record(&no_op, 4).expect_err("op is required");
    assert!(
        message.contains("step 4") && message.contains("\"op\""),
        "{message}"
    );

    let wrong_type = NetworkResult::record(vec![
        ("op".to_string(), NetworkResult::String("habst".to_string())),
        ("t".to_string(), NetworkResult::Int(3)),
    ]);
    let message = step_from_record(&wrong_type, 1).expect_err("t must be a Vec3");
    assert!(
        message.contains("\"t\"") && message.contains("Vec3"),
        "{message}"
    );

    // A non-array on the `steps` pin names what arrived rather than panicking.
    let message = steps_from_array(&NetworkResult::Int(1)).expect_err("not an array");
    assert!(message.contains("BuildStep"), "{message}");
}

// ============================================================================
// `ops_library` and `build_script`
// ============================================================================

#[test]
fn the_loader_nodes_emit_what_their_files_hold() {
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "methylate_ops.json");
    let steps_id = add_build_script(&mut designer, "methylate_build.json");

    let NetworkResult::OpLibrary(library) = evaluate_pin(&designer, ops_id, 0) else {
        panic!("the ops pin should carry an OpLibrary");
    };
    assert_eq!(library.ops.len(), 3);

    let steps = evaluate_pin(&designer, steps_id, 0);
    let converted = steps_from_array(&steps).expect("an array of BuildStep records");
    assert_eq!(converted.len(), 3);
    assert_eq!(converted[0].op, "habst");
    assert_eq!(converted[0].t, DVec3::new(0.0, 0.0, 1.09));
}

#[test]
fn a_wired_file_name_overrides_the_stored_one_and_is_not_cached_into_it() {
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "methylate_ops.json");
    let name_id = add_value_node(
        &mut designer,
        NetworkResult::String(fixture("valid_ops.json")),
    );
    designer.connect_nodes(name_id, 0, ops_id, 0);

    let NetworkResult::OpLibrary(library) = evaluate_pin(&designer, ops_id, 0) else {
        panic!("the ops pin should carry an OpLibrary");
    };
    assert!(
        library.get("keep_only").is_some(),
        "the wired name should be the one read"
    );

    // The wired file is parsed at evaluation and never enters the node data.
    let data = node_data_of::<OpsLibraryData>(&designer, ops_id);
    assert_eq!(
        data.file.as_deref(),
        Some(fixture("methylate_ops.json")).as_deref()
    );
    assert!(
        data.library
            .as_ref()
            .is_some_and(|library| library.get("habst").is_some()),
        "the stored cache still holds the stored file's library"
    );
}

#[test]
fn a_parse_failure_names_the_file_and_the_location() {
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "bad_ops.json");

    let message = expect_error(evaluate_pin(&designer, ops_id, 0));
    assert!(message.contains("bad_ops.json"), "{message}");
    assert!(
        message.contains("add_star"),
        "the offending operation should be named: {message}"
    );
}

#[test]
fn a_missing_file_is_an_error_naming_it() {
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "no_such_ops.json");
    let message = expect_error(evaluate_pin(&designer, ops_id, 0));
    assert!(message.contains("no_such_ops.json"), "{message}");

    let steps_id = add_build_script(&mut designer, "no_such_build.json");
    let message = expect_error(evaluate_pin(&designer, steps_id, 0));
    assert!(message.contains("no_such_build.json"), "{message}");
}

#[test]
fn a_library_that_breaks_the_origin_convention_warns_without_failing() {
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "off_origin_ops.json");

    // A warning, not an error: the value still flows.
    assert!(matches!(
        evaluate_pin(&designer, ops_id, 0),
        NetworkResult::OpLibrary(_)
    ));

    let data = node_data_of::<OpsLibraryData>(&designer, ops_id);
    let error = data
        .get_data_error(&Default::default())
        .expect("the convention warning reaches the error list");
    assert!(!error.blocking, "an advisory warning must not block");
    assert!(error.message.contains("displaced"), "{}", error.message);
}

#[test]
fn a_loader_node_relativizes_its_path_on_save_and_reparses_on_load() {
    let tmp = tempdir().expect("tempdir");
    for name in ["methylate_ops.json", "methylate_build.json"] {
        std::fs::copy(fixture(name), tmp.path().join(name)).expect("copy fixture");
    }
    let project_path = tmp.path().join("project.cnnd");

    let mut designer = setup_designer();
    let ops_id = designer.add_node("ops_library", DVec2::ZERO);
    with_data::<OpsLibraryData, _>(&mut designer, ops_id, |data| {
        data.file = Some(tmp.path().join("methylate_ops.json").display().to_string());
        data.reload_missing(None);
    });
    let steps_id = designer.add_node("build_script", DVec2::new(0.0, 200.0));
    with_data::<BuildScriptData, _>(&mut designer, steps_id, |data| {
        data.file = Some(
            tmp.path()
                .join("methylate_build.json")
                .display()
                .to_string(),
        );
        data.reload_missing(None);
    });

    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &project_path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    assert_eq!(
        node_data_of::<OpsLibraryData>(&designer, ops_id)
            .file
            .as_deref(),
        Some("methylate_ops.json")
    );
    assert_eq!(
        node_data_of::<BuildScriptData>(&designer, steps_id)
            .file
            .as_deref(),
        Some("methylate_build.json")
    );

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, project_path.to_str().unwrap())
        .expect("load should succeed");
    let network = registry.node_networks.get(NET).expect("network survives");

    let ops = network.nodes[&ops_id]
        .data
        .as_any_ref()
        .downcast_ref::<OpsLibraryData>()
        .expect("ops_library data");
    assert!(
        ops.library.is_some(),
        "the loader should have repopulated the #[serde(skip)] cache"
    );
    let steps = network.nodes[&steps_id]
        .data
        .as_any_ref()
        .downcast_ref::<BuildScriptData>()
        .expect("build_script data");
    assert_eq!(steps.script.as_ref().expect("reparsed").steps.len(), 3);
}

#[test]
fn a_no_op_file_name_write_keeps_the_parsed_cache() {
    // The property setter fires on every focus loss of a path field
    // (`project_import_node_payload_wipe`).
    let mut designer = setup_designer();
    let ops_id = add_ops_library(&mut designer, "methylate_ops.json");
    let data = node_data_of::<OpsLibraryData>(&designer, ops_id);

    let rewritten = data.with_file(data.file.clone());
    assert!(rewritten.library.is_some());

    let swapped = data.with_file(Some(fixture("valid_ops.json")));
    assert!(swapped.library.is_none(), "a real change drops the cache");
}

#[test]
fn an_unknown_op_passes_through_build_script_unchanged() {
    // This node sees no library, so it cannot check; the consumer reports it.
    let mut designer = setup_designer();
    let steps_id = designer.add_node("build_script", DVec2::ZERO);
    with_data::<BuildScriptData, _>(&mut designer, steps_id, |data| {
        data.script = Some(
            parse_build_script(
                r#"{ "format": "atomcad-msbuild/1",
                     "steps": [ { "op": "no_such_op", "t": [0, 0, 0] } ] }"#,
                "build.json",
            )
            .expect("parses"),
        );
    });

    let steps = steps_from_array(&evaluate_pin(&designer, steps_id, 0)).expect("an array");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].op, "no_such_op");
}

// ============================================================================
// `export_build_script`
// ============================================================================

/// Builds a one-node network that exports `steps` to `path`, and runs it.
fn export(designer: &mut StructureDesigner, steps: Vec<Step>, path: &str) -> NetworkResult {
    let steps_id = add_value_node(
        designer,
        NetworkResult::Array(steps.iter().map(build_step_record).collect()),
    );
    let export_id = designer.add_node("export_build_script", DVec2::new(200.0, 0.0));
    with_data::<ExportBuildScriptData, _>(designer, export_id, |data| {
        data.file_name = path.to_string();
    });
    designer.connect_nodes(steps_id, 0, export_id, 0);
    execute_pin(designer, export_id)
}

#[test]
fn the_exported_file_reparses_to_the_same_steps() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("build.json");
    let steps = load_build_script(std::path::Path::new(&fixture("metadata_build.json")))
        .expect("fixture parses")
        .steps;

    let mut designer = setup_designer();
    let result = export(&mut designer, steps.clone(), path.to_str().unwrap());
    assert!(matches!(result, NetworkResult::Unit));

    let reparsed = load_build_script(&path).expect("the written file parses");
    assert_eq!(reparsed.steps, steps);
}

#[test]
fn a_default_valued_field_is_omitted_and_a_stated_one_is_written() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("build.json");
    let rotation = DMat3::from_cols(
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(-1.0, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
    );

    let mut designer = setup_designer();
    export(
        &mut designer,
        vec![
            Step::new("plain", DVec3::new(1.0, 2.0, 3.0)),
            Step {
                op: "rich".to_string(),
                t: DVec3::ZERO,
                r: rotation,
                note: Some("a note".to_string()),
                method: "probe".to_string(),
                phase: "layer1".to_string(),
                layer: 1,
                site: 0,
            },
        ],
        path.to_str().unwrap(),
    );

    let text = std::fs::read_to_string(&path).expect("written");
    let document: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(document["format"], "atomcad-msbuild/1");

    let plain = &document["steps"][0];
    for absent in ["r", "note", "method", "phase", "layer", "site"] {
        assert!(
            plain.get(absent).is_none(),
            "a defaulted \"{absent}\" must be omitted: {text}"
        );
    }

    let rich = &document["steps"][1];
    assert_eq!(rich["note"], "a note");
    assert_eq!(rich["method"], "probe");
    assert_eq!(rich["phase"], "layer1");
    assert_eq!(rich["layer"], 1);
    assert_eq!(rich["site"], 0);
    // Three rows, the way the reader expects them.
    let rows = dmat3_to_rows(&rotation);
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, value) in row.iter().enumerate() {
            assert_eq!(rich["r"][row_index][column_index], *value);
        }
    }
}

#[test]
fn an_ordinary_evaluation_writes_nothing_and_an_execute_run_writes_once() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("build.json");

    let mut designer = setup_designer();
    let steps_id = add_value_node(
        &mut designer,
        NetworkResult::Array(vec![build_step_record(&Step::new("plain", DVec3::ZERO))]),
    );
    let export_id = designer.add_node("export_build_script", DVec2::new(200.0, 0.0));
    with_data::<ExportBuildScriptData, _>(&mut designer, export_id, |data| {
        data.file_name = path.display().to_string();
    });
    designer.connect_nodes(steps_id, 0, export_id, 0);

    // The central skip rule keeps `eval` from running at all on a display pass.
    assert!(matches!(
        evaluate_pin(&designer, export_id, 0),
        NetworkResult::Unit
    ));
    assert!(!path.exists(), "a display pass must not touch the disk");

    assert!(matches!(
        execute_pin(&designer, export_id),
        NetworkResult::Unit
    ));
    assert!(path.exists(), "an execute run writes the file");
}

// ============================================================================
// Convert to nodes
// ============================================================================

/// A `mechanosynth` node driven by the deprecated file properties, with a
/// methane on its `base` pin.
///
/// The base is an `import_xyz` node rather than the `value` scaffolding the
/// other tests use: `value` declares `DataType::None`, so the repair pass
/// inside `validate_active_network` — which **Convert to nodes** runs —
/// disconnects its wire as a type mismatch.
fn legacy_node(designer: &mut StructureDesigner) -> u64 {
    let base = designer.add_node("import_xyz", DVec2::ZERO);
    with_data::<atomcad_structure_designer::nodes::import_xyz::ImportXYZData, _>(
        designer,
        base,
        |data| {
            data.file_name = Some(fixture("methane.xyz"));
            data.atomic_structure =
                atomcad_crystolecule::io::xyz_loader::load_xyz(&fixture("methane.xyz"), true).ok();
        },
    );
    let node_id = designer.add_node("mechanosynth", DVec2::new(400.0, 0.0));
    designer.connect_nodes(base, 0, node_id, 0);
    with_data::<MechanosynthData, _>(designer, node_id, |data| {
        data.ops_file = Some(fixture("methylate_ops.json"));
        data.build_file = Some(fixture("methylate_build.json"));
        data.reload_missing(None);
    });
    node_id
}

/// The ids of every node of a type, in ascending id order.
fn nodes_of_type(designer: &StructureDesigner, type_name: &str) -> Vec<u64> {
    let network = designer.node_type_registry.node_networks.get(NET).unwrap();
    let mut ids: Vec<u64> = network
        .nodes
        .iter()
        .filter(|(_, node)| node.node_type_name == type_name)
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn convert_to_nodes_builds_both_loaders_wires_them_and_clears_the_properties() {
    let mut designer = setup_designer();
    let node_id = legacy_node(&mut designer);
    let before = evaluate_pin(&designer, node_id, 0).to_detailed_string();

    designer
        .convert_mechanosynth_files_to_nodes(&[], node_id)
        .expect("the node has both properties");

    let ops = nodes_of_type(&designer, "ops_library");
    let steps = nodes_of_type(&designer, "build_script");
    assert_eq!(ops.len(), 1);
    assert_eq!(steps.len(), 1);

    let network = designer.node_type_registry.node_networks.get(NET).unwrap();
    let arguments = &network.nodes[&node_id].arguments;
    assert!(arguments[1].argument_output_pins().contains_key(&ops[0]));
    assert!(arguments[2].argument_output_pins().contains_key(&steps[0]));

    let data = node_data_of::<MechanosynthData>(&designer, node_id);
    assert!(data.ops_file.is_none() && data.build_file.is_none());
    assert!(data.library.is_none() && data.script.is_none());
    assert!(!data.has_legacy_files());

    // And the result is unchanged, which is the whole point.
    assert_eq!(
        evaluate_pin(&designer, node_id, 0).to_detailed_string(),
        before
    );
}

#[test]
fn convert_to_nodes_builds_only_the_node_a_set_property_calls_for() {
    let mut designer = setup_designer();
    let node_id = legacy_node(&mut designer);
    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| {
        data.build_file = None;
        data.script = None;
    });

    designer
        .convert_mechanosynth_files_to_nodes(&[], node_id)
        .expect("one property is enough");

    assert_eq!(nodes_of_type(&designer, "ops_library").len(), 1);
    assert!(nodes_of_type(&designer, "build_script").is_empty());
}

#[test]
fn convert_to_nodes_refuses_a_node_with_nothing_to_convert() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    assert!(
        designer
            .convert_mechanosynth_files_to_nodes(&[], node_id)
            .is_err()
    );
    assert!(nodes_of_type(&designer, "ops_library").is_empty());
}

#[test]
fn convert_to_nodes_is_a_single_undo_entry_that_restores_the_properties() {
    let mut designer = setup_designer();
    let node_id = legacy_node(&mut designer);
    let entries_before = designer.undo_stack.history_len();

    designer
        .convert_mechanosynth_files_to_nodes(&[], node_id)
        .expect("converts");
    assert_eq!(
        designer.undo_stack.history_len(),
        entries_before + 1,
        "the whole conversion is one entry"
    );

    designer.undo();
    let data = node_data_of::<MechanosynthData>(&designer, node_id);
    assert_eq!(
        data.ops_file.as_deref(),
        Some(fixture("methylate_ops.json")).as_deref()
    );
    assert_eq!(
        data.build_file.as_deref(),
        Some(fixture("methylate_build.json")).as_deref()
    );
    assert!(nodes_of_type(&designer, "ops_library").is_empty());
    assert!(nodes_of_type(&designer, "build_script").is_empty());

    designer.redo();
    let data = node_data_of::<MechanosynthData>(&designer, node_id);
    assert!(data.ops_file.is_none() && data.build_file.is_none());
    assert_eq!(nodes_of_type(&designer, "ops_library").len(), 1);
    assert_eq!(nodes_of_type(&designer, "build_script").len(), 1);
}

// ============================================================================
// Text format
// ============================================================================

#[test]
fn the_three_new_node_types_round_trip_through_the_text_format() {
    let mut designer = setup_designer();
    let text = author_and_serialize(
        &mut designer,
        r#"
        lib = ops_library { file: "ops.json" }
        gen = build_script { file: "build.json" }
        out = export_build_script { steps: gen, file_name: "written.json" }
        "#,
    );

    assert!(text.contains("file: \"ops.json\""), "{text}");
    assert!(text.contains("file: \"build.json\""), "{text}");
    assert!(text.contains("file_name: \"written.json\""), "{text}");

    // serialize -> parse -> serialize must be a fixed point, which is what the
    // corpus test requires of every node (`project_text_format_roundtrip`).
    let again = author_and_serialize(&mut designer, &text);
    assert_eq!(again, text);
}

#[test]
fn a_wired_mechanosynth_round_trips_through_the_text_format() {
    let mut designer = setup_designer();
    let text = author_and_serialize(
        &mut designer,
        r#"
        lib = ops_library { file: "ops.json" }
        gen = build_script { file: "build.json" }
        base = import_xyz { file_name: "methane.xyz" }
        m = mechanosynth { base: base, ops: lib, steps: gen, step: 2 }
        "#,
    );

    assert!(text.contains("ops: lib"), "{text}");
    assert!(text.contains("steps: gen"), "{text}");
    let again = author_and_serialize(&mut designer, &text);
    assert_eq!(again, text);
}

#[test]
fn an_empty_file_property_round_trips_on_a_fresh_loader_node() {
    // `get_text_properties` is **total** on both loaders, so a node with no
    // file yet can still be given one from the text.
    let mut designer = setup_designer();
    let text = author_and_serialize(&mut designer, "lib = ops_library {}\n");
    assert!(text.contains("file: \"\""), "{text}");
    let again = author_and_serialize(&mut designer, &text);
    assert_eq!(again, text);
}
