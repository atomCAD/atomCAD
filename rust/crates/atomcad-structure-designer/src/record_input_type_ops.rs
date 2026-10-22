//! The operations on a node whose pins are derived from the record type of its
//! `record` input (`doc/design_anonymous_record_readers.md`): adopting a type
//! from a wire — on connect while the node is uncommitted, and by *Take type
//! from wire* — and switching it from the schema dropdown.
//!
//! Every operation reaches the node only through
//! [`NodeData::record_typed_input`] / `record_typed_input_mut` and the
//! local-field hooks, and never names a node type, so a node that implements
//! [`RecordTypedInput`] gets all of them with no change here.
//!
//! An adoption can change, add and remove output pins, and the wires out of
//! them live on the consumers. So the old pins are captured **before** the new
//! data is installed, the node type is re-derived, and the consumer wires are
//! moved by [`remap_record_output_wires`]. None of this pushes undo: each
//! caller records the one step its trigger calls for.
//!
//! [`NodeData::record_typed_input`]: crate::node_data::NodeData::record_typed_input
//! [`RecordTypedInput`]: crate::record_typed_input::RecordTypedInput

use crate::data_type::{DataType, RecordType};
use crate::declared_input_ops::is_stale_record_wire;
use crate::node_data::NodeData;
use crate::node_network::{IncomingWire, NodeNetwork, SourcePin};
use crate::structure_designer::StructureDesigner;
use std::collections::HashSet;

/// The undo-history label of a connect that also adopts the wire's type.
pub const CONNECT_WIRE_DESCRIPTION: &str = "Connect wire";
/// The undo-history label of a schema picked from a reader's dropdown.
pub const SET_RECORD_INPUT_SCHEMA_DESCRIPTION: &str = "Set record schema";

/// Whether the node `node_id` of `network` is an **uncommitted** record reader:
/// it has a record-typed input, and nothing depends on its pins and no wire
/// touches it except those into its `record` pin. Concretely: it is not the
/// network's return node (whose pins are the interface every call site
/// depends on); no wire leaves any of its output pins, the function pin
/// included — in the network's own arguments, in the `zone_output_arguments`
/// of a HOF in it, or as a capture from any body nested below it; no
/// body-return wire of `owner` (the HOF whose body `network` is, if any) reads
/// it; and no wire comes into another of its input pins. An unset reader with
/// nothing wired out is always uncommitted.
pub fn is_uncommitted_record_reader(
    network: &NodeNetwork,
    owner: Option<&crate::node_network::Node>,
    node_id: u64,
) -> bool {
    let Some(node) = network.nodes.get(&node_id) else {
        return false;
    };
    let Some(typed_input) = node.data.record_typed_input() else {
        return false;
    };
    if network.return_node_id == Some(node_id) {
        return false;
    }
    let wired_elsewhere = node.arguments.iter().enumerate().any(|(index, argument)| {
        !typed_input.is_record_input_pin(index) && !argument.incoming_wires.is_empty()
    });
    if wired_elsewhere {
        return false;
    }
    if let Some(owner) = owner
        && owner
            .zone_output_arguments
            .iter()
            .any(|argument| argument.incoming_wires.iter().any(|w| reads(w, node_id, 0)))
    {
        return false;
    }
    !has_consumer(network, node_id, 0)
}

/// Whether `wire` reads an output of `node_id`, a node `depth` networks up
/// from the network that stores the wire.
fn reads(wire: &IncomingWire, node_id: u64, depth: usize) -> bool {
    wire.source_node_id == node_id
        && wire.source_scope_depth as usize == depth
        && matches!(wire.source_pin, SourcePin::NodeOutput { .. })
}

/// Whether a wire in `network` or in any body below it reads `node_id`, which
/// lives `level` networks up from `network`.
fn has_consumer(network: &NodeNetwork, node_id: u64, level: usize) -> bool {
    network.nodes.values().any(|consumer| {
        consumer
            .arguments
            .iter()
            .any(|argument| argument.incoming_wires.iter().any(|w| reads(w, node_id, level)))
            // A body-return wire is stored on the HOF but resolved from its
            // body, one network further down.
            || consumer
                .zone_output_arguments
                .iter()
                .any(|argument| {
                    argument
                        .incoming_wires
                        .iter()
                        .any(|w| reads(w, node_id, level + 1))
                })
            || consumer
                .zone
                .as_deref()
                .is_some_and(|body| has_consumer(body, node_id, level + 1))
    })
}

/// The output pins of a node, as `(id, name)` in pin order: what
/// [`remap_record_output_wires`] needs from the layout before a change.
pub type OutputPinKeys = Vec<(Option<u64>, String)>;

/// For each old output pin, the index of the new pin its wires move to, or
/// `None` when its wires are dropped. A pin without an id (the unset node's
/// placeholder) neither keeps nor receives a wire. With `same_id_space` an id
/// match wins and a name match is the fallback; otherwise names alone decide,
/// because ids from two different defs (or a def and the local ids) can
/// coincide without naming the same field. No new pin receives two old ones.
fn pin_mapping(
    old: &[(Option<u64>, String)],
    new: &OutputPinKeys,
    same_id_space: bool,
) -> Vec<Option<usize>> {
    let mut mapping: Vec<Option<usize>> = vec![None; old.len()];
    let mut taken: HashSet<usize> = HashSet::new();
    if same_id_space {
        for (slot, (id, _)) in mapping.iter_mut().zip(old) {
            let Some(id) = id else { continue };
            if let Some(index) = new.iter().position(|(new_id, _)| *new_id == Some(*id))
                && taken.insert(index)
            {
                *slot = Some(index);
            }
        }
    }
    for (slot, (id, name)) in mapping.iter_mut().zip(old) {
        if slot.is_some() || id.is_none() {
            continue;
        }
        if let Some(index) = new
            .iter()
            .position(|(new_id, new_name)| new_id.is_some() && new_name == name)
            && taken.insert(index)
        {
            *slot = Some(index);
        }
    }
    mapping
}

/// Move one wire read by `mapping`; `false` drops it.
fn remap_wire(
    wire: &mut IncomingWire,
    node_id: u64,
    depth: usize,
    mapping: &[Option<usize>],
) -> bool {
    if !reads(wire, node_id, depth) {
        return true;
    }
    let SourcePin::NodeOutput { pin_index } = wire.source_pin else {
        return true;
    };
    if pin_index < 0 {
        return true; // the function pin is not a field
    }
    match mapping.get(pin_index as usize).copied().flatten() {
        Some(new_index) => {
            wire.source_pin = SourcePin::NodeOutput {
                pin_index: new_index as i32,
            };
            true
        }
        None => false,
    }
}

/// Apply `mapping` to every wire in `network` and the bodies below it that
/// reads `node_id`, `level` networks up.
fn remap_consumers(
    network: &mut NodeNetwork,
    node_id: u64,
    level: usize,
    mapping: &[Option<usize>],
) {
    for consumer in network.nodes.values_mut() {
        for argument in consumer.arguments.iter_mut() {
            argument
                .incoming_wires
                .retain_mut(|w| remap_wire(w, node_id, level, mapping));
        }
        for argument in consumer.zone_output_arguments.iter_mut() {
            argument
                .incoming_wires
                .retain_mut(|w| remap_wire(w, node_id, level + 1, mapping));
        }
        if consumer
            .zone
            .as_deref()
            .is_some_and(|body| has_consumer(body, node_id, level + 1))
            && let Some(body) = consumer.zone_mut()
        {
            remap_consumers(body, node_id, level + 1, mapping);
        }
    }
}

/// Move the wires out of `node_id`'s output pins from the layout `old_pins`
/// to the node's current (re-derived) layout, dropping the wires of fields
/// that are gone. Ids are compared only when `same_id_space` (the same def
/// before and after, or anonymous before and after); otherwise fields are
/// matched by name alone (see [`pin_mapping`]). Covers the network's own
/// wires, body-return wires and captures from every body below it; the
/// owning HOF's body-return wires are the caller's (they are stored one
/// network up). Returns the mapping it applied.
///
/// `repair_node_network`'s R3 pass is deliberately **not** this: on a def
/// edit a deleted field must not be re-pointed by name at a new field.
pub fn remap_record_output_wires(
    network: &mut NodeNetwork,
    node_id: u64,
    old_pins: &[(Option<u64>, String)],
    same_id_space: bool,
) -> Vec<Option<usize>> {
    let new_pins: OutputPinKeys = network
        .nodes
        .get(&node_id)
        .and_then(|node| node.custom_node_type.as_ref())
        .map(|node_type| {
            node_type
                .output_pins
                .iter()
                .map(|pin| (pin.id, pin.name.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mapping = pin_mapping(old_pins, &new_pins, same_id_space);
    remap_consumers(network, node_id, 0, &mapping);
    mapping
}

/// The record pin's single wire, when there is exactly one.
fn single_record_wire(network: &NodeNetwork, node_id: u64) -> Option<(usize, IncomingWire)> {
    let node = network.nodes.get(&node_id)?;
    let typed_input = node.data.record_typed_input()?;
    let (index, argument) = node
        .arguments
        .iter()
        .enumerate()
        .find(|(index, _)| typed_input.is_record_input_pin(*index))?;
    match argument.incoming_wires.as_slice() {
        [wire] => Some((index, wire.clone())),
        _ => None,
    }
}

/// The authored field order of the source of `wire`, when that source is an
/// anonymous `record_construct` read straight off its record output — the
/// order an anonymous *type* has lost (`RecordType::anonymous` sorts its
/// fields). `None` for any other source. `source_network` is the network the
/// source lives in. The constructor is told apart from a reader by its local
/// fields with no record-typed input of its own.
pub fn authored_field_order(
    source_network: &NodeNetwork,
    wire: &IncomingWire,
) -> Option<Vec<String>> {
    let SourcePin::NodeOutput { pin_index: 0 } = wire.source_pin else {
        return None;
    };
    let source = source_network.nodes.get(&wire.source_node_id)?;
    if source.data.record_typed_input().is_some() || source.data.record_schema_ref() != Some("") {
        return None;
    }
    let (fields, _) = source.data.local_record_fields()?;
    Some(fields.iter().map(|f| f.name.clone()).collect())
}

/// Put `data`'s local fields (its pins) in `order`, when `order` names exactly
/// those fields. Pin order is cosmetic: output wires are remapped by
/// identity, never by position.
pub fn apply_field_order(data: &mut dyn NodeData, order: &[String]) {
    let Some(fields) = data.local_record_fields_mut() else {
        return;
    };
    let names: HashSet<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    if names.len() == order.len() && order.iter().all(|n| names.contains(n.as_str())) {
        fields.sort_by_key(|f| order.iter().position(|n| *n == f.name));
    }
}

/// What makes a reader's data "the same type" for the purposes of adopting:
/// its input type and the order of its local fields (which is its pin order).
fn reader_shape(data: &dyn NodeData) -> (Option<RecordType>, Vec<String>) {
    let input_type = data
        .record_typed_input()
        .and_then(|typed_input| typed_input.record_input_type());
    let order = data
        .local_record_fields()
        .map(|(fields, _)| fields.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();
    (input_type, order)
}

/// Whether two input types are in one id space: the same def, or both
/// anonymous (whose pins carry local ids, kept by name). Anything else — two
/// defs, or a switch between modes — matches output wires by name alone.
fn same_id_space(old: &Option<RecordType>, new: &Option<RecordType>) -> bool {
    match (old, new) {
        (Some(RecordType::Named(a)), Some(RecordType::Named(b))) => a == b,
        (Some(RecordType::Anonymous(_)), Some(RecordType::Anonymous(_))) => true,
        _ => false,
    }
}

impl StructureDesigner {
    /// [`authored_field_order`] for a wire stored in `dest_scope_path`.
    fn authored_field_order(
        &self,
        dest_scope_path: &[u64],
        wire: &IncomingWire,
    ) -> Option<Vec<String>> {
        let depth = wire.source_scope_depth as usize;
        if depth > dest_scope_path.len() {
            return None;
        }
        let source_network =
            self.get_scope_network(&dest_scope_path[..dest_scope_path.len() - depth])?;
        authored_field_order(source_network, wire)
    }

    /// The data `node_id` would have after adopting `t`, or `Ok(None)` when
    /// adopting would change nothing. When `wire` is the reader's record wire
    /// and comes straight from an anonymous `record_construct`, the adopted
    /// fields take that node's authored order. `Err` (the node's reason) when
    /// the node cannot hold a field of `t`. Read-only.
    fn plan_record_input_type(
        &self,
        scope_path: &[u64],
        node_id: u64,
        t: &RecordType,
        wire: Option<&IncomingWire>,
    ) -> Result<Option<Box<dyn NodeData>>, String> {
        let node = self
            .get_scope_network(scope_path)
            .and_then(|network| network.nodes.get(&node_id))
            .ok_or_else(|| "Node not found".to_string())?;
        let mut data = node.data.clone_box();
        data.record_typed_input_mut()
            .ok_or_else(|| "This node does not take its type from a record wire".to_string())?
            .set_record_input_type(t, self.node_type_registry.record_defs())?;
        if matches!(t, RecordType::Anonymous(_))
            && let Some(order) = wire.and_then(|w| self.authored_field_order(scope_path, w))
        {
            apply_field_order(data.as_mut(), &order);
        }
        if reader_shape(data.as_ref()) == reader_shape(node.data.as_ref()) {
            return Ok(None);
        }
        Ok(Some(data))
    }

    /// Install adopted reader data: capture the old output pins and the
    /// footprints, install, re-derive the node type, move the consumer wires
    /// ([`remap_record_output_wires`], plus the owning HOF's body-return wires)
    /// and reflow neighbours. With `cascade`, the reflow climbs the scope chain;
    /// without it, it stays in the node's own scope (a caller whose undo step
    /// reflows the ancestors itself). Records no undo.
    fn install_record_input_data(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        data: Box<dyn NodeData>,
        cascade: bool,
    ) -> Result<(), String> {
        let mut old_sizes = self.capture_footprint_chain(scope_path, node_id);
        if !cascade {
            old_sizes.truncate(1);
        }
        let (old_pins, old_type): (OutputPinKeys, Option<RecordType>) = {
            let node = self
                .get_scope_network(scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            let pins = self
                .node_type_registry
                .get_node_type_for_node(node)
                .map(|node_type| {
                    node_type
                        .output_pins
                        .iter()
                        .map(|pin| (pin.id, pin.name.clone()))
                        .collect()
                })
                .unwrap_or_default();
            (pins, reader_shape(node.data.as_ref()).0)
        };
        let new_type = reader_shape(data.as_ref()).0;
        {
            let node = self
                .get_scope_network_mut(scope_path)
                .and_then(|network| network.nodes.get_mut(&node_id))
                .ok_or_else(|| "Node not found".to_string())?;
            node.data = data;
        }
        self.rederive_node_type(scope_path, node_id);

        let same = same_id_space(&old_type, &new_type);
        let mapping = match self.get_scope_network_mut(scope_path) {
            Some(network) => remap_record_output_wires(network, node_id, &old_pins, same),
            None => Vec::new(),
        };
        // A body-return wire out of the reader is stored on the HOF that owns
        // its body, one network up.
        if let Some((hof_id, parent)) = scope_path.split_last()
            && let Some(hof) = self
                .get_scope_network_mut(parent)
                .and_then(|network| network.nodes.get_mut(hof_id))
        {
            for argument in hof.zone_output_arguments.iter_mut() {
                argument
                    .incoming_wires
                    .retain_mut(|w| remap_wire(w, node_id, 0, &mapping));
            }
        }
        self.reflow_for_footprint_change(scope_path, node_id, &old_sizes);
        Ok(())
    }

    /// Adopt the record type `t` on the reader `node_id` — data, pins, output
    /// wires and reflow, with no undo of its own (each caller wraps it in the
    /// step its trigger calls for). `wire`, when given, is the reader's record
    /// wire, used for the authored field order of an anonymous
    /// `record_construct` source. `Ok(false)` when there was nothing to change;
    /// `Err` (nothing changed) when the node cannot hold a field of `t`.
    pub fn adopt_record_input_type(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        t: &RecordType,
        wire: Option<&IncomingWire>,
    ) -> Result<bool, String> {
        match self.plan_record_input_type(scope_path, node_id, t, wire)? {
            Some(data) => {
                self.install_record_input_data(scope_path, node_id, data, true)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// The connect hook: the data a reader adopts when `wire` is connected
    /// into its pin `dest_param_index`, or `None` when this connect is an
    /// ordinary one — the destination is not a reader, the pin is not its
    /// record pin, something already depends on the reader (judged before the
    /// wire exists, because the new wire is itself into the record pin), the
    /// source is not a record, adopting would change nothing, or the reader
    /// cannot hold the source's type (the wire is then made all the same, and
    /// stays stale until *Take type from wire*).
    pub(crate) fn connect_adoption(
        &self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        dest_param_index: usize,
        wire: &IncomingWire,
    ) -> Option<Box<dyn NodeData>> {
        let network = self.get_scope_network(dest_scope_path)?;
        let dest = network.nodes.get(&dest_node_id)?;
        if !dest
            .data
            .record_typed_input()?
            .is_record_input_pin(dest_param_index)
        {
            return None;
        }
        if !is_uncommitted_record_reader(network, self.scope_owner(dest_scope_path), dest_node_id) {
            return None;
        }
        let DataType::Record(source_type) = self.resolve_wire_source_type(
            dest_scope_path,
            wire.source_node_id,
            wire.source_pin,
            wire.source_scope_depth,
        )?
        else {
            return None;
        };
        self.plan_record_input_type(dest_scope_path, dest_node_id, &source_type, Some(wire))
            .ok()
            .flatten()
    }

    /// Install the data [`Self::connect_adoption`] planned. Inside a body the
    /// reflow stays in the body: the body's undo step reflows the ancestors.
    pub(crate) fn install_connect_adoption(
        &mut self,
        dest_scope_path: &[u64],
        dest_node_id: u64,
        data: Box<dyn NodeData>,
    ) {
        let cascade = dest_scope_path.is_empty();
        // The plan was made against this very network a moment ago.
        let _ = self.install_record_input_data(dest_scope_path, dest_node_id, data, cascade);
    }

    /// The HOF node whose body `scope_path` names, `None` at top level.
    pub(crate) fn scope_owner(&self, scope_path: &[u64]) -> Option<&crate::node_network::Node> {
        let (hof_id, parent) = scope_path.split_last()?;
        self.get_scope_network(parent)?.nodes.get(hof_id)
    }

    /// Point a reader at the def `schema`, or at none (`""`, which makes it
    /// unset) — one undo step, "Set record schema". The output wires of the
    /// fields the new type shares by name with the old one are kept, the rest
    /// are dropped; picking none drops them all. Picking the current schema
    /// changes nothing and pushes no step.
    pub fn set_record_input_schema(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        schema: &str,
    ) -> Result<(), String> {
        self.ensure_active_editable()?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let t = if schema.is_empty() {
            RecordType::anonymous(Vec::new())
        } else {
            RecordType::Named(schema.to_string())
        };
        let Some(data) = self.plan_record_input_type(scope_path, node_id, &t, None)? else {
            return Ok(());
        };
        let before = self
            .snapshot_network(&network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;
        self.install_record_input_data(scope_path, node_id, data, true)?;
        self.finish_declared_input_edit(
            scope_path,
            node_id,
            &network_name,
            before,
            SET_RECORD_INPUT_SCHEMA_DESCRIPTION,
        );
        Ok(())
    }

    /// The *Take type from wire* offer on a reader's record pin, by pin index
    /// (the counterpart of the declared-input offers in
    /// [`Self::wire_type_offers`]): `Some(source type)` on the record pin when
    /// exactly one wire is on it and the wire is stale against the input type
    /// ([`is_stale_record_wire`] — it does not convert, or both are records
    /// whose resolved fields differ, so a wider source is stale too). The
    /// offered type is always the source's own, named or anonymous. Whether
    /// the node can hold it is decided when the offer is taken, which reports
    /// the reason.
    pub(crate) fn record_input_type_offers(
        &self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<Vec<Option<DataType>>> {
        let network = self.get_scope_network(scope_path)?;
        let node = network.nodes.get(&node_id)?;
        let typed_input = node.data.record_typed_input()?;
        let registry = &self.node_type_registry;
        let node_type = registry.get_node_type_for_node(node)?;
        let (ancestors, ancestor_hof_ids) = self.get_scope_ancestors(scope_path)?;
        Some(
            node_type
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    if !typed_input.is_record_input_pin(index) {
                        return None;
                    }
                    let [wire] = node.arguments.get(index)?.incoming_wires.as_slice() else {
                        return None;
                    };
                    let source = registry
                        .resolve_wire_source_type_scoped(
                            wire,
                            network,
                            &ancestors,
                            &ancestor_hof_ids,
                        )?
                        .data_type;
                    if !matches!(source, DataType::Record(_)) {
                        return None;
                    }
                    is_stale_record_wire(&source, &parameter.data_type, registry).then_some(source)
                })
                .collect(),
        )
    }

    /// *Take type from wire* on a reader: adopt the offered type of its record
    /// pin, in one undo step ("Take type from wire"). The offer is recomputed
    /// here; refused, with nothing changed and no undo step, when a listed pin
    /// has no offer or the node cannot hold the source's type (whose reason is
    /// returned).
    pub(crate) fn take_record_input_type_from_wire(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        pin_indices: &[usize],
        offers: &[Option<DataType>],
        description: &str,
    ) -> Result<(), String> {
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or_else(|| "No active network".to_string())?;
        let mut adopted: Option<RecordType> = None;
        for &index in pin_indices {
            match offers.get(index).cloned().flatten() {
                Some(DataType::Record(t)) => adopted = Some(t),
                _ => return Err(format!("Input {} has no type to take from its wire", index)),
            }
        }
        let t = adopted.ok_or_else(|| "No inputs to retype".to_string())?;
        let wire = self
            .get_scope_network(scope_path)
            .and_then(|network| single_record_wire(network, node_id))
            .map(|(_, wire)| wire);
        let Some(data) = self.plan_record_input_type(scope_path, node_id, &t, wire.as_ref())?
        else {
            return Ok(());
        };
        let before = self
            .snapshot_network(&network_name)
            .ok_or_else(|| "Failed to snapshot network".to_string())?;
        self.install_record_input_data(scope_path, node_id, data, true)?;
        self.finish_declared_input_edit(scope_path, node_id, &network_name, before, description);
        Ok(())
    }
}
