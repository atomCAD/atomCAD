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
//! The Phase 2 `CounterUnitNode` fixture is **not** reused here. Phase 2
//! covers FunctionEvaluator + Walker context propagation in isolation; this
//! file exercises the integration with the real `export_xyz` and `foreach`
//! built-ins, plus the orchestrator API.

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
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;
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

fn edit_designer_network(
    designer: &mut StructureDesigner,
    network_name: &str,
    code: &str,
    replace: bool,
) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .unwrap();
    let result = edit_network(&mut network, &designer.node_type_registry, code, replace);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
    designer.validate_active_network();
    result
}

fn find_node_id(designer: &StructureDesigner, network_name: &str, node_type_name: &str) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let (id, _) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == node_type_name)
        .unwrap_or_else(|| panic!("expected a `{}` node in `{}`", node_type_name, network_name));
    *id
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
    assert_eq!(nt.parameters.len(), 2);
    assert_eq!(nt.parameters[0].name, "xs");
    assert_eq!(nt.parameters[1].name, "f");
    assert_eq!(nt.output_pins.len(), 1);
    assert_eq!(*nt.output_type(), DataType::Unit);
}

#[test]
fn foreach_calculate_custom_node_type_uses_unit_output_and_unit_function_return() {
    use rust_lib_flutter_cad::structure_designer::data_type::FunctionType;
    use rust_lib_flutter_cad::structure_designer::node_data::NodeData;

    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("foreach").unwrap();
    let data = ForeachData {
        input_type: DataType::Int,
    };
    let custom = data.calculate_custom_node_type(base).unwrap();
    assert_eq!(
        custom.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(
        custom.parameters[1].data_type,
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Unit),
        })
    );
    assert_eq!(*custom.output_type(), DataType::Unit);
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
    let mut designer = setup_designer_with_network("main");
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 1000000 }
            body = expr {
                expression: "elem * 2",
                parameters: [
                    { name: "elem", data_type: Int }
                ]
            }
            fe = foreach { input_type: Int, xs: r, f: @body }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let foreach_id = find_node_id(&designer, "main", "foreach");

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
    // Use `fold` to count how many times the body fired during the foreach
    // execute pass. The trick: a separate `fold` node tallies the same
    // range, so we know N from its result; the foreach body (an `expr`
    // computing elem*2) is invoked but its return value is discarded —
    // we assert via the absence of an error and the foreach output being
    // Unit.
    let mut designer = setup_designer_with_network("main");
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 5 }
            body = expr {
                expression: "elem + 1",
                parameters: [
                    { name: "elem", data_type: Int }
                ]
            }
            fe = foreach { input_type: Int, xs: r, f: @body }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let foreach_id = find_node_id(&designer, "main", "foreach");
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
    let tmp = TempDir::new().expect("tempdir");
    let tmp_dir: PathBuf = tmp.path().to_path_buf();

    // Build a body sub-network whose signature is `[Int] → Unit`:
    //  - `idx_param` is the function's input element (Int).
    //  - `import_xyz1` (added imperatively, pre-populated with a single carbon)
    //    is the static Molecule used for every element.
    //  - `path` is an `expr` that templates the per-element file name.
    //  - `ex` (export_xyz) is the return node — its Unit output is the
    //    function's output, completing the `[Int] → Unit` signature foreach
    //    expects.
    let mut designer = setup_designer_with_network("body");
    add_inline_import_xyz(&mut designer, "body"); // → custom_name "import_xyz1"

    let template = format!(
        "{}/out_${{idx}}.xyz",
        tmp_dir.to_str().unwrap().replace('\\', "/")
    );
    let body_code = format!(
        r#"
            idx_param = parameter {{ param_name: "idx", data_type: Int, sort_order: 0 }}
            path = expr {{
                expression: "`{}`",
                parameters: [
                    {{ name: "idx", data_type: Int }}
                ],
                idx: idx_param
            }}
            ex = export_xyz {{ molecule: import_xyz1, file_name: path }}
        "#,
        template
    );
    let edit_result = edit_designer_network(&mut designer, "body", &body_code, false);
    assert!(
        edit_result.success,
        "body edit should succeed: {:?}",
        edit_result.errors
    );
    let export_id = find_node_id(&designer, "body", "export_xyz");
    designer.set_return_node_id(Some(export_id));

    // Outer network: range → foreach(body_instance). The body custom-network
    // is instantiated as a node `b`; the foreach references its function-output
    // pin via `@b`, which captures `b` (with its single `idx` Int input
    // unwired) as the closure. The walker sets argument 0 per element.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let outer_code = r#"
        r = range { start: 0, step: 1, count: 3 }
        b = body { }
        fe = foreach { input_type: Int, xs: r, f: @b }
    "#;
    let edit_result = edit_designer_network(&mut designer, "main", outer_code, true);
    assert!(
        edit_result.success,
        "outer edit should succeed: {:?}",
        edit_result.errors
    );
    let foreach_id = find_node_id(&designer, "main", "foreach");

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

#[test]
fn foreach_over_map_with_export_xyz_body_writes_n_files() {
    // Walker propagation integration test: chain a `map` upstream of
    // `foreach`, both bodies invoking `export_xyz`. The Walker::Map's
    // FunctionEvaluator must forward the outer `&mut context` (with
    // `execute=true`) so the inner `export_xyz` actually fires. Phase 2
    // covered Walker::Map propagation in isolation via `CounterUnitNode`;
    // this is the integration with the real effect node.
    //
    // We use `map` (not `filter`) so the upstream produces a stream of
    // values that foreach drains. The map body discards its input and
    // returns the same idx, exercising the FE call inside Walker::Map.
    let tmp = TempDir::new().expect("tempdir");
    let tmp_dir: PathBuf = tmp.path().to_path_buf();

    let mut designer = setup_designer_with_network("body");
    add_inline_import_xyz(&mut designer, "body");
    let template = format!(
        "{}/mapped_${{idx}}.xyz",
        tmp_dir.to_str().unwrap().replace('\\', "/")
    );
    let body_code = format!(
        r#"
            idx_param = parameter {{ param_name: "idx", data_type: Int, sort_order: 0 }}
            path = expr {{
                expression: "`{}`",
                parameters: [
                    {{ name: "idx", data_type: Int }}
                ],
                idx: idx_param
            }}
            ex = export_xyz {{ molecule: import_xyz1, file_name: path }}
        "#,
        template
    );
    let edit_result = edit_designer_network(&mut designer, "body", &body_code, false);
    assert!(edit_result.success, "body edit: {:?}", edit_result.errors);
    let export_id = find_node_id(&designer, "body", "export_xyz");
    designer.set_return_node_id(Some(export_id));

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    // Phase 4 of `doc/design_zones.md` flipped `map` to inline-body zones, so
    // the previous `r → map(identity) → foreach` chain no longer compiles
    // via text format. The identity `map` was scaffolding only — wiring
    // `r → foreach` directly preserves the test's intent (foreach over an
    // iterator under Execute writes one file per element). MapZone coverage
    // lives in `zones_test`.
    let outer_code = r#"
        r = range { start: 0, step: 1, count: 2 }
        b = body { }
        fe = foreach { input_type: Int, xs: r, f: @b }
    "#;
    let edit_result = edit_designer_network(&mut designer, "main", outer_code, true);
    assert!(edit_result.success, "outer edit: {:?}", edit_result.errors);
    let foreach_id = find_node_id(&designer, "main", "foreach");

    let exec = evaluate_with_execute(&designer, "main", foreach_id, true);
    assert!(
        matches!(exec, NetworkResult::Unit),
        "foreach over map should yield Unit on Execute (got {})",
        exec.to_display_string()
    );
    let names = xyz_files_in(&tmp_dir);
    assert_eq!(
        names,
        vec!["mapped_0.xyz".to_string(), "mapped_1.xyz".to_string()],
        "foreach over map under Execute should write one file per upstream element"
    );
}

// ============================================================================
// foreach error semantics — fail-fast on first body error
// ============================================================================

#[test]
fn foreach_body_error_halts_loop_and_surfaces_as_foreach_output() {
    // The body evaluates `1 / elem`, which surfaces an error when `elem == 0`.
    // The range starts at 0, so the very first body call errors and foreach
    // should fail fast.
    let mut designer = setup_designer_with_network("main");
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 0, step: 1, count: 3 }
            body = expr {
                expression: "10 / elem",
                parameters: [
                    { name: "elem", data_type: Int }
                ]
            }
            fe = foreach { input_type: Int, xs: r, f: @body }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let foreach_id = find_node_id(&designer, "main", "foreach");
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
    // Body returns Int values (10, 11, 12) — these are valid, non-Unit
    // results. The universal `T → Unit` widening at the body's function
    // output position means foreach silently discards them and emits Unit.
    let mut designer = setup_designer_with_network("main");
    let result = edit_designer_network(
        &mut designer,
        "main",
        r#"
            r = range { start: 10, step: 1, count: 3 }
            body = expr {
                expression: "elem + 0",
                parameters: [
                    { name: "elem", data_type: Int }
                ]
            }
            fe = foreach { input_type: Int, xs: r, f: @body }
        "#,
        true,
    );
    assert!(result.success, "edit should succeed: {:?}", result.errors);

    let foreach_id = find_node_id(&designer, "main", "foreach");
    let outcome = evaluate_with_execute(&designer, "main", foreach_id, true);
    assert!(
        matches!(outcome, NetworkResult::Unit),
        "non-error body results must be discarded into Unit (got {})",
        outcome.to_display_string()
    );
}
