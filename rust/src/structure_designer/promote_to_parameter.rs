//! Promote-to-parameter operation.
//!
//! Given a node X in a network, creates a `parameter` node P typed after
//! X's output pin 0 (resolved), wires X.out[0] into P's default input, and
//! rewires every existing consumer of X.out[0] (including a possible
//! return-node reference) to read from P.out[0] instead. After the operation,
//! X serves as P's default value provider.

use glam::f64::DVec2;

use super::data_type::DataType;
use super::node_data::NodeData;
use super::node_network::{Argument, Node, NodeNetwork};
use super::node_type_registry::NodeTypeRegistry;
use super::nodes::parameter::ParameterData;

/// Horizontal offset from the source node where the new parameter is placed.
/// Chosen so the left-to-right reading order remains source → parameter →
/// downstream consumers.
const PROMOTED_PARAM_X_OFFSET: f64 = 220.0;

/// Resolves the eligible data type for promoting the given node's pin 0,
/// or returns an error explaining why the node cannot be promoted.
pub fn check_promote_eligibility(
    network: &NodeNetwork,
    node_id: u64,
    registry: &NodeTypeRegistry,
) -> Result<DataType, String> {
    let node = network
        .nodes
        .get(&node_id)
        .ok_or_else(|| "Node not found".to_string())?;

    if node.node_type_name == "parameter" {
        return Err("Node is already a parameter".to_string());
    }

    let resolved = registry
        .resolve_output_type(node, network, 0)
        .ok_or_else(|| {
            "Cannot promote: output type of pin 0 cannot be resolved (input may be disconnected)"
                .to_string()
        })?;

    if resolved.is_abstract() {
        return Err("Cannot promote: output type is abstract".to_string());
    }
    match resolved {
        DataType::Function(_) => Err("Cannot promote: functions cannot be parameters".to_string()),
        DataType::Unit => Err("Cannot promote: Unit values cannot be parameters".to_string()),
        // `Iter[T]` parameters are allowed. Custom networks already accept
        // `Iter[T]` input pins, and per-read independent walker clones
        // (Invariant 2, no pin-result memoization) make multi-consumer fan-out
        // well-defined — see `doc/design_iterators.md` (Invariant 3: the
        // top-level-parameter restriction is deliberate v1 conservatism, not a
        // soundness issue, and is enforced at the CLI/API *binding* layer). The
        // genuine aliasing hazard — capturing an `Iter[T]` into a closure — is
        // a separate rule enforced by the network validator regardless of how
        // the iterator value is produced, so promotion does not weaken it.
        DataType::None => Err("Cannot promote: invalid type".to_string()),
        t => Ok(t),
    }
}

/// Performs the promote operation in-place on `network`.
///
/// On success returns the new parameter node's id. On any error, the network
/// is left unchanged.
pub fn promote_node_to_parameter(
    network: &mut NodeNetwork,
    node_id: u64,
    registry: &NodeTypeRegistry,
) -> Result<u64, String> {
    let data_type = check_promote_eligibility(network, node_id, registry)?;

    let source_pos = network.nodes.get(&node_id).unwrap().position;
    let param_position = DVec2::new(source_pos.x + PROMOTED_PARAM_X_OFFSET, source_pos.y);

    let existing_param_count = network
        .nodes
        .values()
        .filter(|n| n.node_type_name == "parameter")
        .count();
    let max_sort_order = network
        .nodes
        .values()
        .filter_map(|n| n.data.as_any_ref().downcast_ref::<ParameterData>())
        .map(|d| d.sort_order)
        .max();
    let sort_order = max_sort_order.map(|s| s + 1).unwrap_or(0);

    let param_index = existing_param_count;
    let param_name = format!("param{}", param_index);
    let param_id = network.next_param_id;
    network.next_param_id += 1;

    let param_data = ParameterData {
        param_id: Some(param_id),
        param_index,
        param_name: param_name.clone(),
        data_type,
        sort_order,
        data_type_str: None,
        error: None,
    };

    let new_id = network.next_node_id;
    network.next_node_id += 1;

    // Pre-compute the parameter node's custom_node_type (whose pin types
    // reflect `data_type`). Setting it on the node up front prevents a
    // later `populate_custom_node_type_cache_with_types` call from
    // rebuilding the arguments array — which would silently drop the
    // wire we add below.
    let base_param_type = registry
        .get_node_type("parameter")
        .ok_or_else(|| "parameter node type not registered".to_string())?;
    let custom_type = param_data.calculate_custom_node_type(base_param_type);

    let mut param_node = Node {
        id: new_id,
        node_type_name: "parameter".to_string(),
        custom_name: Some(param_name),
        position: param_position,
        arguments: vec![Argument::new()],
        data: Box::new(param_data),
        custom_node_type: None,
        zone: None,
        zone_output_arguments: Vec::new(),
        body_width: crate::structure_designer::node_network::DEFAULT_BODY_WIDTH,
        body_height: crate::structure_designer::node_network::DEFAULT_BODY_HEIGHT,
        collapse_mode: crate::structure_designer::node_network::CollapseMode::Auto,
    };
    param_node.set_custom_node_type(custom_type, false);
    network.nodes.insert(new_id, param_node);

    for (&consumer_id, consumer) in network.nodes.iter_mut() {
        if consumer_id == new_id {
            continue;
        }
        for arg in &mut consumer.arguments {
            if let Some(pin_idx) = arg.get_source_pin(node_id)
                && pin_idx == 0
            {
                arg.remove_source(node_id);
                arg.set_source(new_id, 0);
            }
        }
    }

    if network.return_node_id == Some(node_id) {
        network.return_node_id = Some(new_id);
    }

    network.connect_nodes(node_id, 0, new_id, 0, false);

    Ok(new_id)
}
