//! Tests for the Promote-to-Parameter operation.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn parameter_data<'a>(
    designer: &'a StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> Option<&'a ParameterData> {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)?;
    let node = network.nodes.get(&node_id)?;
    node.data.as_any_ref().downcast_ref::<ParameterData>()
}

// =============================================================================
// Happy path
// =============================================================================

#[test]
fn test_promote_float_node_creates_parameter_with_matching_type() {
    let mut designer = setup_designer("net");
    let float_id = designer.add_node("float", DVec2::new(100.0, 100.0));

    let new_id = designer
        .promote_node_to_parameter(float_id)
        .expect("promotion should succeed for float");

    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();

    // New parameter node exists with the right type.
    let param_node = network.nodes.get(&new_id).unwrap();
    assert_eq!(param_node.node_type_name, "parameter");

    let param = parameter_data(&designer, "net", new_id).unwrap();
    assert_eq!(param.data_type, DataType::Float);
    assert_eq!(param.param_index, 0);
    assert_eq!(param.sort_order, 0);

    // Float node is wired into the parameter's default input (pin 0).
    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();
    let param_node = network.nodes.get(&new_id).unwrap();
    let default_arg = &param_node.arguments[0];
    assert_eq!(default_arg.argument_output_pins().get(&float_id), Some(&0));
}

#[test]
fn test_promote_rewires_downstream_consumer() {
    let mut designer = setup_designer("net");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0); // sphere.radius <- float

    let param_id = designer.promote_node_to_parameter(float_id).unwrap();

    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();
    let sphere = network.nodes.get(&sphere_id).unwrap();
    let radius_arg = &sphere.arguments[0];

    // Sphere now reads from the parameter, not from the float directly.
    assert_eq!(radius_arg.argument_output_pins().get(&param_id), Some(&0));
    assert!(radius_arg.argument_output_pins().get(&float_id).is_none());
}

#[test]
fn test_promote_rewires_return_node_reference() {
    let mut designer = setup_designer("net");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    designer.set_return_node_id(Some(float_id));

    let param_id = designer.promote_node_to_parameter(float_id).unwrap();

    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();
    assert_eq!(network.return_node_id, Some(param_id));
}

#[test]
fn test_promote_appends_param_index_and_sort_order() {
    let mut designer = setup_designer("net");
    // Manually add a parameter first so the new one should land at index 1.
    let _first_param = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    let float_id = designer.add_node("float", DVec2::new(0.0, 100.0));

    let new_id = designer.promote_node_to_parameter(float_id).unwrap();
    let param = parameter_data(&designer, "net", new_id).unwrap();

    assert_eq!(param.param_index, 1);
    // The first parameter is added with sort_order=0 (see add_node parameter
    // special-case in structure_designer.rs), so the promoted one is 1.
    assert_eq!(param.sort_order, 1);
}

// =============================================================================
// Ineligibility
// =============================================================================

#[test]
fn test_promote_rejects_parameter_node() {
    let mut designer = setup_designer("net");
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));

    let err = designer.promote_node_to_parameter(param_id).unwrap_err();
    assert!(
        err.contains("already a parameter"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn test_promote_rejects_unknown_node_id() {
    let mut designer = setup_designer("net");
    let err = designer.promote_node_to_parameter(9999).unwrap_err();
    assert!(err.contains("Node not found"), "unexpected error: {}", err);
}

#[test]
fn test_promote_rejects_export_xyz_unit_output() {
    // export_xyz returns Unit â€” should be rejected as not parameterizable.
    let mut designer = setup_designer("net");
    let export_id = designer.add_node("export_xyz", DVec2::new(0.0, 0.0));

    let err = designer.promote_node_to_parameter(export_id).unwrap_err();
    assert!(
        err.contains("Unit") || err.contains("cannot be parameters"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn test_promote_iterator_output_creates_iter_parameter() {
    // `range` returns `Iter[Int]`. Promotion is now allowed (custom networks
    // already accept `Iter[T]` input pins; per-read independent walker clones
    // make it sound — see `doc/design_iterators.md` and
    // `promote_to_parameter.rs`). The new parameter must be typed `Iter[Int]`,
    // with `range` wired into its default.
    let mut designer = setup_designer("net");
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));

    let new_id = designer
        .promote_node_to_parameter(range_id)
        .expect("promotion should succeed for an Iter[T] output");

    let param = parameter_data(&designer, "net", new_id).unwrap();
    assert_eq!(
        param.data_type,
        DataType::Iterator(Box::new(DataType::Int)),
        "promoted parameter must carry the source's Iter[Int] type"
    );

    // range is wired into the parameter's default input (pin 0).
    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();
    let param_node = network.nodes.get(&new_id).unwrap();
    assert_eq!(
        param_node.arguments[0]
            .argument_output_pins()
            .get(&range_id),
        Some(&0),
        "the source iterator node must become the parameter's default"
    );
}

#[test]
fn test_promote_iterator_param_fans_out_to_independent_walkers() {
    // The soundness guarantee behind allowing `Iter[T]` parameters is Invariant
    // 2 (no pin-result memoization): each consumer re-evaluates the parameter
    // and gets a *fresh, independent* walker. Two `collect`s reading the same
    // promoted iterator parameter must therefore both drain the FULL stream —
    // one cannot starve the other. Regression guard for that invariant.
    let mut designer = setup_designer("net");

    // range[0:1:5] = [0, 1, 2, 3, 4].
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("net")
            .unwrap();
        let node = network.nodes.get_mut(&range_id).unwrap();
        node.data = Box::new(RangeData {
            start: 0,
            step: 1,
            count: 5,
        });
    }
    designer.validate_active_network();

    // Promote range -> Iter[Int] parameter P (default = range).
    let param_id = designer
        .promote_node_to_parameter(range_id)
        .expect("promotion should succeed");

    // Two independent collect consumers, both reading P.
    let collect_a = designer.add_node("collect", DVec2::new(200.0, -50.0));
    let collect_b = designer.add_node("collect", DVec2::new(200.0, 50.0));
    designer.connect_nodes(param_id, 0, collect_a, 0);
    designer.connect_nodes(param_id, 0, collect_b, 0);
    designer.validate_active_network();

    let eval = |id: u64| -> Vec<i32> {
        let registry = &designer.node_type_registry;
        let network = registry.node_networks.get("net").unwrap();
        let evaluator = NetworkEvaluator::new();
        let mut context = NetworkEvaluationContext::new();
        let stack = vec![NetworkStackElement {
            node_network: network,
            node_id: 0,
        }];
        match evaluator.evaluate(&stack, id, 0, registry, false, &mut context) {
            NetworkResult::Array(items) => items
                .into_iter()
                .map(|e| match e {
                    NetworkResult::Int(v) => v,
                    other => panic!("expected Int element, got {}", other.to_display_string()),
                })
                .collect(),
            other => panic!(
                "expected Array from collect, got {} (errors: {:?})",
                other.to_display_string(),
                context.node_errors.values().cloned().collect::<Vec<_>>()
            ),
        }
    };

    // Both consumers see the complete, independent stream.
    assert_eq!(eval(collect_a), vec![0, 1, 2, 3, 4]);
    assert_eq!(eval(collect_b), vec![0, 1, 2, 3, 4]);
}

// =============================================================================
// Undo / redo
// =============================================================================

#[test]
fn test_promote_undo_restores_original_network() {
    let mut designer = setup_designer("net");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);

    let node_count_before = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap()
        .nodes
        .len();

    designer.promote_node_to_parameter(float_id).unwrap();
    assert_eq!(
        designer
            .node_type_registry
            .node_networks
            .get("net")
            .unwrap()
            .nodes
            .len(),
        node_count_before + 1,
        "parameter node should have been added"
    );

    assert!(designer.undo());

    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();
    assert_eq!(network.nodes.len(), node_count_before);

    // Sphere's argument is rewired back to the float directly.
    let sphere = network.nodes.get(&sphere_id).unwrap();
    assert_eq!(
        sphere.arguments[0].argument_output_pins().get(&float_id),
        Some(&0)
    );
}

#[test]
fn test_promote_redo_reapplies() {
    let mut designer = setup_designer("net");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);

    let param_id = designer.promote_node_to_parameter(float_id).unwrap();
    assert!(designer.undo());
    assert!(designer.redo());

    let network = designer
        .node_type_registry
        .node_networks
        .get("net")
        .unwrap();

    // The parameter id is preserved by the snapshot.
    let param_node = network
        .nodes
        .get(&param_id)
        .expect("parameter node should be re-created with the same id after redo");
    assert_eq!(param_node.node_type_name, "parameter");

    let sphere = network.nodes.get(&sphere_id).unwrap();
    assert_eq!(
        sphere.arguments[0].argument_output_pins().get(&param_id),
        Some(&0)
    );
}
