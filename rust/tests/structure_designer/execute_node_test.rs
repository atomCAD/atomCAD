//! Phase 3 tests for the node-execution design (`doc/design_node_execution.md`).
//!
//! Phase 3 lands the user-visible payoff:
//!  - `export_xyz` returns `Unit` (no longer pass-through Molecule) and
//!    becomes a side-effect node gated by the central skip rule.
//!  - The new `foreach` node iterates a stream of values, runs a body per
//!    element for its side effects, and returns `Unit`. Display-pass cost is
//!    zero because the central skip rule short-circuits the whole subgraph.
//!  - `StructureDesigner::execute_node` triggers an Execute pass on a single
//!    node, setting `context.execute = true` for that pass.
//!
//! The `CounterUnitNode` fixture from `execute_flag_test.rs` is **not** reused
//! here. That file covers the central skip rule and Walker context propagation
//! in isolation; this file exercises the integration with the real
//! `export_xyz` and `foreach` built-ins, plus the orchestrator API.

use std::path::PathBuf;

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::export_xyz::ExportXYZData;
use rust_lib_flutter_cad::structure_designer::nodes::foreach::ForeachData;
use rust_lib_flutter_cad::structure_designer::nodes::import_xyz::ImportXYZData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use tempfile::TempDir;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Build an `import_xyz` node and pre-populate it with a single-atom Molecule
/// so it can drive evaluation without touching disk. Returns the node id.
fn add_inline_import_xyz(designer: &mut StructureDesigner, network_name: &str) -> u64 {
    let id = designer.add_node("import_xyz", DVec2::new(-200.0, 0.0));
    let mut atomic_structure = AtomicStructure::new();
    atomic_structure.add_atom(6, DVec3::new(0.0, 0.0, 0.0)); // a single carbon
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let data = network
        .get_node_network_data_mut(id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<ImportXYZData>()
        .unwrap();
    data.atomic_structure = Some(atomic_structure);
    id
}

fn set_export_xyz_file_name(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    path: &str,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let data = network
        .get_node_network_data_mut(node_id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<ExportXYZData>()
        .unwrap();
    data.file_name = path.to_string();
}

fn evaluate_with_execute(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    execute: bool,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.execute = execute;
    let stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context)
}

// ============================================================================
// foreach registration & default values
// ============================================================================

#[test]
fn foreach_default_input_type_is_float() {
    let data = ForeachData::default();
    assert_eq!(data.input_type, DataType::Float);
}

#[test]
fn foreach_is_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let nt = registry
        .get_node_type("foreach")
        .expect("foreach should be registered");
    assert_eq!(nt.name, "foreach");
    assert!(nt.public);
    // Closures Phase 4 re-added an optional `f` (function value) pin alongside
    // `xs`; the body still lives inside the zone (used when `f` is disconnected).
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "f");
    assert!(matches!(nt.parameters[1].data_type, DataType::Function(_)));
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::Unit);

    // Zone-input pin: element (T). Zone-output: out (Unit).
    assert_eq!(nt.zone_input_pins.len(), 1);
    assert_eq!(nt.zone_input_pins[0].name, "element");
    assert_eq!(nt.zone_output_pins.len(), 1);
    assert_eq!(nt.zone_output_pins[0].name, "out");
    assert_eq!(nt.zone_output_pins[0].data_type, DataType::Unit);
}

#[test]
fn foreach_calculate_custom_node_type_uses_unit_output_and_zone_pins() {
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;

    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("foreach").unwrap();
    let data = ForeachData {
        input_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();
    assert_eq!(custom.parameters.len(), 2);
    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(custom.parameters[1].name, "f");
    assert!(matches!(custom.parameters[1].data_type, DataType::Function(_)));
    assert_eq!(*custom.output_type(), DataType::Unit);

    assert_eq!(custom.zone_input_pins.len(), 1);
    assert_eq!(custom.zone_input_pins[0].fixed_type(), Some(&DataType::Int));
    assert_eq!(custom.zone_output_pins.len(), 1);
    assert_eq!(custom.zone_output_pins[0].data_type, DataType::Unit);
}

// ============================================================================
// export_xyz Unit-ification — central skip rule keeps it inert on display
// ============================================================================

#[test]
fn export_xyz_does_not_write_file_on_display_pass() {
    let mut designer = setup_designer_with_network("main");
    let import_id = add_inline_import_xyz(&mut designer, "main");
    let export_id = designer.add_node("export_xyz", DVec2::ZERO);
    designer.connect_nodes(import_id, 0, export_id, 0);

    let tmp = TempDir::new().expect("tempdir");
    let out_path = tmp.path().join("display_pass.xyz");
    set_export_xyz_file_name(&mut designer, "main", export_id, out_path.to_str().unwrap());

    // execute = false (the default) — central rule must short-circuit the
    // whole eval, so save_xyz is never called.
    let result = evaluate_with_execute(&designer, "main", export_id, false);
    assert!(
        matches!(result, NetworkResult::Unit),
        "display pass on a Unit-returning node must yield Unit (got {})",
        result.to_display_string()
    );
    assert!(
        !out_path.exists(),
        "export_xyz should NOT have written a file on a display pass: {:?}",
        out_path
    );
}

#[test]
fn export_xyz_writes_file_on_execute_pass() {
    let mut designer = setup_designer_with_network("main");
    let import_id = add_inline_import_xyz(&mut designer, "main");
    let export_id = designer.add_node("export_xyz", DVec2::ZERO);
    designer.connect_nodes(import_id, 0, export_id, 0);

    let tmp = TempDir::new().expect("tempdir");
    let out_path = tmp.path().join("execute_pass.xyz");
    set_export_xyz_file_name(&mut designer, "main", export_id, out_path.to_str().unwrap());

    let result = evaluate_with_execute(&designer, "main", export_id, true);
    assert!(matches!(result, NetworkResult::Unit), "expected Unit");
    assert!(
        out_path.exists(),
        "export_xyz should have written a file under Execute: {:?}",
        out_path
    );
}

#[test]
fn execute_node_orchestrator_writes_file_for_export_xyz() {
    // End-to-end test of the Phase 3 API: drive `export_xyz` through
    // `StructureDesigner::execute_node` and assert both the success result
    // and the file landing on disk.
    let mut designer = setup_designer_with_network("main");
    let import_id = add_inline_import_xyz(&mut designer, "main");
    let export_id = designer.add_node("export_xyz", DVec2::ZERO);
    designer.connect_nodes(import_id, 0, export_id, 0);

    let tmp = TempDir::new().expect("tempdir");
    let out_path = tmp.path().join("orchestrated.xyz");
    set_export_xyz_file_name(&mut designer, "main", export_id, out_path.to_str().unwrap());

    let api_result = designer
        .execute_node("main", export_id)
        .expect("execute_node should succeed structurally");
    assert!(
        api_result.ok,
        "Execute pass reported error: {:?}",
        api_result.error
    );
    assert!(api_result.error.is_none());
    assert!(
        out_path.exists(),
        "orchestrator should have produced {:?}",
        out_path
    );
}

#[test]
fn execute_node_returns_err_for_missing_node() {
    let mut designer = setup_designer_with_network("main");
    let result = designer.execute_node("main", 999_999);
    assert!(result.is_err(), "missing node should surface as Err(_)");
}

// ============================================================================
// foreach zone-body helpers — direct API construction
// ============================================================================
//
// Phase 5 retired the function-pin `f` parameter; the body now lives inside
// the foreach node's owned zone. These helpers wire up an inline body via the
// API since the text format doesn't yet have zone syntax.

fn add_expr_to_foreach_body(
    designer: &mut StructureDesigner,
    network_name: &str,
    foreach_id: u64,
    expression: &str,
    parameters: Vec<(String, DataType)>,
) -> u64 {
    use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};

    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let foreach_node = network.nodes.get_mut(&foreach_id).unwrap();
    let body = foreach_node.zone_mut().expect("foreach node missing zone");

    let expr_params: Vec<ExprParameter> = parameters
        .into_iter()
        .map(|(name, dt)| ExprParameter {
            id: None,
            name,
            data_type: dt,
            data_type_str: None,
        })
        .collect();
    let num_params = expr_params.len();
    let mut expr_data = ExprData {
        parameters: expr_params,
        expression: expression.to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = expr_data.parse_and_validate(0);
    let expr_id = body.add_node(
        "expr",
        DVec2::new(50.0, 0.0),
        num_params,
        Box::new(expr_data),
    );

    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        registry
            .node_networks
            .get_mut(network_name)
            .unwrap()
            .nodes
            .get_mut(&foreach_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&expr_id)
            .unwrap(),
        true,
    );

    expr_id
}

fn wire_foreach_zone_input_to_body_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    foreach_id: u64,
    body_node_id: u64,
    body_param_index: usize,
) {
    use rust_lib_flutter_cad::structure_designer::node_network::{IncomingWire, SourcePin};

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let foreach_node = network.nodes.get_mut(&foreach_id).unwrap();
    let body = foreach_node.zone_mut().unwrap();
    let body_node = body.nodes.get_mut(&body_node_id).unwrap();
    body_node.arguments[body_param_index]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: foreach_id,
            source_pin: SourcePin::ZoneInput { pin_index: 0 },
            source_scope_depth: 1,
        });
}

fn wire_foreach_body_node_to_zone_output(
    designer: &mut StructureDesigner,
    network_name: &str,
    foreach_id: u64,
    body_node_id: u64,
) {
    use rust_lib_flutter_cad::structure_designer::node_network::{
        Argument, IncomingWire, SourcePin,
    };

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let foreach_node = network.nodes.get_mut(&foreach_id).unwrap();
    if foreach_node.zone_output_arguments.is_empty() {
        foreach_node.zone_output_arguments.push(Argument::new());
    }
    foreach_node.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node_id,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

// ============================================================================
// foreach perf property — display pass costs zero, even with large iterators
// ============================================================================

#[test]
fn foreach_skipped_on_display_pass_does_not_pull_iterator() {
    // Wire a `range(0..1_000_000)` upstream of `foreach`. If the central
    // rule did NOT skip foreach on display passes, walker construction +
    // body evaluation per element would dominate test runtime. Under the
    // rule, neither input pin is touched — this test should complete
    // essentially instantly. We assert via `Unit` output and no observable
    // side effects.
    use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;

    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&range_id).unwrap();
        node.data = Box::new(RangeData {
            start: 0,
            step: 1,
            count: 1_000_000,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&foreach_id).unwrap();
        node.data = Box::new(ForeachData {
            input_type: DataType::Int,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    // Body: expr "elem * 2"; wire element → expr.elem and expr → out.
    let expr_id = add_expr_to_foreach_body(
        &mut designer,
        "main",
        foreach_id,
        "elem * 2",
        vec![("elem".to_string(), DataType::Int)],
    );
    wire_foreach_zone_input_to_body_node(&mut designer, "main", foreach_id, expr_id, 0);
    wire_foreach_body_node_to_zone_output(&mut designer, "main", foreach_id, expr_id);

    let started = std::time::Instant::now();
    let display = evaluate_with_execute(&designer, "main", foreach_id, false);
    let elapsed = started.elapsed();

    assert!(
        matches!(display, NetworkResult::Unit),
        "expected Unit on display"
    );
    // Generous bound — the central skip rule should make this microseconds,
    // but we leave a 100ms cushion for CI noise. Without the rule, draining
    // a million-element range would take seconds.
    assert!(
        elapsed.as_millis() < 100,
        "display pass took {:?} — central skip rule did not short-circuit foreach",
        elapsed
    );
}

#[test]
fn foreach_drains_all_elements_under_execute() {
    // A `range(0..5)` upstream of `foreach`; the body does some arithmetic
    // on the element. Under Execute, the walker is drained and the body
    // fires N times; the result is Unit.
    use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;

    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&range_id).unwrap();
        node.data = Box::new(RangeData {
            start: 0,
            step: 1,
            count: 5,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&foreach_id).unwrap();
        node.data = Box::new(ForeachData {
            input_type: DataType::Int,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    let expr_id = add_expr_to_foreach_body(
        &mut designer,
        "main",
        foreach_id,
        "elem + 1",
        vec![("elem".to_string(), DataType::Int)],
    );
    wire_foreach_zone_input_to_body_node(&mut designer, "main", foreach_id, expr_id, 0);
    wire_foreach_body_node_to_zone_output(&mut designer, "main", foreach_id, expr_id);

    let exec_result = evaluate_with_execute(&designer, "main", foreach_id, true);
    assert!(
        matches!(exec_result, NetworkResult::Unit),
        "foreach should yield Unit on Execute (got {})",
        exec_result.to_display_string()
    );
}

// ============================================================================
// foreach + export_xyz — the headline batch-export integration
// ============================================================================

/// Returns a list of XYZ filenames present in `dir`, sorted.
fn xyz_files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap_or_default())
        .filter(|n| n.ends_with(".xyz"))
        .collect();
    names.sort();
    names
}

#[test]
fn foreach_with_export_xyz_body_writes_n_files_under_execute() {
    // Phase 5 — zone-based version of the headline batch-export integration:
    // build a `foreach` whose inline zone body holds an `import_xyz` + a
    // path-templating `expr` + an `export_xyz`. The foreach drains the range
    // under Execute, fires the body per element, and one file lands per
    // upstream element.
    use rust_lib_flutter_cad::structure_designer::node_network::{
        Argument, IncomingWire, SourcePin,
    };
    use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
    use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;

    let tmp = TempDir::new().expect("tempdir");
    let tmp_dir: PathBuf = tmp.path().to_path_buf();
    let template = format!(
        "{}/out_${{idx}}.xyz",
        tmp_dir.to_str().unwrap().replace('\\', "/")
    );

    let mut designer = setup_designer_with_network("main");

    // Outer: range(0..3) → foreach.
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&range_id).unwrap();
        node.data = Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&foreach_id).unwrap();
        node.data = Box::new(ForeachData {
            input_type: DataType::Int,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    // Build the zone body: import_xyz → export_xyz; path comes from an expr
    // that templates the per-element file name from the `element` zone-input.
    let mut import_atom = AtomicStructure::new();
    import_atom.add_atom(6, DVec3::new(0.0, 0.0, 0.0));

    let (import_id, expr_id, export_id) = {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let foreach_node = network.nodes.get_mut(&foreach_id).unwrap();
        let body = foreach_node.zone_mut().expect("foreach missing zone");

        // import_xyz pre-populated with one carbon (no file read).
        let mut import_data = ImportXYZData::default();
        import_data.atomic_structure = Some(import_atom);
        let import_id = body.add_node(
            "import_xyz",
            DVec2::new(-100.0, 0.0),
            1,
            Box::new(import_data),
        );

        // expr: outputs the templated path string.
        let mut expr_data = ExprData {
            parameters: vec![ExprParameter {
                id: None,
                name: "idx".to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            }],
            expression: format!("`{}`", template),
            expr: None,
            error: None,
            output_type: None,
        };
        let _ = expr_data.parse_and_validate(0);
        let expr_id = body.add_node("expr", DVec2::new(0.0, 100.0), 1, Box::new(expr_data));

        // export_xyz takes 2 inputs (molecule, file_name).
        let export_data = ExportXYZData::default();
        let export_id = body.add_node(
            "export_xyz",
            DVec2::new(100.0, 0.0),
            2,
            Box::new(export_data),
        );

        (import_id, expr_id, export_id)
    };

    // Populate caches for the new body nodes.
    for nid in [import_id, expr_id, export_id] {
        let registry = &mut designer.node_type_registry;
        let body_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&foreach_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&nid)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            body_node,
            true,
        );
    }

    // Wire the body: element → expr.idx; import → export.molecule; expr → export.file_name;
    // export → foreach.out.
    {
        let registry = &mut designer.node_type_registry;
        let foreach_node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&foreach_id)
            .unwrap();
        let body = foreach_node.zone_mut().unwrap();

        // expr.idx (param 0) ← foreach zone-input `element` (pin 0)
        body.nodes.get_mut(&expr_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: foreach_id,
                source_pin: SourcePin::ZoneInput { pin_index: 0 },
                source_scope_depth: 1,
            });
        // export.molecule (param 0) ← import (pin 0)
        body.nodes.get_mut(&export_id).unwrap().arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: import_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
        // export.file_name (param 1) ← expr (pin 0)
        body.nodes.get_mut(&export_id).unwrap().arguments[1]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: expr_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });

        // export → zone-output `out` (pin 0).
        if foreach_node.zone_output_arguments.is_empty() {
            foreach_node.zone_output_arguments.push(Argument::new());
        }
        foreach_node.zone_output_arguments[0]
            .incoming_wires
            .push(IncomingWire {
                source_node_id: export_id,
                source_pin: SourcePin::NodeOutput { pin_index: 0 },
                source_scope_depth: 0,
            });
    }

    // Display pass: no files should land.
    let display = evaluate_with_execute(&designer, "main", foreach_id, false);
    assert!(matches!(display, NetworkResult::Unit));
    assert!(
        xyz_files_in(&tmp_dir).is_empty(),
        "display pass must not produce any files; saw {:?}",
        xyz_files_in(&tmp_dir)
    );

    // Execute pass: 3 files appear.
    let exec = evaluate_with_execute(&designer, "main", foreach_id, true);
    assert!(
        matches!(exec, NetworkResult::Unit),
        "foreach should yield Unit on Execute (got {})",
        exec.to_display_string()
    );
    let names = xyz_files_in(&tmp_dir);
    assert_eq!(
        names,
        vec![
            "out_0.xyz".to_string(),
            "out_1.xyz".to_string(),
            "out_2.xyz".to_string()
        ],
        "execute pass should write one file per range element"
    );
}

// ============================================================================
// foreach error semantics — fail-fast on first body error
// ============================================================================

#[test]
fn foreach_body_error_halts_loop_and_surfaces_as_foreach_output() {
    // The body evaluates `10 / elem`, which surfaces an error when `elem == 0`.
    // The range starts at 0, so the very first body call errors and foreach
    // should fail fast.
    use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;

    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&range_id).unwrap();
        node.data = Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&foreach_id).unwrap();
        node.data = Box::new(ForeachData {
            input_type: DataType::Int,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    let expr_id = add_expr_to_foreach_body(
        &mut designer,
        "main",
        foreach_id,
        "10 / elem",
        vec![("elem".to_string(), DataType::Int)],
    );
    wire_foreach_zone_input_to_body_node(&mut designer, "main", foreach_id, expr_id, 0);
    wire_foreach_body_node_to_zone_output(&mut designer, "main", foreach_id, expr_id);

    let outcome = evaluate_with_execute(&designer, "main", foreach_id, true);
    match outcome {
        NetworkResult::Error(_) => {} // expected
        other => panic!(
            "foreach with first-element body error must surface as Error (got {})",
            other.to_display_string()
        ),
    }
}

#[test]
fn foreach_body_returning_non_error_value_is_discarded_as_unit() {
    // Body returns Int values — these are valid, non-Unit results. The
    // universal `T → Unit` widening at the body's `out` zone-output pin
    // means foreach silently discards them and emits Unit.
    use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;

    let mut designer = setup_designer_with_network("main");

    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&range_id).unwrap();
        node.data = Box::new(RangeData {
            start: 10,
            step: 1,
            count: 3,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    let foreach_id = designer.add_node("foreach", DVec2::new(200.0, 0.0));
    {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut("main").unwrap();
        let node = network.nodes.get_mut(&foreach_id).unwrap();
        node.data = Box::new(ForeachData {
            input_type: DataType::Int,
        });
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    designer.connect_nodes(range_id, 0, foreach_id, 0);

    let expr_id = add_expr_to_foreach_body(
        &mut designer,
        "main",
        foreach_id,
        "elem + 0",
        vec![("elem".to_string(), DataType::Int)],
    );
    wire_foreach_zone_input_to_body_node(&mut designer, "main", foreach_id, expr_id, 0);
    wire_foreach_body_node_to_zone_output(&mut designer, "main", foreach_id, expr_id);

    let outcome = evaluate_with_execute(&designer, "main", foreach_id, true);
    assert!(
        matches!(outcome, NetworkResult::Unit),
        "non-error body results must be discarded into Unit (got {})",
        outcome.to_display_string()
    );
}
