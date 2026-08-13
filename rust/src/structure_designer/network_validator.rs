//! Network validation: the pass that walks a `NodeNetwork` — and, recursively,
//! every zone body — and records `ValidationError`s on it.
//!
//! The severity model (blocking = cone-poisons the node, non-blocking =
//! advisory badge, interface = whole-network refusal) and the litmus test for
//! choosing between them when adding a rule are in `../AGENTS.md` §"Validation
//! errors". What follows is the mechanics that only matter once you are editing
//! this file. Design doc: `doc/design_error_management.md`.
//!
//! # Passes accumulate
//!
//! Since error-management Phase 2 (D4) the wire/parameter passes **accumulate**
//! rather than stopping at the first violation: `validate_wires` records each
//! node's first error (sorted-node-id order, exact-duplicate rows deduped) and
//! moves on; `validate_parameters` records every violation but skips the
//! interface rebuild when any exists. Under cone-poisoning this is load-bearing,
//! not cosmetic: an error the validator did not record is a node that is *not*
//! poisoned.
//!
//! # Stored-data errors
//!
//! `NodeData::get_data_error() -> Option<NodeDataError>` lets a node report a
//! problem in its own stored data — today the parse failure of a definition
//! string on `motif` (blocking: no motif to emit), and on `motif_sub` /
//! `materialize` (warnings: their `eval` no-ops on unparsed data and still emits
//! a usable value). `validate_zones_recursive`'s Pass A asks every node on every
//! validate pass and pushes the corresponding `ValidationError`, so these reach
//! the unified panel list and the F8 cycle with no transient `initial_errors`
//! plumbing. (`expr` keeps its `initial_errors` route because its errors must be
//! attached at data-set time, before the parse result is stored.)
//!
//! # Wire cycles
//!
//! `validate_zones_recursive` runs `detect_wire_cycles` once per scope: it builds
//! the cross-scope-complete dependency graph (regular depth-0 wires, plus every
//! capture / zone-output wire in a zone owner's body subtree whose depth resolves
//! to the scope, projected onto the owner) and flags every cycle member with a
//! blocking error via Tarjan SCC — so evaluation never enters a fully poisoned
//! cycle. Defense in depth for cycles that escape validation (hand-authored
//! `.cnnd` bypasses connect-time checks): the evaluator's
//! `context.eval_in_progress` re-entrancy guard turns same-frame re-entry into a
//! localized "evaluation cycle detected" error instead of a hang. That guard keys
//! on the **network-stack fingerprint** (`EvalFrameKey`), NOT on `NodeRef` — the
//! eval scope path does not track the `parameter` node's stack excursion, so a
//! scope-keyed guard falsely flags legal graphs (per-network id collisions).
//! Related invariant: `resolve_output_type` has **no** cycle guard, because the
//! evaluator must never type-resolve a poisoned source (`resolve_incoming_wire`
//! skips resolution for them); keep it that way.
//!
//! # The Phase 6 severity sweep (D9)
//!
//! Three rules that were warnings *only* because the runtime already localized
//! the failure — unwired zone-output pin on an HOF/`closure`, `apply` with its
//! required `f` unwired, and `parameter` inside a zone body (#417) — are now
//! **blocking**. Their skip-and-synthesize output says what their `eval` used to
//! (the wording is now the validation rule's, so downstream chain *text* changed
//! slightly), and D8's dedupe shows **one** entry per node instead of an amber
//! validation row plus the red eval row it predicted. The
//! `Supplied`-but-unwired-and-required rule stays a warning: pin 0 still
//! displays. Two consequences to preserve when adding rules in this area:
//!
//! - A blocking rule inside a **zone body** must not also set
//!   `validate_zones_recursive`'s local `ok = false` unless the *owner's* eval is
//!   genuinely broken: `ok` is what raises [`ZONE_BODY_INVALID_MARKER`] on the
//!   enclosing HOF, i.e. it poisons the whole HOF. One stray broken body node
//!   should darken its own cone, not the HOF — the same blast-radius argument D3
//!   made for the network.
//! - Tests (and any code) that build a body by poking the registry directly must
//!   **re-validate** afterwards: an ordinary wire does not re-validate
//!   (`connect_nodes` only does so for function wires; the app's real body-wiring
//!   paths `connect_wire_scoped` / `connect_zone_output_wire` always do), so a
//!   stale "zone-output pin has no incoming wire" error now cone-poisons the node
//!   instead of merely showing amber.
//!
//! # Zone rule 4: no `parameter` node in a body (#417)
//!
//! A `parameter` declares an input pin of the enclosing *network*, and a body has
//! no interface. The rule is single-sourced as
//! `node_type_registry::allowed_in_zone_body(name)`, whose other consumers are
//! `add_node_scoped` / `paste_at_position_scoped` / `duplicate_node_scoped`
//! (refuse/drop), `APINodeTypeView::allowed_in_zone_body` (the add-node popup
//! filters a body-scoped list on it), and `ParameterData::eval` (localized
//! error) — so the validation rule is only ever reached by hand-authored or
//! pre-#417 `.cnnd`.
//!
//! That `eval` guard is what makes the localized choice legal: without it the two
//! eval paths are wrong in different ways. On a real stack the frame below the
//! parameter is the zone **owner**, so `parent_node.arguments[param_index]` reads
//! e.g. `map.xs` (or panics on a `closure`, which has no input pins); on a lazy
//! walker's body-only stack it silently degrades to "constant = my `default`
//! pin". The guard reads "am I in a zone body?" straight off the frame it lives
//! in — `NetworkStackElement::is_zone_body`, recorded `true` at every body push
//! (`run_closure_once`, the capture pre-evaluation pushes, `generate_scene_scoped`'s
//! per-hop descent) and `false` for custom-network entries and root frames.
//! **Never reconstruct this from stack shape or `eval_scope_path`**: the original
//! heuristic (*empty call stack + non-empty `eval_scope_path`*) false-positived on
//! a legal graph, because a custom network's parameter resolves its argument via a
//! parent-stack excursion that pops the stack frame while the instance's eval
//! scope stays pushed — so a top-level `parameter` feeding a custom-node
//! instance's pin was misread as body-resident. Regression test:
//! `parameter_in_zone_body_test.rs::top_level_parameter_feeding_custom_instance_is_not_flagged`.

use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_network::{
    Argument, FunctionPinDisposition, IncomingWire, Node, NodeNetwork, SourcePin, ValidationError,
    function_input_pin_connected, function_pin_dispositions,
};
use crate::structure_designer::node_type::{OutputPinDefinition, Parameter, PinOutputType};
use crate::structure_designer::node_type_registry::{NodeTypeRegistry, allowed_in_zone_body};
use crate::structure_designer::nodes::parameter::ParameterData;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The generic error attached to an HOF in the parent network when its zone
/// body is invalid — it exists so the HOF lights up red even when only a deep
/// body node is at fault. It is a *marker*, not the real diagnosis: the actual
/// error(s) live on the body network with a precise node id. Error-navigation
/// collection recognizes and skips this marker (see
/// `scoped_validation_errors`), so the panel navigates to the real body node
/// rather than to the HOF.
pub const ZONE_BODY_INVALID_MARKER: &str = "Zone body is invalid";

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

/// A single parameter-id reassignment performed by [`dedupe_param_ids_in_network`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamIdReassignment {
    pub network_name: String,
    pub param_node_id: u64,
    pub param_name: String,
    pub old_param_id: u64,
    pub new_param_id: u64,
}

/// Heals "Damage A" of the `next_param_id` bug (see
/// `doc/design_parameter_wire_stability.md`, F6): a project saved by the buggy
/// build can contain two parameter nodes that share the same `param_id`. On the
/// next parameter edit that duplicate makes `repair_call_sites_for_network`
/// mis-match and re-jumble wires. This pass restores the invariant by giving each
/// later duplicate a fresh unique id — the first occurrence (lowest node id) keeps
/// its id.
///
/// It is **safe**: wires are stored positionally and carry no `param_id`, so
/// renumbering moves no connection — it only repairs identity for future edits. It
/// deliberately does NOT touch wiring, so already-corrupted connections ("Damage
/// B") are unaffected and must be surfaced separately (F4). Idempotent: a no-op
/// when ids are already unique. Returns the reassignments performed, for logging
/// and user notification.
pub fn dedupe_param_ids_in_network(network: &mut NodeNetwork) -> Vec<ParamIdReassignment> {
    // Collect (node_id, param_id, param_name) for parameter nodes that carry an
    // id, in ascending node-id order so "keep the first occurrence" is deterministic.
    let mut params: Vec<(u64, u64, String)> = network
        .nodes
        .iter()
        .filter_map(|(&nid, node)| {
            node.data
                .as_ref()
                .as_any_ref()
                .downcast_ref::<ParameterData>()
                .and_then(|p| p.param_id.map(|id| (nid, id, p.param_name.clone())))
        })
        .collect();
    params.sort_by_key(|&(nid, _, _)| nid);

    // Next free id: strictly above every id currently in use and above the counter.
    let mut next_free = params
        .iter()
        .map(|&(_, id, _)| id)
        .max()
        .map(|m| m + 1)
        .unwrap_or(1)
        .max(network.next_param_id);

    let mut seen: HashSet<u64> = HashSet::new();
    let mut fixes = Vec::new();
    for (nid, id, name) in params {
        if seen.insert(id) {
            continue; // first occurrence of this id — keep it
        }
        // Duplicate: assign a fresh id beyond everything in use.
        let new_id = next_free;
        next_free += 1;
        seen.insert(new_id);
        if let Some(node) = network.nodes.get_mut(&nid)
            && let Some(p) = node.data.as_any_mut().downcast_mut::<ParameterData>()
        {
            p.param_id = Some(new_id);
        }
        fixes.push(ParamIdReassignment {
            network_name: network.node_type.name.clone(),
            param_node_id: nid,
            param_name: name,
            old_param_id: id,
            new_param_id: new_id,
        });
    }
    if !fixes.is_empty() {
        network.next_param_id = network.next_param_id.max(next_free);
    }
    fixes
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
    // D4 (`doc/design_error_management.md`): accumulate where safe — the
    // cast-failure, duplicate-name, and abstract-type checks each run over
    // every parameter node and record every violation before this function
    // gives up. When any error was recorded the parameter-interface rebuild
    // below is still skipped wholesale (same as the old first-error
    // behavior): a partially-rebuilt interface out of a broken parameter set
    // is exactly the call-site desync class this pass protects against.
    let mut errors: Vec<ValidationError> = Vec::new();

    // Collect all parameter nodes, in ascending node-id order so the error
    // list (and which duplicate "loses" the name check) is deterministic.
    let mut parameter_nodes: Vec<(u64, &ParameterData)> = Vec::new();
    let mut node_ids: Vec<u64> = network.nodes.keys().copied().collect();
    node_ids.sort_unstable();
    for node_id in node_ids {
        let node = &network.nodes[&node_id];
        if node.node_type_name == "parameter" {
            // Cast node data to ParameterData
            if let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>() {
                parameter_nodes.push((node_id, param_data));
            } else {
                errors.push(ValidationError::interface_error(
                    "Parameter node has invalid data type".to_string(),
                    Some(node_id),
                ));
            }
        }
    }

    // Validate param_name uniqueness: the first occurrence (lowest node id)
    // keeps the name; every later duplicate gets its own error.
    let mut param_names: HashMap<String, u64> = HashMap::new();
    for (node_id, param_data) in &parameter_nodes {
        if param_names.contains_key(&param_data.param_name) {
            errors.push(ValidationError::interface_error(
                format!("Duplicate parameter name '{}'", param_data.param_name),
                Some(*node_id),
            ));
        } else {
            param_names.insert(param_data.param_name.clone(), *node_id);
        }
    }

    // Reject abstract parameter types: abstract types may only appear as declared
    // input-pin types on built-in polymorphic nodes, not on user-declared parameter pins.
    for (node_id, param_data) in &parameter_nodes {
        if contains_abstract(&param_data.data_type) {
            errors.push(ValidationError::interface_error(
                format!(
                    "Parameter '{}' has abstract type {:?}; abstract phase types are not allowed on parameter pins",
                    param_data.param_name, param_data.data_type
                ),
                Some(*node_id),
            ));
        }
    }

    if !errors.is_empty() {
        network.validation_errors.extend(errors);
        return false;
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
        if let Some(node) = network.nodes.get_mut(&node_id)
            && let Some(param_data) = (*node.data).as_any_mut().downcast_mut::<ParameterData>()
        {
            param_data.param_index = new_index;
        }
    }

    true
}

fn check_interface_changed(network: &NodeNetwork) -> bool {
    // Collect current parameter nodes with their IDs for deterministic sorting
    let mut current_params_with_ids: Vec<(u64, &ParameterData)> = Vec::new();

    for (node_id, node) in &network.nodes {
        if node.node_type_name == "parameter"
            && let Some(param_data) = (*node.data).as_any_ref().downcast_ref::<ParameterData>()
        {
            current_params_with_ids.push((*node_id, param_data));
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
        // `function_pin_roles` is pin-index-keyed like `arguments`, so it has
        // the same exposure when a custom node type's pin layout shrinks: prune
        // entries that no longer name a pin. (`function_pin_dispositions`
        // ignores out-of-range entries anyway, so this is hygiene, not
        // correctness — it keeps the map from silently re-attaching a stale
        // role if the layout later grows back.) See
        // `doc/design_function_pin_roles.md`.
        node.function_pin_roles.retain(|&i, _| i < expected_count);
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
    // D4 (`doc/design_error_management.md`): accumulate per node instead of
    // short-circuiting the whole pass — under cone-scoped blocking (D3) an
    // error the validator did not record is a node that is not poisoned, so
    // every invalid node must get its error. Within one node the checks keep
    // their early-outs (later checks assume earlier invariants); the first
    // error per node is recorded and validation moves on to the next node.
    // Nodes are visited in ascending-id order so the error list (and the
    // panel/F8 order built from it) is deterministic — the old first-error
    // behavior followed HashMap iteration order.
    //
    // Some checks attribute their error to the *source* node of a wire, so
    // the same (node, message) pair can be produced once per consuming wire;
    // exact repeats are deduped to keep one row per underlying fact.
    let mut errors: Vec<ValidationError> = Vec::new();
    let mut node_ids: Vec<u64> = network.nodes.keys().copied().collect();
    node_ids.sort_unstable();
    for node_id in node_ids {
        let Some(dest_node) = network.nodes.get(&node_id) else {
            continue;
        };
        if let Some(err) = validate_node_wires(network, node_type_registry, ctx, node_id, dest_node)
            && !errors
                .iter()
                .any(|e| e.node_id == err.node_id && e.error_text == err.error_text)
        {
            errors.push(err);
        }
    }
    let ok = errors.is_empty();
    network.validation_errors.extend(errors);
    ok
}

/// Runs the wire/type checks for a single destination node and returns the
/// first violation found (checks within one node early-out — later checks
/// assume earlier invariants). `None` means the node passed. The returned
/// error is usually attributed to `dest_node_id`, but source-side checks
/// attribute to the offending source node.
fn validate_node_wires(
    network: &NodeNetwork,
    node_type_registry: &NodeTypeRegistry,
    ctx: &mut ValidationContext,
    dest_node_id: u64,
    dest_node: &Node,
) -> Option<ValidationError> {
    // Check if this node references a node network and validate its validity
    if let Some(referenced_network) = node_type_registry
        .node_networks
        .get(&dest_node.node_type_name)
        && !referenced_network.valid
    {
        return Some(ValidationError::new(
            format!(
                "References invalid node network '{}'",
                dest_node.node_type_name
            ),
            Some(dest_node_id),
        ));
    }

    // Get the destination node type to access parameter information
    let dest_node_type = match node_type_registry.get_node_type_for_node(dest_node) {
        Some(node_type) => node_type,
        None => {
            return Some(ValidationError::new(
                format!("Unknown node type '{}'", dest_node.node_type_name),
                Some(dest_node_id),
            ));
        }
    };

    // Validate argument count matches parameter count
    // (This should always pass after repair phase)
    if dest_node.arguments.len() != dest_node_type.parameters.len() {
        return Some(ValidationError::new(
            format!(
                "Node has {} arguments but type expects {} parameters",
                dest_node.arguments.len(),
                dest_node_type.parameters.len()
            ),
            Some(dest_node_id),
        ));
    }

    // Validate each argument (input pin) of the destination node
    for (arg_index, argument) in dest_node.arguments.iter().enumerate() {
        // Get parameter information for this argument
        let parameter = &dest_node_type.parameters[arg_index];

        // Validate non-multi input pins have at most one connection
        if !parameter.data_type.is_array() && argument.len() > 1 {
            return Some(ValidationError::new(
                format!(
                    "Non-multi parameter '{}' has {} connections, but only 1 is allowed",
                    parameter.name,
                    argument.len()
                ),
                Some(dest_node_id),
            ));
        }

        // Validate data types for each connected source node
        for incoming in &argument.incoming_wires {
            let source_node_id = &incoming.source_node_id;
            let output_pin_index = match incoming.source_pin {
                crate::structure_designer::node_network::SourcePin::NodeOutput { pin_index } => {
                    pin_index
                }
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
                    return Some(ValidationError::new(
                        "Wire references non-existent source node".to_string(),
                        Some(dest_node_id),
                    ));
                }
            };

            // Check if this source node references a node network and validate its validity
            if let Some(referenced_network) = node_type_registry
                .node_networks
                .get(&source_node.node_type_name)
                && !referenced_network.valid
            {
                return Some(ValidationError::new(
                    format!(
                        "Source node references invalid node network '{}'",
                        source_node.node_type_name
                    ),
                    Some(*source_node_id),
                ));
            }

            // Get the source node type to access its output type
            let _source_node_type = match node_type_registry.get_node_type_for_node(source_node) {
                Some(node_type) => node_type,
                None => {
                    return Some(ValidationError::new(
                        format!("Unknown source node type '{}'", source_node.node_type_name),
                        Some(*source_node_id),
                    ));
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

            let dest_data_type = node_type_registry.get_node_param_data_type(dest_node, arg_index);
            if !DataType::can_be_converted_to(
                &source_data_type,
                &dest_data_type,
                node_type_registry,
            ) {
                return Some(ValidationError::new(
                    format!(
                        "Data type mismatch: input expects {:?}, but source outputs {:?}",
                        parameter.data_type, source_data_type
                    ),
                    Some(dest_node_id),
                ));
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
            .resolve(network, node_type_registry, dest_node_id, pin_index)
            .is_none()
        {
            return Some(ValidationError::new(
                format!(
                    "Output pin '{}' ({}) could not be resolved to a concrete type",
                    pin.name, pin.data_type
                ),
                Some(dest_node_id),
            ));
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
        let resolved = ctx.resolve(network, node_type_registry, dest_node_id, pin_index);
        if let Some(t) = resolved
            && matches!(t, DataType::AnyFunction { .. })
        {
            let pin = &dest_node_type.output_pins[pin_index_usize];
            return Some(ValidationError::new(
                format!(
                    "Output pin '{}' resolves to `AnyFunction`; \
                             `AnyFunction` is an input-pin-only type",
                    pin.name
                ),
                Some(dest_node_id),
            ));
        }
    }

    None
}

pub fn validate_network(
    network: &mut NodeNetwork,
    node_type_registry: &mut NodeTypeRegistry,
    initial_errors: Option<Vec<crate::structure_designer::node_network::ValidationError>>,
) -> NetworkValidationResult {
    // Clear previous validation state
    network.valid = true;
    network.validation_errors.clear();

    // Add initial errors first if provided. Whether they affect `valid` is
    // decided by the residue predicate at the end of this function (D5) —
    // node-attributed blocking errors (e.g. expr parse errors) cone-poison
    // their node instead of blanking the network.
    if let Some(errors) = initial_errors {
        network.validation_errors.extend(errors);
    }

    // Check if interface changed before validation (to detect changes)
    let interface_changed = check_interface_changed(network);

    // Store old parameters before updating them
    let old_parameters = network.node_type.parameters.clone();

    // Validate parameters (this updates parameter order and indices). Its
    // errors carry `interface: true`, so the residue predicate below keeps
    // the whole network refusing evaluation — a desynced parameter interface
    // is the known call-site OOB-panic class (see
    // `doc/design_error_management.md` D5).
    if !validate_parameters(network) {
        network.valid = !crate::structure_designer::node_network::has_interface_residue(
            &network.validation_errors,
        );
        return NetworkValidationResult {
            valid: network.valid,
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

    // REPAIR PHASE: zip_with `f`-derivation (`doc/design_zip_with.md` Phase 2).
    // The N-lane sibling of the map pass: for every `zip_with` node whose `f`
    // pin is wired with a source whose parameter list starts with the lane
    // types, derive the output pin type (`Iter[R]` / `Iter[Function(tail→R)]`).
    // Same after-apply ordering rationale as map's.
    node_type_registry.update_zip_with_pin_layouts_for_network_preserving_args(network);

    // REPAIR PHASE: Ensure argument counts match parameter counts in this
    // network (runs after the apply/map post-passes so their derived arg-pin
    // counts are in place before padding/truncation).
    repair_network_arguments(network, node_type_registry);

    // REPAIR PHASE: Remove wires to output pins that no longer exist
    repair_output_pin_wires(network, node_type_registry);

    // VALIDATION PHASE: Check wire validity and resolve polymorphic output pins.
    let mut ctx = ValidationContext::new();
    validate_wires(network, node_type_registry, &mut ctx);

    // VALIDATION PHASE: Zone-specific rules (rule 1: zone-output pins have
    // wires; rule 2: capture wires resolve; rule 3: zone-input references
    // resolve) plus per-scope wire-cycle detection. Recurses into every HOF
    // node's owned body and walks nested zones with the ancestor chain
    // extended. See `doc/design_zones.md` (§"Validation").
    validate_zones_recursive(network, &[], &[], node_type_registry);

    // D5 (`doc/design_error_management.md`): `valid` means "free of the
    // interface residue" — an interface-level error or a blocking error with
    // no node attribution. Node-attributed blocking errors do NOT flip it;
    // they cone-poison their node at evaluation time instead (D3's
    // skip-and-synthesize in the evaluator). Every `.valid` reader — the
    // scene gate, the custom-network eval refusal, the "References invalid
    // node network" rule, the execute/CLI gates, the upward validity cascade
    // — asks "is this network usable at all?", and inherits the shrunk
    // meaning through this one producer-side redefinition.
    network.valid =
        !crate::structure_designer::node_network::has_interface_residue(&network.validation_errors);

    // Update the network's output type based on return node, using resolved
    // concrete types for any polymorphic pins on the return node. This runs
    // even when wires are invalid so the enclosing network can still see this
    // network's interface shape (e.g. to repair call-sites). Pins that cannot
    // be resolved fall back to DataType::None.
    let output_type_changed = update_network_output_type(network, node_type_registry, &mut ctx);

    // Phase 0 (doc/design_identity_vs_naming_phase0.md): enforce the full
    // network invariant catalogue now that initialization is guaranteed
    // complete. Debug-only. This generalizes (and folds in) the former
    // single-purpose `custom_node_type` cache assert — `CacheNone` preserves its
    // legacy panic substring so the existing `#[should_panic]` regression test
    // still passes.
    #[cfg(debug_assertions)]
    crate::structure_designer::invariants::debug_assert_network_invariants(
        network,
        node_type_registry,
    );

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
/// Whether the input pin named `param_name` on `node` must be supplied — i.e.
/// whether the node's `eval` has no value to fall back on when the pin is
/// unwired.
///
/// Mirrors the resolution order documented in
/// `text_format/node_type_introspection.rs` (the other consumer of this
/// information), against the node's **own** data rather than a default
/// instance:
///
/// 1. a matching stored text property ⇒ not required (`evaluate_or_default`
///    reads it),
/// 2. otherwise `get_parameter_metadata()`'s `(is_required, _)` flag,
/// 3. otherwise required — the safe default, matching the introspection
///    fallback.
fn parameter_is_required(node: &Node, param_name: &str) -> bool {
    if node
        .data
        .get_text_properties()
        .iter()
        .any(|(name, _)| name == param_name)
    {
        return false;
    }
    match node.data.get_parameter_metadata().get(param_name) {
        Some((is_required, _)) => *is_required,
        None => true,
    }
}

/// Detect wire cycles within the scope of `network` and return one blocking
/// validation error per cycle member (`doc/design_error_management.md` D5).
///
/// The dependency graph is **cross-scope-complete**: wires are not scope-local
/// (capture wires and zone-output wires thread through zone bodies), so a
/// cycle can run "node X → captured into H's body → body node → zone-output
/// wire → H's output → ordinary wires → X" — invisible to a DFS that treats H
/// as opaque. The saving structural fact: a body node's output is consumable
/// only intra-body or by the owning node's `zone_output_arguments` — there is
/// no wire from outside into a body — so every cross-scope cycle passes
/// through the zone-owning node itself. The graph for scope S is therefore:
///
/// - one vertex per node of S; one edge per depth-0 regular wire in S;
/// - for each zone-owning node H of S (HOFs *and* `closure` nodes), every
///   capture / zone-output wire anywhere in H's body subtree whose
///   `source_scope_depth` resolves to a node of S contributes the edge
///   "H depends on that node". Captures resolving to a scope *above* S are
///   projected onto the zone-owning ancestor when that ancestor's own scope
///   is validated (this function runs once per scope via
///   `validate_zones_recursive`).
///
/// `ZoneInput` references need no edge — they reach the iteration value
/// through H's own input wires, which are already edges. Custom-network
/// *reference* cycles are rejected at network-creation time.
fn detect_wire_cycles(network: &NodeNetwork) -> Vec<ValidationError> {
    let scope_nodes: HashSet<u64> = network.nodes.keys().copied().collect();

    // Adjacency: source node -> its same-scope consumers ("consumer depends
    // on source"). Deduped via HashSet targets.
    let mut edges: HashMap<u64, HashSet<u64>> = HashMap::new();
    for (&node_id, node) in &network.nodes {
        // Regular wires stored on this scope's nodes: depth 0 = same-scope
        // source. (Depth ≥ 1 resolves above S and is projected when the
        // ancestor scope is validated.)
        for arg in &node.arguments {
            for w in &arg.incoming_wires {
                if matches!(w.source_pin, SourcePin::NodeOutput { .. })
                    && w.source_scope_depth == 0
                    && scope_nodes.contains(&w.source_node_id)
                {
                    edges.entry(w.source_node_id).or_default().insert(node_id);
                }
            }
        }
        // Zone-owning node: project every dependency its body subtree has on
        // a node of THIS scope onto the owner itself.
        if let Some(body) = node.zone.as_deref() {
            let mut sources: HashSet<u64> = HashSet::new();
            collect_scope_sources_in_body_frame(node, body, 1, &scope_nodes, &mut sources);
            for src in sources {
                edges.entry(src).or_default().insert(node_id);
            }
        }
    }

    let cycle_groups = find_cycle_sccs(network, &edges);
    let mut errors = Vec::new();
    for group in cycle_groups {
        let description: Vec<String> = group
            .iter()
            .map(|id| {
                let type_name = network
                    .nodes
                    .get(id)
                    .map(|n| n.node_type_name.as_str())
                    .unwrap_or("node");
                format!("{} #{}", type_name, id)
            })
            .collect();
        let text = format!("Wire cycle detected among: {}", description.join(", "));
        for id in group {
            errors.push(ValidationError::new(text.clone(), Some(id)));
        }
    }
    errors
}

/// Collect the ids of scope-S nodes that the body subtree rooted at
/// (`owner`, `body`) depends on through capture / zone-output wires.
/// `rel_depth` is the frame depth of `body` relative to S (the top zone
/// owner's own body is 1); a wire stored in a frame at depth `k` with
/// `source_scope_depth == k` resolves to S exactly.
fn collect_scope_sources_in_body_frame(
    owner: &Node,
    body: &NodeNetwork,
    rel_depth: usize,
    scope_nodes: &HashSet<u64>,
    sources: &mut HashSet<u64>,
) {
    // The owner's zone-output wires live in its body's frame.
    for zarg in &owner.zone_output_arguments {
        for w in &zarg.incoming_wires {
            if matches!(w.source_pin, SourcePin::NodeOutput { .. })
                && w.source_scope_depth as usize == rel_depth
                && scope_nodes.contains(&w.source_node_id)
            {
                sources.insert(w.source_node_id);
            }
        }
    }
    for inner_node in body.nodes.values() {
        for arg in &inner_node.arguments {
            for w in &arg.incoming_wires {
                if matches!(w.source_pin, SourcePin::NodeOutput { .. })
                    && w.source_scope_depth as usize == rel_depth
                    && scope_nodes.contains(&w.source_node_id)
                {
                    sources.insert(w.source_node_id);
                }
            }
        }
        if let Some(inner_body) = inner_node.zone.as_deref() {
            collect_scope_sources_in_body_frame(
                inner_node,
                inner_body,
                rel_depth + 1,
                scope_nodes,
                sources,
            );
        }
    }
}

/// Strongly connected components of the scope dependency graph that
/// constitute cycles: every SCC of size ≥ 2, plus single nodes with a
/// self-loop edge. Iterative Tarjan (no recursion — the graph is
/// user-authored, so a deep linear chain must not overflow the stack).
/// Groups and their members are sorted by node id for deterministic error
/// output.
fn find_cycle_sccs(network: &NodeNetwork, edges: &HashMap<u64, HashSet<u64>>) -> Vec<Vec<u64>> {
    let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
    ids.sort_unstable();
    let index_of: HashMap<u64, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let n = ids.len();
    let adj: Vec<Vec<usize>> = ids
        .iter()
        .map(|id| {
            let mut targets: Vec<usize> = edges
                .get(id)
                .map(|t| t.iter().map(|x| index_of[x]).collect())
                .unwrap_or_default();
            targets.sort_unstable();
            targets
        })
        .collect();

    const UNVISITED: usize = usize::MAX;
    let mut index = vec![UNVISITED; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0usize;
    let mut groups: Vec<Vec<u64>> = Vec::new();

    for start in 0..n {
        if index[start] != UNVISITED {
            continue;
        }
        // Explicit call stack of (vertex, next-child position).
        let mut call: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&(v, pos)) = call.last() {
            if pos == 0 && index[v] == UNVISITED {
                index[v] = next_index;
                low[v] = next_index;
                next_index += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if pos < adj[v].len() {
                let w = adj[v][pos];
                call.last_mut().unwrap().1 += 1;
                if index[w] == UNVISITED {
                    call.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                call.pop();
                if low[v] == index[v] {
                    // v is an SCC root: pop its component.
                    let mut scc = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w] = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    let is_cycle = scc.len() > 1 || adj[v].binary_search(&v).is_ok();
                    if is_cycle {
                        let mut group: Vec<u64> = scc.into_iter().map(|w| ids[w]).collect();
                        group.sort_unstable();
                        groups.push(group);
                    }
                }
                if let Some(&(parent, _)) = call.last() {
                    low[parent] = low[parent].min(low[v]);
                }
            }
        }
    }
    groups.sort();
    groups
}

fn validate_zones_recursive(
    network: &mut NodeNetwork,
    ancestors: &[&NodeNetwork],
    ancestor_hof_ids: &[u64],
    registry: &NodeTypeRegistry,
) -> bool {
    let mut ok = true;

    // Wire-cycle rule (`doc/design_error_management.md` D5): this function is
    // called exactly once per scope (the top-level network from
    // `validate_network`, every zone body via the Pass B recursion below), so
    // running the detection here covers every scope of the design. Blocking,
    // attributed to every cycle member — under cone-scoped blocking (D3) the
    // evaluator skips the members' `eval`, so evaluation never enters the
    // cycle.
    let cycle_errors = detect_wire_cycles(network);
    if !cycle_errors.is_empty() {
        ok = false;
        network.validation_errors.extend(cycle_errors);
    }

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

        // Stored-data errors (`doc/design_error_management.md` D9, Phase 6):
        // `motif` / `motif_sub` / `materialize` parse a user-typed definition
        // string when their data is set and keep the failure on the node data.
        // That used to be a **third** error channel — visible only as a node
        // badge, absent from the panel list and the F8 cycle. Asking the node
        // data on every validate pass folds it into the one unified list with
        // no transient `initial_errors` plumbing to keep in sync.
        //
        // Deliberately does **not** set `ok = false` even for a blocking one:
        // `ok` drives the parent's `ZONE_BODY_INVALID_MARKER`, i.e. it poisons
        // the *owning HOF*. A broken `motif` inside a body should darken that
        // motif's cone (which the blocking flag already does via D3), not the
        // whole HOF — the same blast-radius argument D3 made for the network.
        if let Some(data_error) = node.data.get_data_error() {
            let error = if data_error.blocking {
                ValidationError::new(data_error.message, Some(node_id))
            } else {
                ValidationError::warning(data_error.message, Some(node_id))
            };
            network.validation_errors.push(error);
        }

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
                    // **Blocking** since the D9 severity sweep
                    // (`doc/design_error_management.md` Phase 6). This rule was
                    // a warning only because the runtime already localized the
                    // failure — `zone_closure::build_inline_closure` turns a
                    // missing zone-output wire into a `NetworkResult::Error`, so
                    // blocking under the *old* whole-network semantics would
                    // have blanked the viewport. Under cone-scoped blocking (D3)
                    // that reason is gone: the node's output genuinely is
                    // unavailable, skip-and-synthesize reports the same *fact*
                    // (in this rule's wording rather than
                    // `build_inline_closure`'s, so downstream chain text shifts
                    // slightly), and D8's dedupe now shows **one** entry instead
                    // of an amber validation row plus the red eval row it
                    // predicted. (closures `doc/design_closures.md`
                    // §"Validation" check 1 / check 2.)
                    //
                    // Deliberately does NOT set `ok = false`: `ok` drives the
                    // parent's `ZONE_BODY_INVALID_MARKER`, which would poison
                    // the *enclosing* HOF for a broken nested closure. The
                    // blocking flag alone confines the damage to this node's
                    // cone, which is the whole point of the sweep.
                    network.validation_errors.push(ValidationError::new(
                        format!("Zone-output pin '{}' has no incoming wire", pin.name),
                        Some(node_id),
                    ));
                }
            }
        }

        // Issue #417: `parameter` nodes are not allowed inside a zone body (an
        // HOF body or a `closure` body). A `parameter` declares an input pin of
        // the enclosing *network*; a body has no interface — its inputs are
        // zone-input pins and captures. `validate_parameters` only walks the
        // top-level network, so a body `parameter` never gets a coherent
        // `param_index`/`param_id` and its eval would read the enclosing HOF's
        // arguments by a stale index.
        //
        // `ancestors` is non-empty exactly when `network` is a body (Pass B
        // recurses with the extended chain), so this fires only inside bodies.
        //
        // **Blocking** since the D9 severity sweep
        // (`doc/design_error_management.md` Phase 6) — third member of the
        // "demoted only because the runtime already localizes it" class:
        // `ParameterData::eval` detects the same condition and returns a
        // localized `NetworkResult::Error`, which under the old whole-network
        // semantics would have blanked everything. The node's output is not
        // useful, so blocking is the honest severity; skip-and-synthesize now
        // reports this (more specific) text instead and D8's dedupe keeps it to
        // one entry. Every authoring path that could create one is refused up
        // front (`add_node_scoped`, `paste_at_position_scoped`,
        // `duplicate_node_scoped`, plus the add-node popup filter), so in
        // practice this rule only ever sees hand-authored or pre-#417 `.cnnd`.
        // Does NOT set `ok = false` — poisoning the enclosing HOF for one stray
        // body node is exactly the blast radius D3 shrank.
        if !ancestors.is_empty() && !allowed_in_zone_body(&node.node_type_name) {
            network.validation_errors.push(ValidationError::new(
                format!(
                    "`{}` nodes are not allowed inside a zone body — use the body's \
                     zone-input pins or a capture wire from the enclosing network instead",
                    node.node_type_name
                ),
                Some(node_id),
            ));
        }

        // Rule 4 (closures `doc/design_closures.md`, §"Validation" check 4):
        // `apply` owns no inline body to fall back to, so its required `f`
        // (Function) pin must be connected. (An HOF with a disconnected `f`
        // uses its inline body and is fine; `apply` cannot.) The `f`-source's
        // function-type/shape is checked by `validate_wires` via
        // `can_be_converted_to`, like any other typed wire.
        if node.node_type_name == "apply" && !function_input_pin_connected(node, node_type) {
            // **Blocking** since the D9 severity sweep, same rationale as the
            // zone-output rule above: `apply.eval` returns a clean localized
            // `NetworkResult::Error("apply: f not connected")` (`nodes/apply.rs`),
            // so this was demoted purely to avoid the old whole-network blank.
            // Under D3 the `apply`'s output is unavailable and only its cone
            // goes dark either way, so blocking is the honest severity and D8's
            // dedupe collapses the former amber+red pair into one entry.
            // Does NOT set `ok = false` — see the zone-output rule above.
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

        // Function pin roles (`doc/design_function_pin_roles.md`, §"Validation"):
        // a pin marked `Supplied` while unwired takes its value from the node's
        // stored property data — but a **required** pin has no stored-data
        // fallback, so invoking the synthesized function would yield a
        // localized error from deep inside whatever HOF consumed it. Surface it
        // at the node instead.
        //
        // Non-blocking (does NOT set `ok = false`) per the blast-radius litmus
        // test in `structure_designer/AGENTS.md`: the runtime already localizes
        // this into a `NetworkResult::Error` (`evaluate_arg_required` on an
        // empty body argument), so it must not blank the whole network.
        //
        // Gated on the `-1` pin actually being consumed: on an unconsumed node
        // the roles are inert, so warning there would be pure noise — and the
        // gate is what confines every validation-visible effect of a role
        // toggle to consumed nodes, which is exactly the condition the undo
        // path's conditional revalidation keys on.
        if network.function_pin_consumed(node_id) {
            let dispositions = function_pin_dispositions(node, node_type);
            for (i, disposition) in dispositions.iter().enumerate() {
                if *disposition != FunctionPinDisposition::CaptureStored {
                    continue;
                }
                let param_name = &node_type.parameters[i].name;
                if !parameter_is_required(node, param_name) {
                    continue;
                }
                network.validation_errors.push(ValidationError::warning(
                    format!(
                        "Input pin '{}' is marked Supplied but is unwired and required \
                         (it has no stored value to bake into the function)",
                        param_name
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

        // D5: a body's `valid` flag follows the same interface-residue
        // predicate as a top-level network's (bodies have no parameter
        // interface, so in practice this only flips on an unattributed
        // blocking error). The blast-radius vehicle for body errors is the
        // marker below, not this flag.
        {
            let body = Arc::make_mut(&mut body_arc);
            body.valid = !crate::structure_designer::node_network::has_interface_residue(
                &body.validation_errors,
            );
        }

        if !body_inner_ok {
            ok = false;
            // The marker is a blocking error attributed to the zone-owning
            // node: under cone-scoped blocking (D3) it poisons the owner —
            // the node whose eval would run the broken body — while the rest
            // of the parent network keeps evaluating.
            network.validation_errors.push(ValidationError::new(
                ZONE_BODY_INVALID_MARKER.to_string(),
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
                    id: pin.id,
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

    old_output_pins.len() != new_output_pins.len()
        || old_output_pins
            .iter()
            .zip(new_output_pins.iter())
            .any(|(old, new)| old.name != new.name || old.data_type != new.data_type)
}
