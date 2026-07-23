//! Regression tests for an `apply` node whose `f` pin is wired to a custom
//! network's *function pin* (`-1`), where that network takes an `Iter[T]`
//! parameter.
//!
//! Two bugs were fixed together (see the project memory / PR that introduced
//! this fixture):
//!
//! 1. **Dropped arg wire on load.** `apply`'s arg-pin layout (`arg0`, …) is
//!    derived from the wired `f` source, not from per-node data, so the
//!    load-path rebuilds that reset `apply` to its bare `[f]` layout silently
//!    truncated the freshly-deserialized `arg0` wire before the currying
//!    post-pass could re-derive the layout. Fixed by preserving `apply`'s
//!    arguments positionally in `repair_node_network` / `validate_network` and
//!    running the post-pass before `repair_network_arguments`.
//!
//! 2. **`Iterator` broadcast as a single element.** An `Iter[T]` value flowing
//!    through a `ZoneInput` whose owning node isn't an HOF (a custom network
//!    used through its function pin) resolved to a `None` source type
//!    (`infer_data_type` has no `Iterator` arm), so `convert_to(_, Iter[T])`
//!    wrapped the whole iterator into a one-element stream. The downstream
//!    `map` then saw the entire iterator as its single element, producing
//!    "Arithmetic operation not supported for these types". Fixed by skipping
//!    the broadcast when the runtime value is itself an `Iterator`.
//!
//! The fixture mirrors a user's `Main`/`mynet` design: `range[0:1:10]` is
//! applied through `mynet` (which maps `x + val`, `val = 4`) and `collect`ed,
//! expecting `[4, 5, …, 13]`.

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

const FIXTURE: &str = "tests/fixtures/apply_function_pin/apply_over_custom_network_iter.cnnd";

fn only_node_id(network: &NodeNetwork, node_type_name: &str) -> u64 {
    let ids: Vec<u64> = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == node_type_name)
        .map(|n| n.id)
        .collect();
    assert_eq!(
        ids.len(),
        1,
        "expected exactly one '{}' node, got {:?}",
        node_type_name,
        ids
    );
    ids[0]
}

fn load_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer
        .load_node_networks(FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {}", e));
    designer
}

#[test]
fn apply_arg_wire_survives_load() {
    let designer = load_designer();
    let main = designer
        .node_type_registry
        .node_networks
        .get("Main")
        .expect("Main network");

    let apply_id = only_node_id(main, "apply");
    let apply = main.nodes.get(&apply_id).unwrap();

    // The `apply` node's `f` pin (index 0) is wired to mynet's function pin,
    // and its `arg0` pin (index 1) is wired to `range`. Before the fix, index 1
    // was truncated away on load.
    assert_eq!(
        apply.arguments.len(),
        2,
        "apply should have f + arg0 pins after load"
    );
    assert_eq!(
        apply.arguments[1].incoming_wires.len(),
        1,
        "the range -> apply.arg0 wire must survive load (regression: it was dropped)"
    );
}

#[test]
fn collect_over_applied_custom_network_iter() {
    let designer = load_designer();
    let registry = &designer.node_type_registry;
    let main = registry.node_networks.get("Main").expect("Main network");

    let collect_id = only_node_id(main, "collect");

    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: main,
        node_id: 0,
    }];
    let result = evaluator.evaluate(&network_stack, collect_id, 0, registry, false, &mut context);

    let elements = match result {
        NetworkResult::Array(items) => items,
        other => panic!(
            "expected Array from collect, got {} (node errors: {:?})",
            other.to_display_string(),
            context.node_errors.values().cloned().collect::<Vec<_>>()
        ),
    };

    let values: Vec<i32> = elements
        .iter()
        .map(|e| match e {
            NetworkResult::Int(i) => *i,
            other => panic!("expected Int element, got {}", other.to_display_string()),
        })
        .collect();

    // range[0:1:10] = 0..=9, each mapped through mynet as `x + val` with val=4.
    assert_eq!(values, vec![4i32, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
}
