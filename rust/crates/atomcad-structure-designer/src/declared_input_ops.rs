//! The two operations on a node's declared inputs
//! (`doc/design_wire_declared_inputs.md`): the **open port** — dropping a wire
//! on a node appends an input of the wire's type and connects it — and **Take
//! type from wire**, which retypes a stale declared input to the type its wire
//! now carries.
//!
//! Both reach the node only through [`NodeData::declared_inputs`] /
//! `declared_inputs_mut` and never name a node type, so a node that
//! implements [`DeclaredInputs`] gets both with no change here. Pins are
//! addressed by **pin id** across the re-derivation of the node type; an
//! index is read back only after the type has been rebuilt.
//!
//! [`NodeData::declared_inputs`]: crate::node_data::NodeData::declared_inputs
//! [`DeclaredInputs`]: crate::declared_inputs::DeclaredInputs

use crate::data_type::{DataType, RecordType};
use crate::declared_inputs::{NameSource, derive_open_port_name, next_declared_input_name};
use crate::node_network::{IncomingWire, NodeNetwork, SourcePin};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::network_snapshot::NetworkSnapshotCommand;
use std::collections::HashSet;

/// The undo-history label of an open-port drop.
pub const ADD_INPUT_FROM_WIRE_DESCRIPTION: &str = "Add input from wire";
/// The undo-history label of a retype from wires.
pub const TAKE_TYPE_FROM_WIRE_DESCRIPTION: &str = "Take type from wire";

/// What an open-port drop created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedInput {
    /// The new input's pin index — also its row in the node's editor list.
    pub pin_index: usize,
    /// The new input's pin id (`DeclaredInputs::append`'s return).
    pub pin_id: u64,
    /// The name the input was given.
    pub name: String,
    /// The name derived from the source, when the node named the input
    /// otherwise (`expr`'s letters) and the source has a name of its own: the
    /// panel offers it as a one-click rename. `None` when it would be `name`.
    pub source_name: Option<String>,
}

/// Everything the drop needs, gathered by the read-only checks
/// ([`StructureDesigner::check_add_input_from_wire`]).
pub(crate) struct AddInputPlan {
    pub(crate) network_name: String,
    pub(crate) data_type: DataType,
    pub(crate) name: String,
    pub(crate) source_name: Option<String>,
}

/// The offered type for one declared input, or `None`: the four conditions of
/// the design's "Take type from wire — when it is offered" row, minus the
/// `declares_pin` / one-wire checks the caller has already made.
fn offered_type(
    source: &DataType,
    declared: &DataType,
    registry: &NodeTypeRegistry,
) -> Option<DataType> {
    if !is_stale_record_wire(source, declared, registry) {
        return None;
    }
    // An anonymous declared input stays anonymous: `expr` cannot read a named
    // record's fields, so offering the name would swap a stale type for a
    // broken one.
    if let (
        DataType::Record(source_record @ RecordType::Named(_)),
        DataType::Record(RecordType::Anonymous(_)),
    ) = (source, declared)
        && let Some(fields) = source_record.resolve_fields(registry)
    {
        return Some(DataType::Record(RecordType::anonymous(fields.to_vec())));
    }
    Some(source.clone())
}

/// Whether a wire carrying `source` into a pin typed `declared` is **stale**:
/// the source fails to convert, or both are records whose resolved,
/// canonically sorted fields differ ([`records_differ`]) — so a *wider* record
/// source is stale too, although the wire stays valid under width subtyping.
/// The one staleness rule of *Take type from wire*, for declared inputs and
/// for a reader's record pin (`record_input_type_ops.rs`) alike.
pub(crate) fn is_stale_record_wire(
    source: &DataType,
    declared: &DataType,
    registry: &NodeTypeRegistry,
) -> bool {
    !DataType::can_be_converted_to(source, declared, registry)
        || records_differ(source, declared, registry)
}

/// Whether both types are records whose resolved, canonically sorted field
/// lists differ (names and types). Not `DataType` equality: a named record
/// with exactly the declared anonymous shape does **not** differ. A record
/// that cannot be resolved (a dangling name) is left to the conversion check.
fn records_differ(a: &DataType, b: &DataType, registry: &NodeTypeRegistry) -> bool {
    let (DataType::Record(a), DataType::Record(b)) = (a, b) else {
        return false;
    };
    match (a.resolve_fields(registry), b.resolve_fields(registry)) {
        (Some(a_fields), Some(b_fields)) => a_fields != b_fields,
        _ => false,
    }
}

impl StructureDesigner {
    /// The resolved type of the source a wire describes, as seen from
    /// `dest_scope_path` (the network that stores the wire). A node output
    /// resolves through `resolve_output_type_scoped`, which follows
    /// `SameAsInput` up the chain; the function pin (`-1`) to the node's
    /// function signature; a zone input to the HOF's zone-input pin type at
    /// `source_scope_depth`. It is the same resolver the function-pin witness
    /// uses (`NodeTypeRegistry::resolve_wire_source_type_scoped`), and for a
    /// top-level wire the same resolution `validate_wires` checks the wire
    /// against, so an offer and the validator agree about a mismatch.
    pub fn resolve_wire_source_type(
        &self,
        dest_scope_path: &[u64],
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Option<DataType> {
        let network = self.get_scope_network(dest_scope_path)?;
        let (ancestors, ancestor_hof_ids) = self.get_scope_ancestors(dest_scope_path)?;
        let wire = IncomingWire {
            source_node_id,
            source_pin,
            source_scope_depth,
        };
        self.node_type_registry
            .resolve_wire_source_type_scoped(&wire, network, &ancestors, &ancestor_hof_ids)
            .map(|resolved| resolved.data_type)
    }

    /// The hover check of the open port: `None` when dropping this wire on
    /// `dest_node_id`'s `+` would succeed, `Some(reason)` when it would be
    /// refused. Runs exactly the checks [`Self::add_input_from_wire`] runs
    /// before it changes anything, so the two can never disagree.
    pub fn can_add_input_from_wire(
        &self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Option<String> {
        self.preview_add_input_from_wire(
            dest_scope_path,
            dest_node_id,
            source_node_id,
            source_pin,
            source_scope_depth,
        )
        .err()
    }

    /// [`Self::can_add_input_from_wire`] with the answer the hovered `+` tab
    /// shows: `Ok(name)` — the name the drop would give the new input — or
    /// `Err(reason)`. The same checks as the drop, so the preview is exact.
    pub fn preview_add_input_from_wire(
        &self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Result<String, String> {
        self.check_add_input_from_wire(
            dest_scope_path,
            dest_node_id,
            source_node_id,
            source_pin,
            source_scope_depth,
        )
        .map(|plan| plan.name)
    }

    /// Steps 1–4 of the drop: every refusal, and the type and name the new
    /// input will get. Changes nothing.
    fn check_add_input_from_wire(
        &self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Result<AddInputPlan, String> {
        // 1. Read-only network, destination without declared inputs.
        self.ensure_active_editable()?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let network = self
            .get_scope_network(dest_scope_path)
            .ok_or_else(|| "Scope not found".to_string())?;
        let dest_node = network
            .nodes
            .get(&dest_node_id)
            .ok_or_else(|| "Node not found".to_string())?;
        let declared = dest_node
            .data
            .declared_inputs()
            .ok_or_else(|| "This node's inputs are not declared by wires".to_string())?;
        if source_scope_depth == 0
            && source_node_id == dest_node_id
            && matches!(source_pin, SourcePin::NodeOutput { .. })
        {
            return Err("A node cannot take its own output as an input".to_string());
        }

        // 2. The source's resolved type.
        let data_type = self
            .resolve_wire_source_type(
                dest_scope_path,
                source_node_id,
                source_pin,
                source_scope_depth,
            )
            .ok_or_else(|| "The source's type cannot be resolved".to_string())?;
        if data_type == DataType::None {
            return Err("The source has no type".to_string());
        }

        // 3. The node's type gate.
        declared.accepts_type(&data_type)?;

        // 4. The derived name.
        let name_source = self
            .open_port_name_source(
                dest_scope_path,
                source_node_id,
                source_pin,
                source_scope_depth,
            )
            .ok_or_else(|| "Source node not found".to_string())?;
        let taken = self.input_pin_names(dest_node);
        let source = NameSource {
            node_name: name_source.0.as_deref(),
            pin_name: name_source.1.as_deref(),
            prefer_pin: name_source.2,
        };
        let name = next_declared_input_name(declared, Some(&source), &taken);
        // The source's own name, offered as a rename when the node chose
        // another (a name pool). A source with no name offers nothing: the
        // derivation would only fall back.
        let source_name = (source.node_name.is_some() || source.pin_name.is_some())
            .then(|| {
                derive_open_port_name(
                    &source,
                    &taken,
                    declared.reserved_names(),
                    declared.fallback_name(),
                )
            })
            .filter(|derived| *derived != name);

        Ok(AddInputPlan {
            network_name,
            data_type,
            name,
            source_name,
        })
    }

    /// The name the node's next declared input would get with no wire to name
    /// it after — what the panel's add button uses, so it names inputs the way
    /// the open port does (`expr`: the next free letter). `None` when the node
    /// has no declared inputs. Read-only.
    pub fn next_declared_input_name(&self, scope_path: &[u64], node_id: u64) -> Option<String> {
        let node = self.get_scope_network(scope_path)?.nodes.get(&node_id)?;
        let declared = node.data.declared_inputs()?;
        Some(next_declared_input_name(
            declared,
            None,
            &self.input_pin_names(node),
        ))
    }

    /// The names of `node`'s input pins.
    fn input_pin_names(&self, node: &crate::node_network::Node) -> HashSet<String> {
        self.node_type_registry
            .get_node_type_for_node(node)
            .map(|node_type| {
                node_type
                    .parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `(node name, pin name, prefer pin)` for [`NameSource`]: the node is the
    /// source node (for a zone input, the HOF), the pin its output pin (for a
    /// zone input, the zone-input pin). `None` when the source does not exist.
    pub(crate) fn open_port_name_source(
        &self,
        dest_scope_path: &[u64],
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Option<(Option<String>, Option<String>, bool)> {
        let depth = source_scope_depth as usize;
        if depth > dest_scope_path.len() {
            return None;
        }
        let source_network =
            self.get_scope_network(&dest_scope_path[..dest_scope_path.len() - depth])?;
        let source_node = source_network.nodes.get(&source_node_id)?;
        let node_type = self
            .node_type_registry
            .get_node_type_for_node(source_node)?;
        let node_name = source_node.custom_name.clone();
        Some(match source_pin {
            // The function pin stands for the whole node.
            SourcePin::NodeOutput { pin_index } if pin_index < 0 => (node_name, None, false),
            SourcePin::NodeOutput { pin_index } => (
                node_name,
                node_type
                    .output_pins
                    .get(pin_index as usize)
                    .map(|pin| pin.name.clone()),
                node_type.output_pin_count() > 1,
            ),
            SourcePin::ZoneInput { pin_index } => (
                node_name,
                node_type
                    .zone_input_pins
                    .get(pin_index)
                    .map(|pin| pin.name.clone()),
                true,
            ),
        })
    }

    /// The open-port drop: append an input of the wire's type to
    /// `dest_node_id`, under a derived name, and connect the wire to it — one
    /// undo step ("Add input from wire"). Takes the same source description
    /// as `connect_wire_scoped`. Every refusal leaves the network untouched
    /// and pushes no undo step.
    pub fn add_input_from_wire(
        &mut self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Result<AddedInput, String> {
        // Steps 1–4: check only.
        let plan = self.check_add_input_from_wire(
            dest_scope_path,
            dest_node_id,
            source_node_id,
            source_pin,
            source_scope_depth,
        )?;

        // 5. Snapshot, and the pre-edit footprints for reflow.
        let before = self
            .snapshot_network(&plan.network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;
        let old_sizes = self.capture_footprint_chain(dest_scope_path, dest_node_id);

        // 6. Append, re-derive the node type, find the pin by id, connect.
        let added = self.append_declared_input_and_connect(
            dest_scope_path,
            dest_node_id,
            &plan,
            source_node_id,
            source_pin,
            source_scope_depth,
        )?;

        // 7. Reflow neighbours if the node grew (the moves are inside the
        //    after-snapshot, so they need no command of their own), revalidate,
        //    mark dirty, push one step.
        self.reflow_for_footprint_change(dest_scope_path, dest_node_id, &old_sizes);
        if dest_scope_path.is_empty() {
            // As `connect_nodes` does for a top-level wire.
            let dirty: HashSet<u64> = [source_node_id, dest_node_id].into_iter().collect();
            self.apply_node_display_policy(Some(&dirty));
        }
        self.finish_declared_input_edit(
            dest_scope_path,
            dest_node_id,
            &plan.network_name,
            before,
            ADD_INPUT_FROM_WIRE_DESCRIPTION,
        );
        Ok(added)
    }

    /// Step 6 of [`Self::add_input_from_wire`]. The plan has passed every
    /// check, so a failure here means the network changed underneath it.
    pub(crate) fn append_declared_input_and_connect(
        &mut self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        plan: &AddInputPlan,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Result<AddedInput, String> {
        let pin_id = {
            let node = self
                .get_scope_network_mut(dest_scope_path)
                .and_then(|network| network.nodes.get_mut(&dest_node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            node.data
                .declared_inputs_mut()
                .ok_or_else(|| "This node's inputs are not declared by wires".to_string())?
                .append(plan.name.clone(), plan.data_type.clone())?
        };
        self.rederive_node_type(dest_scope_path, dest_node_id);

        let network = self
            .get_scope_network(dest_scope_path)
            .ok_or_else(|| "Scope not found".to_string())?;
        let (pin_index, is_multi) =
            pin_index_for_id(network, &self.node_type_registry, dest_node_id, pin_id)
                .ok_or_else(|| "The new input has no pin".to_string())?;
        if let Some(network) = self.get_scope_network_mut(dest_scope_path) {
            network.connect_wire(
                source_node_id,
                source_pin,
                source_scope_depth,
                dest_node_id,
                pin_index,
                is_multi,
            );
        }
        Ok(AddedInput {
            pin_index,
            pin_id,
            name: plan.name.clone(),
            source_name: plan.source_name.clone(),
        })
    }

    /// Re-derive one node's custom node type from its data, rebuilding its
    /// arguments by pin id (`Node::set_custom_node_type` with `refresh_args`),
    /// so every existing wire stays on its pin.
    pub(crate) fn rederive_node_type(&mut self, scope_path: &[u64], node_id: u64) {
        let Some(network_name) = self.active_node_network_name.clone() else {
            return;
        };
        let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
            &self.node_type_registry.built_in_node_types,
            &self.node_type_registry.record_type_defs,
            &self.node_type_registry.built_in_record_type_defs,
            &mut self.node_type_registry.node_networks,
        );
        let mut current: Option<&mut NodeNetwork> = node_networks.get_mut(&network_name);
        for hof_id in scope_path {
            current = current.and_then(|net| net.nodes.get_mut(hof_id).and_then(|n| n.zone_mut()));
        }
        if let Some(node) = current.and_then(|net| net.nodes.get_mut(&node_id)) {
            NodeTypeRegistry::populate_custom_node_type_cache_with_types(
                built_in_types,
                record_type_defs,
                built_in_record_type_defs,
                node,
                true,
            );
        }
    }

    /// The shared tail of both operations: revalidate, mark the design dirty
    /// and the node changed, request a full refresh (the node's output type
    /// can change, which can clear or raise errors downstream), and push one
    /// [`NetworkSnapshotCommand`] labelled `description`.
    pub(crate) fn finish_declared_input_edit(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        network_name: &str,
        before: crate::serialization::node_networks_serialization::SerializableNodeNetwork,
        description: &str,
    ) {
        if let Some(command) =
            self.finish_network_edit(scope_path, node_id, network_name, before, description)
        {
            self.push_command(command);
        }
    }

    /// [`Self::finish_declared_input_edit`] without the push: the command is
    /// returned for a caller that bundles it with another (promotion's
    /// `CompositeCommand`). `None` only when the network cannot be
    /// snapshotted.
    pub(crate) fn finish_network_edit(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        network_name: &str,
        before: crate::serialization::node_networks_serialization::SerializableNodeNetwork,
        description: &str,
    ) -> Option<NetworkSnapshotCommand> {
        self.validate_active_network();
        self.set_dirty(true);
        self.mark_node_data_changed_scoped(scope_path, node_id);
        self.mark_full_refresh();
        let after = self.snapshot_network(network_name)?;
        Some(NetworkSnapshotCommand {
            network_name: network_name.to_string(),
            description: description.to_string(),
            before_snapshot: before,
            after_snapshot: after,
        })
    }

    /// One entry per input pin of the node, by pin index: `Some(type)` when
    /// Take type from wire is offered for that pin, carrying the type it would
    /// set. Empty when the node has no declared inputs (or does not exist).
    /// Read-only: no refresh, no undo.
    ///
    /// A pin is offered exactly when (1) the node declares it, (2) exactly one
    /// wire is on it, (3) the wire's resolved source type is stale against the
    /// declared type — it fails `can_be_converted_to`, or both are records
    /// whose resolved fields differ — and (4) the node accepts the offered
    /// type. The offered type is the source's, except that an anonymous
    /// declared record is offered the anonymous form of a named source.
    ///
    /// A node with a record-typed input (`record_destructure`) is offered on
    /// its record pin instead, by the reader's rule
    /// ([`Self::record_input_type_offers`]).
    pub fn wire_type_offers(&self, scope_path: &[u64], node_id: u64) -> Vec<Option<DataType>> {
        if let Some(offers) = self.record_input_type_offers(scope_path, node_id) {
            return offers;
        }
        let Some(network) = self.get_scope_network(scope_path) else {
            return Vec::new();
        };
        let Some((ancestors, ancestor_hof_ids)) = self.get_scope_ancestors(scope_path) else {
            return Vec::new();
        };
        let Some(node) = network.nodes.get(&node_id) else {
            return Vec::new();
        };
        let Some(declared) = node.data.declared_inputs() else {
            return Vec::new();
        };
        let registry = &self.node_type_registry;
        let Some(node_type) = registry.get_node_type_for_node(node) else {
            return Vec::new();
        };
        node_type
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let pin_id = parameter.id?;
                if !declared.declares_pin(pin_id) {
                    return None;
                }
                let argument = node.arguments.get(index)?;
                let [wire] = argument.incoming_wires.as_slice() else {
                    return None;
                };
                let source = registry
                    .resolve_wire_source_type_scoped(wire, network, &ancestors, &ancestor_hof_ids)?
                    .data_type;
                let offered = offered_type(&source, &parameter.data_type, registry)?;
                declared.accepts_type(&offered).ok()?;
                Some(offered)
            })
            .collect()
    }

    /// Retype each listed declared input to its offered type
    /// ([`Self::wire_type_offers`]), in one undo step ("Take type from
    /// wire"). The offers are recomputed here, never trusted from the caller,
    /// and the operation is all or nothing: an empty list, a read-only
    /// network, or any listed index without an offer is refused before
    /// anything changes. On a node with a record-typed input, the record pin's
    /// offer is *adopted* (`record_input_type_ops.rs`): pins, output wires and
    /// reflow, refused with the node's reason when it cannot hold the type.
    pub fn take_types_from_wires(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        pin_indices: &[usize],
    ) -> Result<(), String> {
        // 1. Refusals.
        self.ensure_active_editable()?;
        if pin_indices.is_empty() {
            return Err("No inputs to retype".to_string());
        }
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let offers = self.wire_type_offers(scope_path, node_id);
        let is_reader = self
            .get_scope_network(scope_path)
            .and_then(|network| network.nodes.get(&node_id))
            .is_some_and(|node| node.data.record_typed_input().is_some());
        if is_reader {
            return self.take_record_input_type_from_wire(
                scope_path,
                node_id,
                pin_indices,
                &offers,
                TAKE_TYPE_FROM_WIRE_DESCRIPTION,
            );
        }
        let retypes: Vec<(u64, DataType)> = {
            let node_type = self
                .get_scope_network(scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| self.node_type_registry.get_node_type_for_node(node))
                .ok_or_else(|| "Node not found".to_string())?;
            pin_indices
                .iter()
                .map(|&index| {
                    let offered = offers.get(index).cloned().flatten().ok_or_else(|| {
                        format!("Input {} has no type to take from its wire", index)
                    })?;
                    let pin_id = node_type.parameters[index]
                        .id
                        .expect("an offered pin has an id");
                    Ok((pin_id, offered))
                })
                .collect::<Result<_, String>>()?
        };

        // 2. Snapshot.
        let before = self
            .snapshot_network(&network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;

        // 3. Retype. Retyping never changes a node's footprint: no reflow.
        {
            let node = self
                .get_scope_network_mut(scope_path)
                .and_then(|network| network.nodes.get_mut(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let declared = node
                .data
                .declared_inputs_mut()
                .ok_or_else(|| "This node's inputs are not declared by wires".to_string())?;
            for (pin_id, data_type) in retypes {
                declared.retype(pin_id, data_type)?;
            }
        }

        // 4. Re-derive the node type; ids are unchanged, so every wire stays.
        self.rederive_node_type(scope_path, node_id);

        // 5. Revalidate, dirty, push.
        self.finish_declared_input_edit(
            scope_path,
            node_id,
            &network_name,
            before,
            TAKE_TYPE_FROM_WIRE_DESCRIPTION,
        );
        Ok(())
    }
}

/// The index and multi-ness of the input pin whose `Parameter` carries
/// `pin_id`.
fn pin_index_for_id(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    node_id: u64,
    pin_id: u64,
) -> Option<(usize, bool)> {
    let node = network.nodes.get(&node_id)?;
    let node_type = registry.get_node_type_for_node(node)?;
    node_type
        .parameters
        .iter()
        .position(|p| p.id == Some(pin_id))
        .map(|index| (index, node_type.parameters[index].data_type.is_array()))
}
