use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_network::{
    Argument, IncomingWire, NodeNetwork, SourcePin, ValidationError, function_input_pin_connected,
};
use crate::structure_designer::node_type::{OutputPinDefinition, Parameter, PinOutputType};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::nodes::parameter::ParameterData;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

/// Per-validation-run cache of resolved concrete output types, keyed by
/// `(node_id, output_pin_index)`. A `None` entry means "we tried to resolve
/// and failed" (unresolved — treated as disconnected downstream).
#[derive(Default)]
pub struct ValidationContext {
    resolved_outputs: HashMap<(u64, i32), Option<DataType>>,
}

impl ValidationContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve (with memoization) the concrete output type of `(node_id, pin_index)`.
    pub fn resolve(
        &mut self,
        network: &NodeNetwork,
        registry: &NodeTypeRegistry,
        node_id: u64,
        output_pin_index: i32,
    ) -> Option<DataType> {
        if let Some(cached) = self.resolved_outputs.get(&(node_id, output_pin_index)) {
            return cached.clone();
        }
        // Insert a tentative None to guard against infinite recursion on malformed
        // cyclic graphs; real cycles should be rejected elsewhere.
        self.resolved_outputs
            .insert((node_id, output_pin_index), None);
        let node = network.nodes.get(&node_id)?;
        let resolved = registry.resolve_output_type(node, network, output_pin_index);
        self.resolved_outputs
            .insert((node_id, output_pin_index), resolved.clone());
        resolved
    }
}

#[derive(Debug, Clone)]
pub struct NetworkValidationResult {
    pub valid: bool,
    pub interface_changed: bool,
}

/// Compares two parameters for deterministic sorting.
/// Primary sort key: sort_order (ascending)
/// Secondary sort key: node_id (ascending)
fn compare_parameters(
    node_id_a: u64,
    param_data_a: &ParameterData,
    node_id_b: u64,
    param_data_b: &ParameterData,
) -> Ordering {
    param_data_a
        .sort_order
        .cmp(&param_data_b.sort_order)
        .then_with(|| node_id_a.cmp(&node_id_b))
}

/// Repairs call sites when a network's parameter interface changes.
/// This function updates all nodes that use the given network as their type,
/// preserving argument connections based on parameter IDs (primary) or names (fallback).
fn repair_call_sites_for_network(
    network_name: &str,
    old_parameters: &[Parameter],
    new_parameters: &[Parameter],
    node_type_registry: &mut NodeTypeRegistry,
) {
    // Build mapping: parameter_id -> old_index (primary matching strategy)
    let old_param_id_map: HashMap<u64, usize> = old_parameters
        .iter()
        .enumerate()
        .filter_map(|(idx, param)| param.id.map(|id| (id, idx)))
        .collect();

    // Build mapping: parameter_name -> old_index (fallback for backwards compatibility)
    let old_param_name_map: HashMap<&str, usize> = old_parameters
        .iter()
        .enumerate()
        .map(|(idx, param)| (param.name.as_str(), idx))
        .collect();

    // Find all parent networks that use this network
    let parent_network_names = node_type_registry.find_parent_networks(network_name);

    // Update each parent network's call sites. Walk recursively into HOF
    // zone bodies so a body-internal node calling the renamed network has
    // its arguments fixed up too — `node_id` is per-network and can collide
    // across scopes, so we apply the update in place during the walk
    // rather than staging `(node_id, new_arguments)` pairs.
    for parent_name in parent_network_names {
        if let Some(parent_network) = node_type_registry.node_networks.get_mut(&parent_name) {
            crate::structure_designer::node_network::walk_all_nodes_mut(
                parent_network,
                &mut |node| {
                    if node.node_type_name != network_name {
                        return;
                    }
                    let mut new_arguments = Vec::with_capacity(new_parameters.len());
                    for new_param in new_parameters {
                        let old_idx = {
                            // First try ID-based matching (handles renames)
                            if let Some(new_id) = new_param.id {
                                if let Some(&idx) = old_param_id_map.get(&new_id) {
                                    Some(idx)
                                } else {
                                    // Fall back to name-based matching
                                    old_param_name_map.get(new_param.name.as_str()).copied()
                                }
                            } else {
                                // No ID, use name-based matching (backwards compatibility)
                                old_param_name_map.get(new_param.name.as_str()).copied()
                            }
                        };
                        if let Some(old_idx) = old_idx {
                            if old_idx < node.arguments.len() {
                                new_arguments.push(node.arguments[old_idx].clone());
                            } else {
                                new_arguments.push(Argument::new());
                            }
                        } else {
                            new_arguments.push(Argument::new());
                        }
                    }
                    node.arguments = new_arguments;
                },
            );
        }
    }
}

fn validate_parameters(network: &mut NodeNetwork) -> bool {
    // Collect all parameter nodes
    let mut parameter_nodes: Vec<(u64, &ParameterData)> = Vec::new();

    for (node_id, node) in &network.nodes {
        if node.node_type_name == "parameter" {
            // Cast node data to ParameterData
            if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>() {
                parameter_nodes.push((*node_id, param_data));
            } else {
                network.validation_errors.push(ValidationError::new(
                    "Parameter node has invalid data type".to_string(),
                    Some(*node_id),
                ));
                return false;
            }
        }
    }

    // Validate param_name uniqueness
    let mut param_names: HashMap<String, u64> = HashMap::new();
    for (node_id, param_data) in &parameter_nodes {
        if let Some(_existing_node_id) = param_names.get(&param_data.param_name) {
            network.validation_errors.push(ValidationError::new(
                format!("Duplicate parameter name '{}'", param_data.param_name),
                Some(*node_id),
            ));
            return false;
        } else {
            param_names.insert(param_data.param_name.clone(), *node_id);
        }
    }

    // Reject abstract parameter types: abstract types may only appear as declared
    // input-pin types on built-in polymorphic nodes, not on user-declared parameter pins.
    for (node_id, param_data) in &parameter_nodes {
        if contains_abstract(&param_data.data_type) {
            network.validation_errors.push(ValidationError::new(
                format!(
                    "Parameter '{}' has abstract type {:?}; abstract phase types are not allowed on parameter pins",
                    param_data.param_name, param_data.data_type
                ),
                Some(*node_id),
            ));
            return false;
        }
    }

    // Sort parameter nodes by sort_order (primary) and node_id (secondary)
    // This ensures deterministic ordering even when multiple parameters have the same sort_order
    parameter_nodes.sort_by(|(node_id_a, param_data_a), (node_id_b, param_data_b)| {
        compare_parameters(*node_id_a, param_data_a, *node_id_b, param_data_b)
    });

    // Recreate the parameters array based on sort order, propagating IDs for wire preservation
    network.node_type.parameters = parameter_nodes
        .iter()
        .map(|(_, param_data)| {
            Parameter {
                id: param_data.param_id, // Propagate ID for wire preservation across renames
                name: param_data.param_name.clone(),
                data_type: param_data.data_type.clone(),
            }
        })
        .collect();

    // Update param_index for each parameter node
    // Collect node IDs and their new indices to avoid borrowing conflicts
    let param_updates: Vec<(u64, usize)> = parameter_nodes
        .iter()
        .enumerate()
        .map(|(index, (node_id, _))| (*node_id, index))
        .collect();

    for (node_id, new_index) in param_updates {
        if let Some(node) = network.nodes.get_mut(&node_id) {
            if let Some(param_data) = (*node.data).as_any_mut().downcast_mut::<ParameterData>() {
                param_data.param_index = new_index;
            }
        }
    }

    true
}

fn check_interface_changed(network: &NodeNetwork) -> bool {
    // Collect current parameter nodes with their IDs for deterministic sorting
    let mut current_params_with_ids: Vec<(u64, &ParameterData)> = Vec::new();

    for (node_id, node) in &network.nodes {
        if node.node_type_name == "parameter" {
            if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>() {
                current_params_with_ids.push((*node_id, param_data));
            }
        }
    }

    // Sort by sort_order (primary) and node_id (secondary) for deterministic comparison
    current_params_with_ids.sort_by(|(node_id_a, param_data_a), (node_id_b, param_data_b)| {
        compare_parameters(*node_id_a, param_data_a, *node_id_b, param_data_b)
    });

    // Check if the interface changed by comparing with existing parameters
    if network.node_type.parameters.len() != current_params_with_ids.len() {
        return true;
    }

    current_params_with_ids
        .iter()
        .enumerate()
        .any(|(index, (_, param_data))| {
            if let Some(existing_param) = network.node_type.parameters.get(index) {
                existing_param.name != param_data.param_name
                    || existing_param.data_type != param_data.data_type
            } else {
                true
            }
        })
}

/// Repairs argument counts in the network to match parameter counts.
/// This ensures all nodes have the correct number of arguments for their type.
fn repair_network_arguments(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) {
    // Recurse into HOF zone bodies as well as the top-level nodes. The pin
    // *layout* post-passes (`update_apply_pin_layouts_for_network` /
    // `update_map_pin_layouts_for_network`) already recurse into bodies and
    // derive `[f, arg0, …]` there, but they install it with the preserving-
    // args variant (`refresh_args = false`) so they never grow the
    // `arguments` vector themselves. Growing/truncating `arguments` to match
    // the pin count is *this* function's job; doing it only on `network.nodes`
    // left a body `apply`/instance with `parameters.len() > arguments.len()`,
    // so connection gating (`can_connect_nodes`, which indexes `arguments`)
    // rejected every wire into the extra pins. See
    // `https://github.com/atomCAD/atomCAD/issues/331` and the
    // "bare `network.nodes` walk skips body nodes" note in
    // `structure_designer/AGENTS.md`.
    crate::structure_designer::node_network::walk_all_nodes_mut(network, &mut |node| {
        // `get_node_type_for_node` borrows from `node`, so extract the count
        // before mutating `node.arguments`.
        let Some(expected_count) = node_type_registry
            .get_node_type_for_node(node)
            .map(|nt| nt.parameters.len())
        else {
            return;
        };
        let current_count = node.arguments.len();
        match current_count.cmp(&expected_count) {
            Ordering::Less => {
                // Add empty arguments when too few.
                for _ in current_count..expected_count {
                    node.arguments.push(Argument::new());
                }
            }
            Ordering::Greater => {
                // Remove excess arguments when too many.
                node.arguments.truncate(expected_count);
            }
            Ordering::Equal => {}
        }
    });
}

/// Removes wire connections that reference output pins that no longer exist on the source node.
/// This handles the case where a custom network's return node changes from multi-output to
/// single-output, leaving dangling wires to pins that were removed.
///
/// Recurses into every HOF/closure zone body. `pin_counts` is rebuilt per
/// network/body because intra-scope (`source_scope_depth == 0`) wires reference
/// nodes in the *same* network, and `node_id` is only unique within one network
/// (a body node and a top-level node can share an id). Without the recursion a
/// dangling wire to a removed output pin survived inside a body — the sibling of
/// issue #331's `repair_network_arguments` body-skip; see the "bare
/// `network.nodes` walk skips body nodes" note in `structure_designer/AGENTS.md`.
fn repair_output_pin_wires(network: &mut NodeNetwork, node_type_registry: &NodeTypeRegistry) {
    // First pass: build a map of node_id -> output_pin_count for THIS network's
    // own nodes.
    let pin_counts: HashMap<u64, usize> = network
        .nodes
        .iter()
        .filter_map(|(&node_id, node)| {
            node_type_registry
                .get_node_type_for_node(node)
                .map(|nt| (node_id, nt.output_pin_count()))
        })
        .collect();

    // Second pass: remove wires to non-existent output pins, then recurse into
    // each node's zone body (which resolves its own intra-scope wires against
    // its own `pin_counts`).
    for node in network.nodes.values_mut() {
        for argument in node.arguments.iter_mut() {
            argument.incoming_wires.retain(|wire| {
                let Some((source_node_id, output_pin_index)) = wire.as_legacy_pair() else {
                    // ZoneInput or non-zero scope_depth wires aren't tied to
                    // a regular-output pin count; leave them to later
                    // zone-aware validation (Phase 6).
                    return true;
                };
                // The function pin (`-1`) is not a regular result pin and is
                // not counted by `output_pin_count()`; it always exists on a
                // non-HOF node. Preserve it here and let `validate_wires`
                // type-check it via `get_function_type()`
                // (doc/design_function_pins.md). Without this guard `-1 as
                // usize` is a huge value `>= count`, so the wire would be
                // silently stripped on every `.cnnd` load / validation pass.
                if output_pin_index < 0 {
                    return true;
                }
                if let Some(&count) = pin_counts.get(&source_node_id) {
                    (output_pin_index as usize) < count
                } else {
                    true // Unknown source — let validate_wires catch it
                }
            });
        }
        if let Some(body) = node.zone_mut() {
            repair_output_pin_wires(body, node_type_registry);
        }
    }
}

/// Returns true if `t` is itself abstract or contains an abstract type inside
/// an `Array[..]` wrapper. Used for guards on user-declared type fields
/// (parameter pins, sequence element_type) where abstract is always invalid.
fn contains_abstract(t: &DataType) -> bool {
    match t {
        _ if t.is_abstract() => true,
        DataType::Array(inner) => contains_abstract(inner),
        // `AnyFunction` is a structural acceptance constraint on input pins,
        // not an abstract phase type — match the `DataType::is_abstract`
        // policy and return false uniformly. (Built-in nodes use
        // `AnyFunction` for `apply.f` / `map.f`; user-declared parameter
        // types cannot select it through the UI.) See
        // `doc/design_function_pin_unification.md` Phase A.
        DataType::AnyFunction { .. } => false,
        _ => false,
    }
}

fn validate_wires(
    network: &mut NodeNetwork,
    node_type_registry: &NodeTypeRegistry,
    ctx: &mut ValidationContext,
) -> bool {
    // Validate wires - pure checking, no repairs
    for (dest_node_id, dest_node) in &network.nodes {
        // Check if this node references a node network and validate its validity
        if let Some(referenced_network) = node_type_registry
            .node_networks
            .get(&dest_node.node_type_name)
        {
            if !referenced_network.valid {
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "References invalid node network '{}'",
                        dest_node.node_type_name
                    ),
                    Some(*dest_node_id),
                ));
                return false;
            }
        }

        // Get the destination node type to access parameter information
        let dest_node_type = match node_type_registry.get_node_type_for_node(dest_node) {
            Some(node_type) => node_type,
            None => {
                network.validation_errors.push(ValidationError::new(
                    format!("Unknown node type '{}'", dest_node.node_type_name),
                    Some(*dest_node_id),
                ));
                return false;
            }
        };

        // Validate argument count matches parameter count
        // (This should always pass after repair phase)
        if dest_node.arguments.len() != dest_node_type.parameters.len() {
            network.validation_errors.push(ValidationError::new(
                format!(
                    "Node has {} arguments but type expects {} parameters",
                    dest_node.arguments.len(),
                    dest_node_type.parameters.len()
                ),
                Some(*dest_node_id),
            ));
            return false;
        }

        // Validate each argument (input pin) of the destination node
        for (arg_index, argument) in dest_node.arguments.iter().enumerate() {
            // Get parameter information for this argument
            let parameter = &dest_node_type.parameters[arg_index];

            // Validate non-multi input pins have at most one connection
            if !parameter.data_type.is_array() && argument.len() > 1 {
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "Non-multi parameter '{}' has {} connections, but only 1 is allowed",
                        parameter.name,
                        argument.len()
                    ),
                    Some(*dest_node_id),
                ));
                return false;
            }

            // Validate data types for each connected source node
            for incoming in &argument.incoming_wires {
                let source_node_id = &incoming.source_node_id;
                let output_pin_index = match incoming.source_pin {
                    crate::structure_designer::node_network::SourcePin::NodeOutput {
                        pin_index,
                    } => pin_index,
                    // Zone-input sources (later phases) aren't validated here.
                    crate::structure_designer::node_network::SourcePin::ZoneInput { .. } => {
                        continue;
                    }
                };
                let output_pin_index = &output_pin_index;
                // Get the source node
                let source_node = match network.nodes.get(source_node_id) {
                    Some(node) => node,
                    None => {
                        network.validation_errors.push(ValidationError::new(
                            "Wire references non-existent source node".to_string(),
                            Some(*dest_node_id),
                        ));
                        return false;
                    }
                };

                // Check if this source node references a node network and validate its validity
                if let Some(referenced_network) = node_type_registry
                    .node_networks
                    .get(&source_node.node_type_name)
                {
                    if !referenced_network.valid {
                        network.validation_errors.push(ValidationError::new(
                            format!(
                                "Source node references invalid node network '{}'",
                                source_node.node_type_name
                            ),
                            Some(*source_node_id),
                        ));
                        return false;
                    }
                }

                // Get the source node type to access its output type
                let _source_node_type = match node_type_registry.get_node_type_for_node(source_node)
                {
                    Some(node_type) => node_type,
                    None => {
                        network.validation_errors.push(ValidationError::new(
                            format!("Unknown source node type '{}'", source_node.node_type_name),
                            Some(*source_node_id),
                        ));
                        return false;
                    }
                };

                // Validate data type compatibility using the resolved concrete
                // source type. If resolution fails (unresolved polymorphic
                // output upstream), treat the wire as disconnected — the
                // upstream node itself is flagged invalid below.
                let source_data_type = match ctx.resolve(
                    network,
                    node_type_registry,
                    *source_node_id,
                    *output_pin_index,
                ) {
                    Some(t) => t,
                    None => continue,
                };

                let dest_data_type =
                    node_type_registry.get_node_param_data_type(dest_node, arg_index);
                if !DataType::can_be_converted_to(
                    &source_data_type,
                    &dest_data_type,
                    node_type_registry,
                ) {
                    network.validation_errors.push(ValidationError::new(
                        format!(
                            "Data type mismatch: input expects {:?}, but source outputs {:?}",
                            parameter.data_type, source_data_type
                        ),
                        Some(*dest_node_id),
                    ));
                    return false;
                }
            }

            // Note: a direct "abstract input pin unconnected → invalid" check
            // is subsumed by the polymorphic-output-unresolved check below
            // once a node's outputs are migrated to `SameAsInput` /
            // `SameAsArrayElements`. Not-yet-migrated nodes still declare
            // `Fixed(Atomic)` on their outputs, and enforcing the rule on
            // their abstract input pins directly would flag existing valid
            // graphs invalid before migration lands. The uniform rule is
            // applied via the output-resolution check below.
        }

        // Polymorphic output pins must resolve to a concrete type. If any
        // output is unresolved, the node is flagged invalid. This is the
        // uniform rule that covers both single-input SameAsInput pins
        // (disconnected input) and SameAsArrayElements pins (mixed phases,
        // empty arrays, upstream unresolved).
        for pin_index_usize in 0..dest_node_type.output_pin_count() {
            let pin_index = pin_index_usize as i32;
            let pin = &dest_node_type.output_pins[pin_index_usize];
            let is_polymorphic = !matches!(pin.data_type, PinOutputType::Fixed(_));
            if !is_polymorphic {
                continue;
            }
            if ctx
                .resolve(network, node_type_registry, *dest_node_id, pin_index)
                .is_none()
            {
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "Output pin '{}' ({}) could not be resolved to a concrete type",
                        pin.name, pin.data_type
                    ),
                    Some(*dest_node_id),
                ));
                return false;
            }
        }

        // Defensive rule: an output pin's resolved type must never be
        // `AnyFunction`. Built-in nodes don't declare it on outputs
        // (the registry-build-time debug assertion in `NodeTypeRegistry::add_node_type`
        // catches authoring mistakes), and no `SameAsInput` / `SameAsArrayElements`
        // pin can resolve to `AnyFunction` either (sources always carry a fully
        // -specified `Function`). This is here so a stray hand-edited fixture
        // can't sneak it past the type checker. See
        // `doc/design_function_pin_unification.md` Phase A.
        for pin_index_usize in 0..dest_node_type.output_pin_count() {
            let pin_index = pin_index_usize as i32;
            let resolved = ctx.resolve(network, node_type_registry, *dest_node_id, pin_index);
            if let Some(t) = resolved {
                if matches!(t, DataType::AnyFunction { .. }) {
                    let pin = &dest_node_type.output_pins[pin_index_usize];
                    network.validation_errors.push(ValidationError::new(
                        format!(
                            "Output pin '{}' resolves to `AnyFunction`; \
                             `AnyFunction` is an input-pin-only type",
                            pin.name
                        ),
                        Some(*dest_node_id),
                    ));
                    return false;
                }
            }
        }
    }

    true
}

pub fn validate_network(
    network: &mut NodeNetwork,
    node_type_registry: &mut NodeTypeRegistry,
    initial_errors: Option<Vec<crate::structure_designer::node_network::ValidationError>>,
) -> NetworkValidationResult {
    // Clear previous validation state
    network.valid = true;
    network.validation_errors.clear();

    // Add initial errors first if provided
    if let Some(errors) = initial_errors {
        for error in errors {
            network.validation_errors.push(error);
            network.valid = false;
        }
    }

    // Check if interface changed before validation (to detect changes)
    let interface_changed = check_interface_changed(network);

    // Store old parameters before updating them
    let old_parameters = network.node_type.parameters.clone();

    // Validate parameters (this updates parameter order and indices)
    if !validate_parameters(network) {
        network.valid = false;
        return NetworkValidationResult {
            valid: false,
            interface_changed,
        };
    }

    // REPAIR PHASE: Update call sites if interface changed
    if interface_changed {
        let new_parameters = network.node_type.parameters.clone();
        let network_name = network.node_type.name.clone();
        repair_call_sites_for_network(
            &network_name,
            &old_parameters,
            &new_parameters,
            node_type_registry,
        );
    }

    // REPAIR PHASE: Currying Phase 3 (`doc/design_currying.md`). For every
    // `apply` node whose `f` pin is wired, override the node's
    // `custom_node_type` from the wired source's declared (canonical, flat)
    // function type — so the f pin's type matches the source, the arg pin
    // count equals `N` (the source's flat arity), and the output pin type
    // reflects partial application (`k < N`) or full evaluation (`k == N`).
    // Must run BEFORE `validate_wires` so the type checks see the up-to-date
    // pin types; idempotent so re-running on a steady state is a no-op.
    //
    // Runs BEFORE `repair_network_arguments` (which would otherwise truncate a
    // freshly-loaded `apply` to its bare `[f]` arity, dropping the still-present
    // `arg0…` wires before they can be re-derived) and uses the
    // *preserving-args* variant so the positionally-present arg wires survive
    // the layout install. Arg-pin names are generic/stable, so on an
    // already-consistent graph this matches the by-name rebuild.
    node_type_registry.update_apply_pin_layouts_for_network_preserving_args(network);

    // REPAIR PHASE: Currying Phase 4 (`doc/design_currying.md`,
    // §"HOF auto-partialization (`map`)"). For every `map` node whose `f`
    // pin is wired with a starts-with-compatible source, override the map's
    // `f` pin type to match the source exactly and derive `output_type` from
    // `f`. Runs after the apply post-pass so an `apply` source feeding
    // `map.f` has its output type resolved against its updated arg-pin
    // layout first.
    node_type_registry.update_map_pin_layouts_for_network_preserving_args(network);

    // REPAIR PHASE: Ensure argument counts match parameter counts in this
    // network (runs after the apply/map post-passes so their derived arg-pin
    // counts are in place before padding/truncation).
    repair_network_arguments(network, node_type_registry);

    // REPAIR PHASE: Remove wires to output pins that no longer exist
    repair_output_pin_wires(network, node_type_registry);

    // VALIDATION PHASE: Check wire validity and resolve polymorphic output pins.
    let mut ctx = ValidationContext::new();
    let wires_valid = validate_wires(network, node_type_registry, &mut ctx);
    if !wires_valid {
        network.valid = false;
    }

    // VALIDATION PHASE: Zone-specific rules (rule 1: zone-output pins have
    // wires; rule 2: capture wires resolve; rule 3: zone-input references
    // resolve). Recurses into every HOF node's owned body and walks nested
    // zones with the ancestor chain extended. See `doc/design_zones.md`
    // (§"Validation").
    let zones_valid = validate_zones_recursive(network, &[], &[], node_type_registry);
    if !zones_valid {
        network.valid = false;
    }

    // Update the network's output type based on return node, using resolved
    // concrete types for any polymorphic pins on the return node. This runs
    // even when wires are invalid so the enclosing network can still see this
    // network's interface shape (e.g. to repair call-sites). Pins that cannot
    // be resolved fall back to DataType::None.
    let output_type_changed = update_network_output_type(network, node_type_registry, &mut ctx);

    NetworkValidationResult {
        valid: network.valid,
        interface_changed: interface_changed || output_type_changed,
    }
}

/// Recursively validate zone-related rules in `network` and every nested
/// zone body. Reports errors directly on the network whose node the violation
/// belongs to (body errors land on the body's `validation_errors`; the owning
/// HOF in the parent network also gets a generic "zone body invalid" marker).
///
/// `ancestors[i]` is the network at depth `i` from the root (so `ancestors[0]`
/// is the root, `ancestors[len-1]` is the immediate parent of `network`).
/// `ancestor_hof_ids[i]` is the HOF node id (in `ancestors[i]`) whose owned
/// zone body is `ancestors[i+1]` — except for the deepest entry, which is the
/// HOF whose body is `network` itself. The two vectors always have the same
/// length; at the top-level call from `validate_network` both are empty.
///
/// Returns `true` iff `network` and every nested body passed validation.
fn validate_zones_recursive(
    network: &mut NodeNetwork,
    ancestors: &[&NodeNetwork],
    ancestor_hof_ids: &[u64],
    registry: &NodeTypeRegistry,
) -> bool {
    let mut ok = true;

    let node_ids: Vec<u64> = network.nodes.keys().copied().collect();

    // Pass A — for every node in `network`, check rule 1 (every zone-output
    // pin has an incoming wire) and check rules 2 & 3 on wires in the
    // node's `arguments` list. Wires in `zone_output_arguments` are scoped
    // to the body — they are checked in Pass B with the extended chain.
    for &node_id in &node_ids {
        let Some(node) = network.nodes.get(&node_id) else {
            continue;
        };
        let Some(node_type) = registry.get_node_type_for_node(node) else {
            continue;
        };

        // Rule 1: every zone-output pin must have at least one incoming wire.
        //
        // Suspended for an HOF whose `f` (Function) pin is connected: the
        // wired-in closure drives evaluation and the inline body is ignored,
        // so an empty body is fine (closures `doc/design_closures.md`,
        // §"Validation" check 1). The `closure` node has no `f` *input* pin, so
        // this never suspends its own "body is complete" check (check 2).
        if node_type.has_zone() && !function_input_pin_connected(node, node_type) {
            for (i, pin) in node_type.zone_output_pins.iter().enumerate() {
                let has_wire = node
                    .zone_output_arguments
                    .get(i)
                    .map(|arg| !arg.incoming_wires.is_empty())
                    .unwrap_or(false);
                if !has_wire {
                    // Non-blocking (does NOT set `ok = false`): the evaluator
                    // already turns a missing zone-output wire into a localized
                    // `NetworkResult::Error` (`zone_closure::build_inline_closure`),
                    // so an independent HOF/closure with an unwired body should
                    // not blank the whole viewport. We still push the error so
                    // the node lights up with a badge — but because the runtime
                    // poisons only this node and its downstream cone, the rest of
                    // the network stays evaluable. When the closure is actually
                    // consumed, the consumer goes dark via normal error
                    // propagation. (closures `doc/design_closures.md`
                    // §"Validation" check 1 / check 2.)
                    network.validation_errors.push(ValidationError::warning(
                        format!("Zone-output pin '{}' has no incoming wire", pin.name),
                        Some(node_id),
                    ));
                }
            }
        }

        // Rule 4 (closures `doc/design_closures.md`, §"Validation" check 4):
        // `apply` owns no inline body to fall back to, so its required `f`
        // (Function) pin must be connected. (An HOF with a disconnected `f`
        // uses its inline body and is fine; `apply` cannot.) The `f`-source's
        // function-type/shape is checked by `validate_wires` via
        // `can_be_converted_to`, like any other typed wire.
        if node.node_type_name == "apply" && !function_input_pin_connected(node, node_type) {
            ok = false;
            network.validation_errors.push(ValidationError::new(
                "apply: required `f` (Function) pin is not connected".to_string(),
                Some(node_id),
            ));
        }

        // Currying Phase 3 (`doc/design_currying.md`, §"Validation" check 1):
        // `apply`'s arg pins must be wired as a contiguous prefix — wiring
        // `arg_j` while some `arg_i` (i < j) is unwired is rejected. This is
        // what makes "k = number of wired arg pins" unambiguous at eval time,
        // and is the rule the editor enforces interactively. The function-pin
        // input lives at pin 0; arg pins are 1..N.
        if node.node_type_name == "apply" {
            let mut seen_unwired = false;
            let mut bad_pin: Option<usize> = None;
            for (i, arg) in node.arguments.iter().enumerate().skip(1) {
                if arg.incoming_wires.is_empty() {
                    seen_unwired = true;
                } else if seen_unwired {
                    bad_pin = Some(i - 1); // 0-based arg pin index
                    break;
                }
            }
            if let Some(j) = bad_pin {
                ok = false;
                network.validation_errors.push(ValidationError::new(
                    format!(
                        "apply: arg pins must be wired as a contiguous prefix \
                         (arg{} is wired while an earlier pin is unwired)",
                        j
                    ),
                    Some(node_id),
                ));
            }
        }

        // The function-mode mutual-exclusion rule is gone
        // (`doc/design_node_function_pin_captures.md`): wired inputs on a node
        // whose `-1` pin is consumed are now legal *captures*, not dead wires.
        // The `-1` source's wire-type check (now resolved against the
        // wiring-aware `resolve_output_type(-1)`) still runs in
        // `validate_wires`.

        // Wires in `arguments` are in this network's frame — depth = 0
        // resolves locally, depth > 0 walks `ancestors`.
        let arg_wires: Vec<IncomingWire> = node
            .arguments
            .iter()
            .flat_map(|a| a.incoming_wires.iter().cloned())
            .collect();
        for incoming in &arg_wires {
            if let Some(err) =
                check_zone_wire(incoming, node_id, ancestors, ancestor_hof_ids, registry)
            {
                ok = false;
                network.validation_errors.push(err);
            }
        }
    }

    // Pass B — for each HOF in `network`: validate the zone-output wires
    // (which live in the body's frame), then recurse into the owned body.
    let hof_ids: Vec<u64> = node_ids
        .iter()
        .filter(|id| {
            network
                .nodes
                .get(id)
                .and_then(|n| n.zone.as_ref())
                .is_some()
        })
        .copied()
        .collect();

    for hof_id in hof_ids {
        // Snapshot the zone-output wires before mutating — they're in the
        // body's frame (depth = 0 resolves to a body-internal source), so
        // we'll check them with the extended chain below.
        let zone_output_wires_snapshot: Vec<IncomingWire> = network
            .nodes
            .get(&hof_id)
            .map(|n| {
                n.zone_output_arguments
                    .iter()
                    .flat_map(|a| a.incoming_wires.iter().cloned())
                    .collect()
            })
            .unwrap_or_default();

        // Take the body Arc out so we can hold both `&network` (as the
        // immediate-parent reference in the extended chain) and `&mut body`
        // at once.
        let body_arc_opt = network.nodes.get_mut(&hof_id).and_then(|n| n.zone.take());
        let Some(mut body_arc) = body_arc_opt else {
            continue;
        };

        // Reset the body's validation state — bodies are only ever
        // validated through this recursion, so we own the error list.
        {
            let body = Arc::make_mut(&mut body_arc);
            body.valid = true;
            body.validation_errors.clear();
        }

        // Collect deferred errors so we don't have to hold `&*network`
        // (via the extended ancestors chain) while pushing onto
        // `network.validation_errors`.
        let (recursion_ok, deferred_errors) = {
            let mut new_ancestors: Vec<&NodeNetwork> = ancestors.to_vec();
            new_ancestors.push(&*network);
            let mut new_hof_ids: Vec<u64> = ancestor_hof_ids.to_vec();
            new_hof_ids.push(hof_id);

            let mut errs: Vec<ValidationError> = Vec::new();
            for wire in &zone_output_wires_snapshot {
                if let Some(err) =
                    check_zone_wire(wire, hof_id, &new_ancestors, &new_hof_ids, registry)
                {
                    errs.push(err);
                }
            }

            let body = Arc::make_mut(&mut body_arc);
            let r_ok = validate_zones_recursive(body, &new_ancestors, &new_hof_ids, registry);
            (r_ok, errs)
        };

        let body_inner_ok = recursion_ok && deferred_errors.is_empty();

        for err in deferred_errors {
            network.validation_errors.push(err);
        }

        if !body_inner_ok {
            {
                let body = Arc::make_mut(&mut body_arc);
                body.valid = false;
            }
            ok = false;
            network.validation_errors.push(ValidationError::new(
                "Zone body is invalid".to_string(),
                Some(hof_id),
            ));
        }

        if let Some(node) = network.nodes.get_mut(&hof_id) {
            node.zone = Some(body_arc);
        }
    }

    ok
}

/// Validates a single wire under the zone rules. Returns `Some(err)` if the
/// wire violates rule 2 or rule 3; `None` if the wire is fine (or is a
/// depth-0 local wire — those are handled by `validate_wires`).
fn check_zone_wire(
    incoming: &IncomingWire,
    dest_node_id: u64,
    ancestors: &[&NodeNetwork],
    ancestor_hof_ids: &[u64],
    registry: &NodeTypeRegistry,
) -> Option<ValidationError> {
    match incoming.source_pin {
        SourcePin::NodeOutput { pin_index } => {
            let depth = incoming.source_scope_depth as usize;
            if depth == 0 {
                // Local wire — handled by `validate_wires`.
                return None;
            }
            // Rule 2: depth > 0 means the source is in an ancestor network.
            // The chain `ancestors` is indexed root-first; depth-N up means
            // we want `ancestors[len - N]`. (`ancestors.last()` is depth=1.)
            if depth > ancestors.len() {
                return Some(ValidationError::new(
                    format!(
                        "Capture wire's source_scope_depth ({}) exceeds the \
                         enclosing-zone chain length ({})",
                        depth,
                        ancestors.len()
                    ),
                    Some(dest_node_id),
                ));
            }
            let source_network = ancestors[ancestors.len() - depth];
            let Some(source_node) = source_network.nodes.get(&incoming.source_node_id) else {
                return Some(ValidationError::new(
                    format!(
                        "Capture wire references non-existent source node {} \
                         in ancestor network (depth {})",
                        incoming.source_node_id, depth
                    ),
                    Some(dest_node_id),
                ));
            };
            // Confirm the named source pin exists on the ancestor source node.
            let Some(source_node_type) = registry.get_node_type_for_node(source_node) else {
                return Some(ValidationError::new(
                    format!(
                        "Capture wire's source node {} (depth {}) has \
                         unknown node type '{}'",
                        incoming.source_node_id, depth, source_node.node_type_name
                    ),
                    Some(dest_node_id),
                ));
            };
            let pin_count = source_node_type.output_pin_count();
            if (pin_index as usize) >= pin_count {
                return Some(ValidationError::new(
                    format!(
                        "Capture wire references output pin index {} on \
                         source node {} (depth {}) but that node has only \
                         {} output pin(s)",
                        pin_index, incoming.source_node_id, depth, pin_count
                    ),
                    Some(dest_node_id),
                ));
            }
            None
        }
        SourcePin::ZoneInput { pin_index } => {
            let depth = incoming.source_scope_depth as usize;
            // Rule 3: ZoneInput must reference an enclosing HOF (depth >= 1).
            if depth < 1 {
                return Some(ValidationError::new(
                    "ZoneInput wire must have source_scope_depth >= 1 \
                     (sibling zone-input references are not allowed)"
                        .to_string(),
                    Some(dest_node_id),
                ));
            }
            if depth > ancestor_hof_ids.len() {
                return Some(ValidationError::new(
                    format!(
                        "ZoneInput wire's source_scope_depth ({}) exceeds the \
                         enclosing-zone chain length ({})",
                        depth,
                        ancestor_hof_ids.len()
                    ),
                    Some(dest_node_id),
                ));
            }
            let expected_hof_id = ancestor_hof_ids[ancestor_hof_ids.len() - depth];
            if incoming.source_node_id != expected_hof_id {
                return Some(ValidationError::new(
                    format!(
                        "ZoneInput wire's source_node_id ({}) does not match \
                         the enclosing HOF id ({}) at depth {}",
                        incoming.source_node_id, expected_hof_id, depth
                    ),
                    Some(dest_node_id),
                ));
            }
            // Verify pin_index is within the source HOF's zone_input_pins.
            let hof_network = ancestors[ancestors.len() - depth];
            let Some(hof_node) = hof_network.nodes.get(&expected_hof_id) else {
                return Some(ValidationError::new(
                    format!(
                        "ZoneInput wire references HOF id {} at depth {} but \
                         that node no longer exists in the ancestor network",
                        expected_hof_id, depth
                    ),
                    Some(dest_node_id),
                ));
            };
            let Some(hof_type) = registry.get_node_type_for_node(hof_node) else {
                return Some(ValidationError::new(
                    format!(
                        "ZoneInput wire references HOF id {} at depth {} with \
                         unknown node type '{}'",
                        expected_hof_id, depth, hof_node.node_type_name
                    ),
                    Some(dest_node_id),
                ));
            };
            if pin_index >= hof_type.zone_input_pins.len() {
                return Some(ValidationError::new(
                    format!(
                        "ZoneInput pin_index {} out of range for HOF '{}' \
                         (it declares {} zone-input pin(s))",
                        pin_index,
                        hof_type.name,
                        hof_type.zone_input_pins.len()
                    ),
                    Some(dest_node_id),
                ));
            }
            None
        }
    }
}

fn update_network_output_type(
    network: &mut NodeNetwork,
    node_type_registry: &NodeTypeRegistry,
    ctx: &mut ValidationContext,
) -> bool {
    let old_output_pins = network.node_type.output_pins.clone();

    // Determine the new output pins based on return_node_id. Substitute
    // `Fixed(<concrete>)` for each pin by resolving polymorphic pins against
    // the validation cache. Custom-network parameter pins are concrete
    // (enforced in `validate_parameters`), so resolution always succeeds in a
    // valid graph; unresolved pins fall back to DataType::None, which is
    // consistent with how unresolved outputs were treated previously.
    let new_output_pins = if let Some(return_node_id) = network.return_node_id {
        if let Some(return_node) = network.nodes.get(&return_node_id) {
            let return_node_type = node_type_registry
                .get_node_type_for_node(return_node)
                .unwrap();
            let mut pins = Vec::with_capacity(return_node_type.output_pins.len());
            for (pin_idx, pin) in return_node_type.output_pins.iter().enumerate() {
                // Preserve `Fixed` pins as-is so their declared types (even
                // abstract ones on not-yet-migrated nodes) reach the
                // enclosing network unchanged. For polymorphic pins,
                // substitute the resolved concrete type; if resolution fails
                // fall back to DataType::None.
                let data_type = match &pin.data_type {
                    PinOutputType::Fixed(_) => pin.data_type.clone(),
                    _ => PinOutputType::Fixed(
                        ctx.resolve(network, node_type_registry, return_node_id, pin_idx as i32)
                            .unwrap_or(DataType::None),
                    ),
                };
                pins.push(OutputPinDefinition {
                    name: pin.name.clone(),
                    data_type,
                });
            }
            pins
        } else {
            // Return node doesn't exist, set to None
            OutputPinDefinition::single(DataType::None)
        }
    } else {
        // No return node, output type is None
        OutputPinDefinition::single(DataType::None)
    };

    // Update the network's output pins
    network.node_type.output_pins = new_output_pins.clone();

    // Check if output pins changed (count or types)
    let changed = old_output_pins.len() != new_output_pins.len()
        || old_output_pins
            .iter()
            .zip(new_output_pins.iter())
            .any(|(old, new)| old.name != new.name || old.data_type != new.data_type);

    changed
}
