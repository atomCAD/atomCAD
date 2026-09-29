//! Library linking, Phase 3 (`doc/design_library_linking.md` D7, D9, D10,
//! D13, §7): bringing a changed dependency in without losing the user's
//! wiring.
//!
//! # Reconciliation (D13, §7.1 step 5)
//!
//! Wires are stored positionally, so a host's arguments on an instance of a
//! linked network mean something only relative to the interface they were
//! made against. The importing file records that interface per name (its
//! `uses` table — `LibraryMount::stored_uses`), and [`reconcile_network`]
//! moves every argument from the recorded interface to the one mounted now:
//! by `param_id` / `FieldId` first, by name second
//! (`network_validator::parameter_mapping`). **The one procedure serves the
//! open and the refresh alike**: on open the recorded interface is read from
//! the file, on a refresh it is captured from memory just before the re-mount
//! (§7.1 step 1) and written into `stored_uses`, so both read it from the same
//! place. It covers instances (inputs, and output pins by position, §7.3),
//! `record_construct` / `product` inputs and `record_destructure` outputs (by
//! field id), and `apply` nodes fed by an instance's function pin (their
//! `arg…` pins derive from the instance's parameters). Every wire it cannot
//! carry over is listed in the [`RefreshReport`] — nothing is dropped silently.
//!
//! **The order is load-bearing:** reconciliation runs before any pass that
//! realigns arguments by count (`repair_network_arguments`, a by-name
//! `set_custom_node_type(.., true)`), which would leave it nothing to match.
//!
//! # Change detection (D7)
//!
//! Every watched file — each linked library, nested ones included, and each
//! data file a node reads at load time ([`NodeData::file_paths`]) — has a
//! `loaded` stamp (the content in memory) and a `last_seen` one (the disk as
//! last observed). [`detect_changes`] compares the disk with `last_seen`, not
//! with `loaded`, so that after an undone refresh (memory deliberately older
//! than the disk) nothing is refreshed again.
//!
//! [`NodeData::file_paths`]: crate::node_data::NodeData::file_paths

use crate::library_links::{
    self, DetachedMount, FileStamp, ImportSpec, LinkFs, MountStatus, UsedField, UsedParam,
    is_frozen, is_under, recorded_entry,
};
use crate::network_validator::{
    cross_file_parameter_mapping, live_parameters, remap_node_arguments,
};
use crate::node_network::{
    Argument, FunctionPinDisposition, Node, NodeNetwork, SourcePin, function_pin_dispositions_for,
    walk_all_nodes,
};
use crate::node_type::Parameter;
use crate::node_type_registry::{NodeTypeRegistry, collect_record_refs_in_node};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The report (§7.2)
// ---------------------------------------------------------------------------

/// Which input a reported wire fed, by identity where there is one.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WireSlot {
    /// A parameter or record field with a stable id.
    Id(u64),
    /// A pin without an id; its name.
    Name(String),
    /// A pin known only by position (a frozen node).
    Pos(usize),
    /// A zone-output pin of an HOF.
    ZoneOut(usize),
}

/// One host wire the operation removed, flagged or has a warning about.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportedWire {
    /// The local network the wire is in.
    pub network: String,
    /// The HOF chain down to the body the wire is in (empty = top level).
    pub scope_path: Vec<u64>,
    /// The node the wire goes into.
    pub node_id: u64,
    pub slot: WireSlot,
    /// The input's name, for display.
    pub pin_name: String,
    pub source_node_id: u64,
    pub source_pin: SourcePin,
    /// The id of the source's output pin, when it has one (a
    /// `record_destructure` field) — the pin's identity, as the index alone
    /// may have moved.
    pub source_pin_id: Option<u64>,
    pub source_scope_depth: u8,
    pub reason: String,
}

/// One host node the report is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportedNode {
    pub network: String,
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    /// The linked name the node refers to.
    pub name: String,
}

/// What an open, a refresh or a retarget did to the host (§7.2). Every wire
/// that is gone after the operation is in `dropped_wires`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefreshReport {
    /// Direct mounts re-mounted, as `alias (path)`.
    pub refreshed_mounts: Vec<String>,
    /// Host data files re-read.
    pub refreshed_data_files: Vec<String>,
    /// Host nodes whose wiring was moved to a changed interface.
    pub reconciled_nodes: Vec<ReportedNode>,
    /// Wires into a parameter / field that no longer exists, or out of an
    /// output pin that no longer exists.
    pub dropped_wires: Vec<ReportedWire>,
    /// Wires kept into a parameter / field whose type changed; validation
    /// marks the incompatible ones.
    pub flagged_wires: Vec<ReportedWire>,
    /// Wires kept from an output pin ≥ 1 of a network whose output list
    /// changed: output pins match by position, so they may now carry
    /// something else (§7.3).
    pub output_pin_warnings: Vec<ReportedWire>,
    /// Linked names host nodes refer to that no longer exist.
    pub removed_names_in_use: Vec<String>,
    /// Host nodes frozen by the operation (§8).
    pub frozen_nodes: Vec<ReportedNode>,
    /// Mounts whose status changed, with the new status.
    pub status_changes: Vec<(String, MountStatus)>,
    /// Changes detected but not applied because redo history exists (D9).
    pub held: Vec<String>,
    /// Direct mounts whose content differs from the one the file was saved
    /// against (open only; a hint, D13).
    pub changed_since_saved: Vec<String>,
    pub errors: Vec<String>,
}

impl RefreshReport {
    /// A clean report gets the transient snackbar: nothing was dropped,
    /// flagged, frozen or removed, and every status change is to `Loaded`.
    pub fn is_clean(&self) -> bool {
        self.dropped_wires.is_empty()
            && self.flagged_wires.is_empty()
            && self.output_pin_warnings.is_empty()
            && self.removed_names_in_use.is_empty()
            && self.frozen_nodes.is_empty()
            && self.errors.is_empty()
            && self
                .status_changes
                .iter()
                .all(|(_, s)| *s == MountStatus::Loaded)
    }

    /// True when there is nothing at all to tell the user.
    pub fn is_empty(&self) -> bool {
        self.refreshed_mounts.is_empty()
            && self.refreshed_data_files.is_empty()
            && self.reconciled_nodes.is_empty()
            && self.status_changes.is_empty()
            && self.held.is_empty()
            && self.changed_since_saved.is_empty()
            && self.is_clean()
    }

    /// Appends everything `other` reports.
    pub fn merge(&mut self, other: RefreshReport) {
        self.refreshed_mounts.extend(other.refreshed_mounts);
        self.refreshed_data_files.extend(other.refreshed_data_files);
        self.reconciled_nodes.extend(other.reconciled_nodes);
        self.dropped_wires.extend(other.dropped_wires);
        self.flagged_wires.extend(other.flagged_wires);
        self.output_pin_warnings.extend(other.output_pin_warnings);
        self.removed_names_in_use.extend(other.removed_names_in_use);
        self.frozen_nodes.extend(other.frozen_nodes);
        self.status_changes.extend(other.status_changes);
        self.held.extend(other.held);
        self.changed_since_saved.extend(other.changed_since_saved);
        self.errors.extend(other.errors);
    }
}

// ---------------------------------------------------------------------------
// Reconciliation (D13, §7.1 step 5)
// ---------------------------------------------------------------------------

/// How one node's pins move from the recorded interface to the live one.
struct Plan {
    /// An instance of a linked network (else a record node).
    instance: bool,
    old_inputs: Vec<Parameter>,
    new_inputs: Vec<Parameter>,
    /// Per new input, the old input it continues; `None` = inputs unchanged.
    input_map: Option<Vec<Option<usize>>>,
    /// New inputs whose type changed.
    retyped: Vec<usize>,
    /// Per old output pin, the new pin; `None` = outputs unchanged.
    out_map: Option<Vec<Option<usize>>>,
    /// Per old output pin, its id (record fields), for the report.
    old_out_ids: Vec<Option<u64>>,
    /// The output list changed and pins match by position (instances, §7.3).
    warn_outputs: bool,
    /// The linked name, for the report.
    name: String,
}

fn parse_type(s: &str) -> crate::data_type::DataType {
    crate::data_type::DataType::from_string(s).unwrap_or(crate::data_type::DataType::None)
}

fn params_of_used(params: &[UsedParam]) -> Vec<Parameter> {
    params
        .iter()
        .map(|p| Parameter {
            id: p.id,
            name: p.name.clone(),
            data_type: parse_type(&p.data_type),
        })
        .collect()
}

fn params_of_fields(fields: &[UsedField]) -> Vec<Parameter> {
    fields
        .iter()
        .map(|f| Parameter {
            id: Some(f.id),
            name: f.name.clone(),
            data_type: parse_type(&f.data_type),
        })
        .collect()
}

fn same_identity(a: &[Parameter], b: &[Parameter]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.id == y.id && x.name == y.name)
}

/// New inputs whose type differs from the old input they continue.
fn retyped(old: &[Parameter], new: &[Parameter], map: &[Option<usize>]) -> Vec<usize> {
    map.iter()
        .enumerate()
        .filter_map(|(n, o)| {
            let o = (*o)?;
            (old[o].data_type.to_string() != new[n].data_type.to_string()).then_some(n)
        })
        .collect()
}

/// Inverts a per-new mapping into a per-old one.
fn invert(map: &[Option<usize>], old_len: usize) -> Vec<Option<usize>> {
    let mut out = vec![None; old_len];
    for (n, o) in map.iter().enumerate() {
        if let Some(o) = o
            && *o < old_len
            && out[*o].is_none()
        {
            out[*o] = Some(n);
        }
    }
    out
}

fn identity(len: usize) -> Vec<Option<usize>> {
    (0..len).map(Some).collect()
}

/// The reconciliation plan of `node`, if it refers to a linked name that
/// resolves and whose interface differs from the one the owning file
/// recorded — or if it still carries the recorded layout of a name that
/// resolves again (an instance unfreezing: that layout would otherwise shadow
/// the network's own interface, §11 Phase 3).
fn plan_for(node: &Node, registry: &NodeTypeRegistry, owner: Option<&str>) -> Option<Plan> {
    if is_frozen(node, registry) {
        return None;
    }
    let links = &registry.library_links;
    let name = &node.node_type_name;
    if !registry.built_in_node_types.contains_key(name)
        && let Some(network) = registry.node_networks.get(name)
    {
        links.mount_containing(name)?;
        let new_inputs = live_parameters(network);
        let new_outputs: Vec<(String, String)> = network
            .node_type
            .output_pins
            .iter()
            .map(|p| (p.name.clone(), p.data_type.to_string()))
            .collect();
        let stale_layout = node.custom_node_type.is_some();
        let Some(entry) = recorded_entry(registry, name, owner, |u, k| u.networks.get(k)) else {
            return stale_layout.then(|| Plan {
                instance: true,
                old_inputs: new_inputs.clone(),
                input_map: None,
                retyped: Vec::new(),
                out_map: None,
                old_out_ids: Vec::new(),
                warn_outputs: false,
                new_inputs,
                name: name.clone(),
            });
        };
        let old_inputs = params_of_used(&entry.params);
        let old_outputs: Vec<(String, String)> = entry
            .outputs
            .iter()
            .map(|o| (o.name.clone(), o.data_type.clone()))
            .collect();
        let inputs_same = same_identity(&old_inputs, &new_inputs);
        let map = if inputs_same {
            identity(new_inputs.len())
        } else {
            let name_only = links
                .iter()
                .find_map(|m| m.name_only_param_ids.get(name))
                .cloned()
                .unwrap_or_default();
            cross_file_parameter_mapping(&old_inputs, &new_inputs, &name_only)
        };
        let retyped = retyped(&old_inputs, &new_inputs, &map);
        let outputs_same = old_outputs == new_outputs;
        if inputs_same && retyped.is_empty() && outputs_same && !stale_layout {
            return None;
        }
        let out_map = (!outputs_same).then(|| {
            (0..old_outputs.len())
                .map(|p| (p < new_outputs.len()).then_some(p))
                .collect()
        });
        return Some(Plan {
            instance: true,
            input_map: (!inputs_same).then_some(map),
            retyped,
            out_map,
            old_out_ids: Vec::new(),
            warn_outputs: !outputs_same,
            old_inputs,
            new_inputs,
            name: name.clone(),
        });
    }

    let schema = library_links::record_node_schema(node)?;
    links.mount_containing(&schema)?;
    let def = registry.lookup_record_type_def(&schema)?;
    let entry = recorded_entry(registry, &schema, owner, |u, k| u.records.get(k))?;
    let old = params_of_fields(&entry.fields);
    let new: Vec<Parameter> = def
        .fields
        .iter()
        .map(|f| Parameter {
            id: Some(f.id.0),
            name: f.name.clone(),
            data_type: f.data_type.clone(),
        })
        .collect();
    let same = same_identity(&old, &new);
    let map = if same {
        identity(new.len())
    } else {
        cross_file_parameter_mapping(&old, &new, &BTreeSet::new())
    };
    let field_retyped = retyped(&old, &new, &map);
    if same && field_retyped.is_empty() {
        return None;
    }
    if node.node_type_name == "record_destructure" {
        // One `record` input; the fields are the outputs.
        return Some(Plan {
            instance: false,
            old_inputs: Vec::new(),
            new_inputs: Vec::new(),
            input_map: None,
            retyped: Vec::new(),
            out_map: (!same).then(|| invert(&map, old.len())),
            old_out_ids: old.iter().map(|p| p.id).collect(),
            warn_outputs: false,
            name: schema,
        });
    }
    // `record_construct` / `product`: the fields are the inputs.
    Some(Plan {
        instance: false,
        input_map: (!same).then_some(map),
        retyped: field_retyped,
        out_map: None,
        old_out_ids: Vec::new(),
        warn_outputs: false,
        old_inputs: old,
        new_inputs: new,
        name: schema,
    })
}

fn slot_of_param(p: &Parameter) -> WireSlot {
    match p.id {
        Some(id) => WireSlot::Id(id),
        None => WireSlot::Name(p.name.clone()),
    }
}

/// The slot input `i` of `node` has **now** (after its own inputs moved).
fn current_slot(
    node: &Node,
    i: usize,
    registry: &NodeTypeRegistry,
    plan: Option<&Plan>,
) -> (WireSlot, String) {
    if let Some(p) = plan.and_then(|p| p.new_inputs.get(i)) {
        return (slot_of_param(p), p.name.clone());
    }
    if is_frozen(node, registry) {
        return (WireSlot::Pos(i), library_links::positional_pin_name(i));
    }
    match registry
        .get_node_type_for_node(node)
        .and_then(|t| t.parameters.get(i))
    {
        Some(p) => {
            let id = p.id.or_else(|| {
                registry
                    .node_networks
                    .get(&node.node_type_name)
                    .and_then(|n| live_parameters(n).into_iter().find(|q| q.name == p.name))
                    .and_then(|q| q.id)
            });
            match id {
                Some(id) => (WireSlot::Id(id), p.name.clone()),
                None => (WireSlot::Name(p.name.clone()), p.name.clone()),
            }
        }
        None => (WireSlot::Pos(i), library_links::positional_pin_name(i)),
    }
}

/// Where the report is being built: the network and the scope in it.
struct Site<'a> {
    network: &'a str,
    scope: &'a [u64],
}

impl Site<'_> {
    fn wire(
        &self,
        node_id: u64,
        slot: WireSlot,
        pin_name: &str,
        w: &crate::node_network::IncomingWire,
        reason: &str,
    ) -> ReportedWire {
        ReportedWire {
            network: self.network.to_string(),
            scope_path: self.scope.to_vec(),
            node_id,
            slot,
            pin_name: pin_name.to_string(),
            source_node_id: w.source_node_id,
            source_pin: w.source_pin,
            source_pin_id: None,
            source_scope_depth: w.source_scope_depth,
            reason: reason.to_string(),
        }
    }

    fn node(&self, node_id: u64, name: &str) -> ReportedNode {
        ReportedNode {
            network: self.network.to_string(),
            scope_path: self.scope.to_vec(),
            node_id,
            name: name.to_string(),
        }
    }
}

/// The input pins of `node` that are parameters of its function-pin view,
/// for an input-pin count of `count` (the one partition, `node_network`).
fn exposed(node: &Node, count: usize) -> Vec<usize> {
    function_pin_dispositions_for(node, count)
        .into_iter()
        .enumerate()
        .filter(|(_, d)| *d == FunctionPinDisposition::Parameter)
        .map(|(i, _)| i)
        .collect()
}

/// Reconciles `network` — every scope, bodies included — against the
/// interfaces the file owning it recorded (the `stored_uses` of that file's
/// imports; `owner` is the mount the network belongs to, `None` for the file
/// the registry belongs to). `name` is the network's name, for the report.
/// The network may be outside the registry (it is on load and on refresh).
pub fn reconcile_network(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    owner: Option<&str>,
    name: &str,
    report: &mut RefreshReport,
) {
    if registry.library_links.is_empty() {
        return;
    }
    let mut outs = Vec::new();
    reconcile_scope(
        network,
        registry,
        owner,
        name,
        &mut Vec::new(),
        &mut outs,
        report,
    );
}

/// Per planned node of one scope: its output-pin map (old pin → new pin),
/// whether kept pins ≥ 1 get a position warning, and the old pins' ids.
type OutLevel = HashMap<u64, (Vec<Option<usize>>, bool, Vec<Option<u64>>)>;

/// The id of the output pin a same-scope wire reads, as its source was laid
/// out before this reconciliation — what identifies the pin when its index
/// may move (a `record_destructure` field).
fn old_source_pin_id(
    net: &NodeNetwork,
    registry: &NodeTypeRegistry,
    plans: &BTreeMap<u64, Plan>,
    w: &crate::node_network::IncomingWire,
) -> Option<u64> {
    if w.source_scope_depth != 0 {
        return None;
    }
    let SourcePin::NodeOutput { pin_index } = w.source_pin else {
        return None;
    };
    let pin = usize::try_from(pin_index).ok()?;
    if let Some(plan) = plans.get(&w.source_node_id)
        && !plan.old_out_ids.is_empty()
    {
        return plan.old_out_ids.get(pin).copied().flatten();
    }
    // Only a destructure's outputs follow identity; a network instance's
    // match by position (§7.3), whatever ids they inherit.
    let source = net
        .nodes
        .get(&w.source_node_id)
        .filter(|s| s.node_type_name == "record_destructure")?;
    registry
        .get_node_type_for_node(source)?
        .output_pins
        .get(pin)?
        .id
}

fn reconcile_scope(
    net: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    owner: Option<&str>,
    name: &str,
    scope: &mut Vec<u64>,
    outs: &mut Vec<OutLevel>,
    report: &mut RefreshReport,
) {
    let mut ids: Vec<u64> = net.nodes.keys().copied().collect();
    ids.sort_unstable();
    let plans: BTreeMap<u64, Plan> = ids
        .iter()
        .filter_map(|id| plan_for(&net.nodes[id], registry, owner).map(|p| (*id, p)))
        .collect();
    let site = Site {
        network: name,
        scope: &scope.clone(),
    };

    // Inputs, and the `apply` nodes fed by a moved instance's function pin.
    for (&id, plan) in &plans {
        report.reconciled_nodes.push(site.node(id, &plan.name));
        let Some(map) = &plan.input_map else {
            continue;
        };
        let node = &net.nodes[&id];
        let used: BTreeSet<usize> = map.iter().flatten().copied().collect();
        for (o, arg) in node.arguments.iter().enumerate() {
            if used.contains(&o) {
                continue;
            }
            let (slot, pin) = match plan.old_inputs.get(o) {
                Some(p) => (slot_of_param(p), p.name.clone()),
                None => (WireSlot::Pos(o), library_links::positional_pin_name(o)),
            };
            for w in &arg.incoming_wires {
                let mut r = site.wire(id, slot.clone(), &pin, w, "parameter removed");
                r.source_pin_id = old_source_pin_id(net, registry, &plans, w);
                report.dropped_wires.push(r);
            }
        }
        let old_exposed = exposed(node, plan.old_inputs.len());
        let appliers = applies_fed_by(net, id);
        let node = net.nodes.get_mut(&id).expect("planned node");
        remap_node_arguments(node, map);
        if plan.instance && !appliers.is_empty() {
            let new_exposed = exposed(node, plan.new_inputs.len());
            let positions: Vec<Option<usize>> = new_exposed
                .iter()
                .map(|&n| {
                    map.get(n)
                        .copied()
                        .flatten()
                        .and_then(|o| old_exposed.iter().position(|&x| x == o))
                })
                .collect();
            if positions != identity(old_exposed.len()) {
                for a in appliers {
                    remap_apply(
                        net,
                        a,
                        &positions,
                        &old_exposed,
                        plan,
                        &plans,
                        registry,
                        &site,
                        report,
                    );
                }
            }
        }
    }

    // Retyped inputs keep their wires; validation judges them.
    for (&id, plan) in &plans {
        let node = &net.nodes[&id];
        for &n in &plan.retyped {
            if let (Some(arg), Some(p)) = (node.arguments.get(n), plan.new_inputs.get(n)) {
                for w in &arg.incoming_wires {
                    report.flagged_wires.push(site.wire(
                        id,
                        slot_of_param(p),
                        &p.name,
                        w,
                        "parameter type changed",
                    ));
                }
            }
        }
    }

    // Output pins: every wire in this scope, and in bodies below, that reads
    // a moved output. Before the layouts change, so that consumer slots are
    // still read from the layouts the wires were made against.
    let level: OutLevel = plans
        .iter()
        .filter_map(|(id, p)| {
            p.out_map
                .clone()
                .map(|m| (*id, (m, p.warn_outputs, p.old_out_ids.clone())))
        })
        .collect();
    outs.push(level);
    if outs.iter().any(|l| !l.is_empty()) {
        remap_output_wires(net, registry, &plans, outs, &site, report);
    }

    // Layouts: an instance takes its network's own interface (drop a recorded
    // layout left from a frozen state); a record node is rebuilt from the
    // live def — positionally, its arguments already moved.
    for (&id, plan) in &plans {
        let node = net.nodes.get_mut(&id).expect("planned node");
        if plan.instance {
            node.custom_node_type = None;
        } else {
            registry.populate_custom_node_type_cache(node, false);
        }
    }

    for id in ids {
        if net.nodes.get(&id).is_some_and(|n| n.zone.is_some()) {
            scope.push(id);
            if let Some(body) = net.nodes.get_mut(&id).and_then(|n| n.zone_mut()) {
                reconcile_scope(body, registry, owner, name, scope, outs, report);
            }
            scope.pop();
        }
    }
    outs.pop();
}

/// The `apply` nodes of `net` whose `f` is wired to node `id`'s function pin.
fn applies_fed_by(net: &NodeNetwork, id: u64) -> Vec<u64> {
    let mut out: Vec<u64> = net
        .nodes
        .values()
        .filter(|n| {
            n.node_type_name == "apply"
                && n.arguments.first().is_some_and(|a| {
                    a.incoming_wires.iter().any(|w| {
                        w.source_scope_depth == 0
                            && w.source_node_id == id
                            && w.source_pin == SourcePin::NodeOutput { pin_index: -1 }
                    })
                })
        })
        .map(|n| n.id)
        .collect();
    out.sort_unstable();
    out
}

/// Moves the `arg…` wires of `apply` node `a`, fed by a moved instance's
/// function pin: `apply`'s arguments are `[f, arg0, …]`, one per parameter of
/// the instance's function view, so they follow the instance's own mapping
/// (§7.3, "function values"). `positions[j]` is the old `arg` index new
/// `arg j` continues.
#[allow(clippy::too_many_arguments)]
fn remap_apply(
    net: &mut NodeNetwork,
    a: u64,
    positions: &[Option<usize>],
    old_exposed: &[usize],
    plan: &Plan,
    plans: &BTreeMap<u64, Plan>,
    registry: &NodeTypeRegistry,
    site: &Site,
    report: &mut RefreshReport,
) {
    let used: BTreeSet<usize> = positions.iter().flatten().copied().collect();
    let node = &net.nodes[&a];
    for (k, &old_pin) in old_exposed.iter().enumerate() {
        if used.contains(&k) {
            continue;
        }
        // The `arg` pin is identified by the instance parameter it stood for.
        let slot = plan
            .old_inputs
            .get(old_pin)
            .map(slot_of_param)
            .unwrap_or_else(|| WireSlot::Name(format!("arg{}", k)));
        if let Some(arg) = node.arguments.get(1 + k) {
            for w in &arg.incoming_wires {
                let mut r = site.wire(
                    a,
                    slot.clone(),
                    &format!("arg{}", k),
                    w,
                    "parameter removed",
                );
                r.source_pin_id = old_source_pin_id(net, registry, plans, w);
                report.dropped_wires.push(r);
            }
        }
    }
    let node = net.nodes.get_mut(&a).expect("listed by the caller");
    let mut args = vec![
        node.arguments
            .first()
            .cloned()
            .unwrap_or_else(Argument::new),
    ];
    args.extend(positions.iter().map(|k| {
        k.and_then(|k| node.arguments.get(1 + k).cloned())
            .unwrap_or_else(Argument::new)
    }));
    node.arguments = args;
    report.reconciled_nodes.push(site.node(a, "apply"));
}

/// Re-points (or drops, reporting it) every wire of `net`'s own nodes that
/// reads an output pin moved by a plan at the scope level its depth names.
fn remap_output_wires(
    net: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    plans: &BTreeMap<u64, Plan>,
    outs: &[OutLevel],
    site: &Site,
    report: &mut RefreshReport,
) {
    let mut ids: Vec<u64> = net.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let plan = plans.get(&id);
        let node = net.nodes.get(&id).expect("id from the map");
        // Slots are computed before the borrow below.
        let slots: Vec<(WireSlot, String)> = (0..node.arguments.len())
            .map(|i| current_slot(node, i, registry, plan))
            .collect();
        let node = net.nodes.get_mut(&id).expect("id from the map");
        let mut remap = |arg: &mut Argument, slot: &(WireSlot, String)| {
            arg.incoming_wires.retain_mut(|w| {
                let SourcePin::NodeOutput { pin_index } = w.source_pin else {
                    return true;
                };
                let depth = w.source_scope_depth as usize;
                if pin_index < 0 || depth >= outs.len() {
                    return true;
                }
                let Some((map, warn, old_ids)) =
                    outs[outs.len() - 1 - depth].get(&w.source_node_id)
                else {
                    return true;
                };
                let old_id = old_ids.get(pin_index as usize).copied().flatten();
                match map.get(pin_index as usize) {
                    Some(Some(new_pin)) => {
                        let mut reported = site.wire(id, slot.0.clone(), &slot.1, w, "");
                        reported.source_pin_id = old_id;
                        w.source_pin = SourcePin::NodeOutput {
                            pin_index: *new_pin as i32,
                        };
                        if *warn && *new_pin >= 1 {
                            report.output_pin_warnings.push(ReportedWire {
                                reason:
                                    "output pins of the network changed; pins match by position"
                                        .to_string(),
                                ..reported
                            });
                        }
                        true
                    }
                    Some(None) => {
                        let mut r = site.wire(id, slot.0.clone(), &slot.1, w, "output removed");
                        r.source_pin_id = old_id;
                        report.dropped_wires.push(r);
                        false
                    }
                    None => true,
                }
            });
        };
        for (i, arg) in node.arguments.iter_mut().enumerate() {
            remap(arg, &slots[i]);
        }
        for (i, arg) in node.zone_output_arguments.iter_mut().enumerate() {
            remap(arg, &(WireSlot::ZoneOut(i), format!("zone output {}", i)));
        }
    }
}

// ---------------------------------------------------------------------------
// The safety net: wires the repair passes remove after reconciliation
// ---------------------------------------------------------------------------

/// Identifies one wire by position: scope, destination node, input (argument
/// index, or `ZONE_OUT + i` for zone-output input `i`), source, source pin,
/// scope depth. The repair and validation passes never move a wire to another
/// input, so a wire missing from a later image was removed.
pub type WireKey = (Vec<u64>, u64, usize, u64, SourcePin, u8);

/// A [`wire_images`] result.
pub type WireImages = Vec<(WireKey, ReportedWire)>;

const ZONE_OUT: usize = usize::MAX / 2;

/// The identity slot input `i` of `node` in `net` has now: like
/// [`current_slot`], but an `apply`'s `arg…` inputs are identified by the
/// parameter of the node feeding `f` that they stand for.
fn slot_in(
    net: &NodeNetwork,
    node: &Node,
    i: usize,
    registry: &NodeTypeRegistry,
) -> (WireSlot, String) {
    if node.node_type_name == "apply" && i >= 1 {
        let source = node.arguments.first().and_then(|a| {
            a.incoming_wires.iter().find_map(|w| {
                (w.source_scope_depth == 0
                    && w.source_pin == SourcePin::NodeOutput { pin_index: -1 })
                .then(|| net.nodes.get(&w.source_node_id))
                .flatten()
            })
        });
        if let Some(source) = source
            && let Some(source_type) = registry.get_node_type_for_node(source)
            && let Some(p) = exposed(source, source_type.parameters.len())
                .get(i - 1)
                .and_then(|&k| source_type.parameters.get(k))
        {
            let id = p.id.or_else(|| {
                registry
                    .node_networks
                    .get(&source.node_type_name)
                    .and_then(|n| live_parameters(n).into_iter().find(|q| q.name == p.name))
                    .and_then(|q| q.id)
            });
            let slot = match id {
                Some(id) => WireSlot::Id(id),
                None => WireSlot::Name(p.name.clone()),
            };
            return (slot, format!("arg{}", i - 1));
        }
    }
    current_slot(node, i, registry, None)
}

/// Every wire of `net` (bodies included), keyed by position, with what the
/// report would say about it.
pub fn wire_images(
    net: &NodeNetwork,
    registry: &NodeTypeRegistry,
    name: &str,
) -> Vec<(WireKey, ReportedWire)> {
    fn walk(
        net: &NodeNetwork,
        registry: &NodeTypeRegistry,
        name: &str,
        scope: &mut Vec<u64>,
        out: &mut Vec<(WireKey, ReportedWire)>,
    ) {
        let site = Site {
            network: name,
            scope: &scope.clone(),
        };
        let no_plans = BTreeMap::new();
        for node in net.nodes.values() {
            for (i, arg) in node.arguments.iter().enumerate() {
                let (slot, pin) = slot_in(net, node, i, registry);
                for w in &arg.incoming_wires {
                    let mut r = site.wire(node.id, slot.clone(), &pin, w, "");
                    r.source_pin_id = old_source_pin_id(net, registry, &no_plans, w);
                    let key = (
                        scope.clone(),
                        node.id,
                        i,
                        w.source_node_id,
                        w.source_pin,
                        w.source_scope_depth,
                    );
                    out.push((key, r));
                }
            }
            for (i, arg) in node.zone_output_arguments.iter().enumerate() {
                for w in &arg.incoming_wires {
                    let r = site.wire(
                        node.id,
                        WireSlot::ZoneOut(i),
                        &format!("zone output {}", i),
                        w,
                        "",
                    );
                    let key = (
                        scope.clone(),
                        node.id,
                        ZONE_OUT + i,
                        w.source_node_id,
                        w.source_pin,
                        w.source_scope_depth,
                    );
                    out.push((key, r));
                }
            }
            if let Some(body) = node.zone.as_deref() {
                scope.push(node.id);
                walk(body, registry, name, scope, out);
                scope.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(net, registry, name, &mut Vec::new(), &mut out);
    out
}

/// Reports, as dropped, every wire of `before` (an image taken right after
/// reconciliation) that `net` no longer has — whichever repair or validation
/// pass removed it (a zone-body wire whose type no longer converts, an output
/// pin that is gone, …). With this, every wire gone after an open, a refresh
/// or a retarget is in the report by construction (§7.2).
pub fn report_wires_removed_by_repair(
    before: &[(WireKey, ReportedWire)],
    net: &NodeNetwork,
    registry: &NodeTypeRegistry,
    name: &str,
    report: &mut RefreshReport,
) {
    let now: std::collections::HashSet<WireKey> = wire_images(net, registry, name)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    for (key, wire) in before {
        if now.contains(key) {
            continue;
        }
        let already = report.dropped_wires.iter().any(|d| {
            d.network == wire.network
                && d.scope_path == wire.scope_path
                && d.node_id == wire.node_id
                && d.source_node_id == wire.source_node_id
                && d.source_pin == wire.source_pin
                && d.source_scope_depth == wire.source_scope_depth
        });
        if !already {
            report.dropped_wires.push(ReportedWire {
                reason: "disconnected: incompatible after the library changed".to_string(),
                ..wire.clone()
            });
        }
    }
}

/// True when `network` has a node referring to a name under any mount.
pub fn refers_to_any_mount(network: &NodeNetwork, registry: &NodeTypeRegistry) -> bool {
    let mounts: Vec<String> = registry
        .library_links
        .direct_mounts()
        .map(|m| m.mount_path.clone())
        .collect();
    let mut hit = false;
    walk_all_nodes(network, &mut |node| {
        hit |= mounts.iter().any(|m| node_refers_to_mount(node, m));
    });
    hit
}

// ---------------------------------------------------------------------------
// References to mounts
// ---------------------------------------------------------------------------

/// True when `node` refers to a name at or under `mount_path` by any
/// reference kind of §8: its type, a record schema, or a `DataType` in its
/// data.
pub fn node_refers_to_mount(node: &Node, mount_path: &str) -> bool {
    let mut hit = is_under(&node.node_type_name, mount_path);
    collect_record_refs_in_node(node, &mut |n, _| hit |= is_under(n, mount_path));
    hit
}

/// The local networks (not under any mount) with a node that refers to a
/// name under one of `mount_paths`, sorted.
pub fn local_networks_referring_to(
    registry: &NodeTypeRegistry,
    mount_paths: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = registry
        .node_networks
        .iter()
        .filter(|(name, _)| registry.library_links.mount_containing(name).is_none())
        .filter(|(_, network)| {
            let mut hit = false;
            walk_all_nodes(network, &mut |node| {
                hit |= mount_paths.iter().any(|m| node_refers_to_mount(node, m));
            });
            hit
        })
        .map(|(name, _)| name.clone())
        .collect();
    out.sort();
    out
}

/// Every frozen node of the local networks `names`, as `(network, scope,
/// node id) → the unresolved name`.
pub fn frozen_nodes_of(
    registry: &NodeTypeRegistry,
    names: &[String],
) -> BTreeMap<(String, Vec<u64>, u64), String> {
    fn walk(
        registry: &NodeTypeRegistry,
        name: &str,
        net: &NodeNetwork,
        scope: &mut Vec<u64>,
        out: &mut BTreeMap<(String, Vec<u64>, u64), String>,
    ) {
        for node in net.nodes.values() {
            if let Some(n) = library_links::unresolved_mount_ref(node, registry) {
                out.insert((name.to_string(), scope.clone(), node.id), n);
            }
            if let Some(body) = node.zone.as_deref() {
                scope.push(node.id);
                walk(registry, name, body, scope, out);
                scope.pop();
            }
        }
    }
    let mut out = BTreeMap::new();
    for name in names {
        if let Some(net) = registry.node_networks.get(name) {
            walk(registry, name, net, &mut Vec::new(), &mut out);
        }
    }
    out
}

/// The linked names under `mount_paths` that the local networks refer to as
/// instances or record schemas (the kinds that can go missing and freeze a
/// node), and whether each resolves now.
pub fn referenced_linked_names(
    registry: &NodeTypeRegistry,
    mount_paths: &[String],
) -> BTreeMap<String, bool> {
    let mut out = BTreeMap::new();
    for (name, network) in &registry.node_networks {
        if registry.library_links.mount_containing(name).is_some() {
            continue;
        }
        walk_all_nodes(network, &mut |node| {
            let mut names = vec![node.node_type_name.clone()];
            if let Some(s) = library_links::record_node_schema(node) {
                names.push(s);
            }
            for n in names {
                if !mount_paths.iter().any(|m| is_under(&n, m) && n != *m) {
                    continue;
                }
                let resolves = registry.node_networks.contains_key(&n)
                    || registry.lookup_record_type_def(&n).is_some();
                out.insert(n, resolves);
            }
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Mounting helpers
// ---------------------------------------------------------------------------

/// Mounts `spec` as a direct link of the registry's design file (no
/// validation). The design file is on the cycle stack.
pub fn mount_direct(registry: &mut NodeTypeRegistry, spec: &ImportSpec) -> MountStatus {
    let Some(host) = registry.design_file_name.clone() else {
        return MountStatus::Error("the design has no file".to_string());
    };
    let fs = registry.library_links.fs();
    let host_path = PathBuf::from(&host);
    let canonical = fs.canonicalize(&host_path).unwrap_or(host_path.clone());
    let mut stack = vec![canonical];
    library_links::mount_library(registry, spec, &host_path, &mut stack)
}

/// A copy of everything at or under `mount_path` (the non-destructive
/// counterpart of `library_links::unmount`).
pub fn snapshot_mount(registry: &NodeTypeRegistry, mount_path: &str) -> DetachedMount {
    let mut networks: Vec<(String, NodeNetwork)> = registry
        .node_networks
        .iter()
        .filter(|(k, _)| is_under(k, mount_path))
        .map(|(k, n)| (k.clone(), n.clone()))
        .collect();
    networks.sort_by(|a, b| a.0.cmp(&b.0));
    let mut record_type_defs: Vec<_> = registry
        .record_type_defs
        .values()
        .filter(|d| is_under(&d.name, mount_path))
        .cloned()
        .collect();
    record_type_defs.sort_by(|a, b| a.name.cmp(&b.name));
    DetachedMount {
        networks,
        record_type_defs,
        folders: registry
            .folders
            .iter()
            .filter(|f| is_under(f, mount_path))
            .cloned()
            .collect(),
        mounts: registry
            .library_links
            .iter()
            .filter(|m| is_under(&m.mount_path, mount_path))
            .cloned()
            .collect(),
    }
}

/// The direct mount a mount path belongs to (itself, for a direct mount).
pub fn direct_of(registry: &NodeTypeRegistry, mount_path: &str) -> String {
    let mut current = mount_path.to_string();
    while let Some(parent) = registry
        .library_links
        .get(&current)
        .and_then(|m| m.parent.clone())
    {
        current = parent;
    }
    current
}

// ---------------------------------------------------------------------------
// Watching files (D7)
// ---------------------------------------------------------------------------

/// A watched data file: the direct mount whose content reads it (`None` = the
/// host), and its absolute path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DataFileKey {
    pub owner: Option<String>,
    pub path: PathBuf,
}

/// The two stamps of a watched data file (D7), and whether a change of it is
/// being held (D9).
#[derive(Clone, Debug, PartialEq)]
pub struct DataFileWatch {
    pub loaded: Option<FileStamp>,
    pub last_seen: Option<FileStamp>,
    pub held: bool,
}

/// What the disk says about a watched file, compared with `last_seen`.
#[derive(Clone, Debug, PartialEq)]
pub enum Observation {
    /// `mtime` and `size` equal `last_seen`'s.
    Same,
    /// Different `mtime` or `size`, same content.
    Touched(FileStamp),
    Missing,
    Unreadable(String),
    /// Different content.
    Changed(FileStamp),
}

/// Stats `path` and, only when `mtime` or `size` moved, hashes it (D7).
pub fn observe(fs: &dyn LinkFs, path: &Path, last_seen: Option<&FileStamp>) -> Observation {
    let meta = match fs.stat(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Observation::Missing,
        Err(e) => return Observation::Unreadable(e.to_string()),
    };
    if let Some(seen) = last_seen
        && meta.mtime.is_some()
        && seen.mtime == meta.mtime
        && seen.size == meta.size
    {
        return Observation::Same;
    }
    let bytes = match fs.read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Observation::Missing,
        Err(e) => return Observation::Unreadable(e.to_string()),
    };
    let stamp = FileStamp::of_bytes(&bytes, meta.mtime);
    match last_seen {
        Some(seen) if seen.blake3 == stamp.blake3 => Observation::Touched(stamp),
        _ => Observation::Changed(stamp),
    }
}

/// The current stamp of `path` (read in full), or `None` when it cannot be
/// read. The mtime is taken before the read (see `mount_library`).
pub fn stamp_file(fs: &dyn LinkFs, path: &Path) -> Option<FileStamp> {
    let mtime = fs.stat(path).ok()?.mtime;
    let bytes = fs.read(path).ok()?;
    Some(FileStamp::of_bytes(&bytes, mtime))
}

/// The folder a network's relative data-file paths resolve against: its
/// library's for a linked network, the design's for a local one (D8).
pub fn base_dir_of(registry: &NodeTypeRegistry, network_name: &str) -> Option<PathBuf> {
    match registry.library_links.mount_containing(network_name) {
        Some(m) => m.abs_path.parent().map(Path::to_path_buf),
        None => registry
            .design_file_name
            .as_ref()
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf)),
    }
}

/// Resolves a data-file path as stored in node data.
pub fn resolve_data_path(base: Option<&Path>, stored: &str) -> Option<PathBuf> {
    let p = Path::new(stored);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    library_links::lexical_join(base?, stored).ok()
}

/// Every data file the registry's networks read at load time, keyed by owner.
pub fn watched_data_files(registry: &NodeTypeRegistry) -> BTreeSet<DataFileKey> {
    data_file_sites(registry)
        .into_iter()
        .map(|(_, key)| key)
        .collect()
}

/// Every place a data file is read at load: `(network, stored path)` with
/// the watch key it resolves to now. Two calls around a change of folders
/// pair the old key of a read with its new one ([`relocate_after_save`]).
pub fn data_file_sites(registry: &NodeTypeRegistry) -> Vec<((String, String), DataFileKey)> {
    let mut out = Vec::new();
    for (name, network) in &registry.node_networks {
        let owner = registry
            .library_links
            .mount_containing(name)
            .map(|m| direct_of(registry, &m.mount_path));
        let base = base_dir_of(registry, name);
        walk_all_nodes(network, &mut |node| {
            for stored in node.data.file_paths() {
                if let Some(path) = resolve_data_path(base.as_deref(), &stored) {
                    out.push((
                        (name.clone(), stored),
                        DataFileKey {
                            owner: owner.clone(),
                            path,
                        },
                    ));
                }
            }
        });
    }
    out
}

/// After the design was saved into another folder: every relative library
/// and data-file path now means a file there (D7, D11). Mounts are re-pointed
/// ([`library_links::relocate_mounts`]); each data-file watch moves to the path
/// its reads resolve to now, keeping its `loaded` stamp (the content in
/// memory) and forgetting the mtime of its last sighting, so the next check
/// hashes the new file: a copy is merely *touched*, a different file kept at
/// the new place is a change that refreshes, a missing one goes missing.
/// `sites_before` is [`data_file_sites`] taken before the folder changed.
pub fn relocate_after_save(
    registry: &mut NodeTypeRegistry,
    sites_before: &[((String, String), DataFileKey)],
) {
    library_links::relocate_mounts(registry);
    let before: BTreeMap<&(String, String), &DataFileKey> =
        sites_before.iter().map(|(site, key)| (site, key)).collect();
    let old = std::mem::take(&mut registry.library_links.data_files);
    let fs = registry.library_links.fs();
    let mut watches = BTreeMap::new();
    for (site, key) in data_file_sites(registry) {
        if watches.contains_key(&key) {
            continue;
        }
        let carried = before
            .get(&site)
            .and_then(|old_key| old.get(*old_key).map(|w| (*old_key, w)));
        let watch = match carried {
            Some((old_key, w)) => {
                let mut w = w.clone();
                if old_key.path != key.path
                    && let Some(seen) = w.last_seen.as_mut()
                {
                    seen.mtime = None;
                }
                w
            }
            None => {
                let stamp = stamp_file(fs.as_ref(), &key.path);
                DataFileWatch {
                    loaded: stamp.clone(),
                    last_seen: stamp,
                    held: false,
                }
            }
        };
        watches.insert(key, watch);
    }
    registry.library_links.data_files = watches;
}

/// Brings the data-file watch list in line with the networks: entries no
/// longer read are dropped, new ones are stamped now (their content is what a
/// load just read). Existing entries keep their stamps.
pub fn rebuild_data_watches(registry: &mut NodeTypeRegistry) {
    let wanted = watched_data_files(registry);
    let fs = registry.library_links.fs();
    let watches = &mut registry.library_links.data_files;
    watches.retain(|k, _| wanted.contains(k));
    for key in wanted {
        watches.entry(key.clone()).or_insert_with(|| {
            let stamp = stamp_file(fs.as_ref(), &key.path);
            DataFileWatch {
                loaded: stamp.clone(),
                last_seen: stamp,
                held: false,
            }
        });
    }
}

/// The outcome of one check of every watched file.
#[derive(Debug, Default)]
pub struct Detection {
    /// Direct mounts to refresh (a library, one of its nested libraries or
    /// one of their data files changed).
    pub mounts: BTreeSet<String>,
    /// Host data files to re-read.
    pub host_files: BTreeSet<PathBuf>,
    pub status_changes: Vec<(String, MountStatus)>,
}

/// Checks every watched file against its `last_seen` stamp (D7). Updates
/// what needs no refresh on the spot — a touched file's `last_seen`, a mount
/// that went missing or unreadable (the loaded content keeps evaluating,
/// D10), a file that came back unchanged — and returns what does.
pub fn detect_changes(registry: &mut NodeTypeRegistry) -> Detection {
    let fs = registry.library_links.fs();
    let mut out = Detection::default();
    let paths: Vec<String> = registry
        .library_links
        .iter()
        .map(|m| m.mount_path.clone())
        .collect();
    for mount_path in paths {
        let direct = direct_of(registry, &mount_path);
        let Some(m) = registry.library_links.get(&mount_path) else {
            continue;
        };
        if m.abs_path.as_os_str().is_empty() || m.status == MountStatus::Cycle {
            continue;
        }
        let observation = observe(fs.as_ref(), &m.abs_path, m.last_seen.as_ref());
        let m = registry
            .library_links
            .get_mut(&mount_path)
            .expect("listed above");
        match observation {
            Observation::Same => {}
            Observation::Touched(stamp) => m.last_seen = Some(stamp),
            Observation::Missing => {
                m.last_seen = None;
                if m.status != MountStatus::Missing {
                    m.status = MountStatus::Missing;
                    out.status_changes.push((mount_path, MountStatus::Missing));
                }
            }
            Observation::Unreadable(e) => {
                let status = MountStatus::Error(format!("cannot read '{}': {}", m.rel_path, e));
                if m.status != status {
                    m.status = status.clone();
                    out.status_changes.push((mount_path, status));
                }
            }
            Observation::Changed(stamp) => {
                let back_as_loaded = m.loaded.as_ref().is_some_and(|l| l.blake3 == stamp.blake3);
                if back_as_loaded {
                    // The file came back (or went back) to the content in
                    // memory: nothing to refresh.
                    m.last_seen = Some(stamp);
                    if m.status != MountStatus::Loaded {
                        m.status = MountStatus::Loaded;
                        out.status_changes.push((mount_path, MountStatus::Loaded));
                    }
                } else {
                    out.mounts.insert(direct);
                }
            }
        }
    }
    let keys: Vec<DataFileKey> = registry.library_links.data_files.keys().cloned().collect();
    for key in keys {
        let watch = &registry.library_links.data_files[&key];
        let observation = observe(fs.as_ref(), &key.path, watch.last_seen.as_ref());
        let watch = registry
            .library_links
            .data_files
            .get_mut(&key)
            .expect("listed above");
        match observation {
            Observation::Same | Observation::Unreadable(_) => {}
            Observation::Touched(stamp) => watch.last_seen = Some(stamp),
            Observation::Missing => watch.last_seen = None,
            Observation::Changed(stamp) => {
                if watch
                    .loaded
                    .as_ref()
                    .is_some_and(|l| l.blake3 == stamp.blake3)
                {
                    watch.last_seen = Some(stamp);
                    watch.held = false;
                } else {
                    match &key.owner {
                        Some(owner) => {
                            out.mounts.insert(owner.clone());
                        }
                        None => {
                            out.host_files.insert(key.path.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

/// True when anything under direct mount `alias` — its file, a nested
/// library, a data file of theirs — differs on disk from what is in memory
/// (an explicit *Refresh* re-hashes unconditionally, D7).
pub fn mount_differs_from_disk(registry: &NodeTypeRegistry, alias: &str) -> bool {
    let fs = registry.library_links.fs();
    let files = registry
        .library_links
        .iter()
        .filter(|m| is_under(&m.mount_path, alias))
        .filter(|m| !m.abs_path.as_os_str().is_empty() && m.status != MountStatus::Cycle)
        .map(|m| (m.abs_path.clone(), m.loaded.clone()))
        .chain(
            registry
                .library_links
                .data_files
                .iter()
                .filter(|(k, _)| k.owner.as_deref() == Some(alias))
                .map(|(k, w)| (k.path.clone(), w.loaded.clone())),
        );
    for (path, loaded) in files {
        let now = stamp_file(fs.as_ref(), &path);
        if now.as_ref().map(|s| &s.blake3) != loaded.as_ref().map(|s| &s.blake3) {
            return true;
        }
    }
    false
}

/// Re-reads the data of every node in `network` that reads one of `files`,
/// through its own saver and loader: the loader re-reads the file, every
/// user-set field comes back as it was. Returns how many nodes were reloaded.
pub fn reload_data_file_nodes(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    base: Option<&Path>,
    files: &BTreeSet<PathBuf>,
) -> usize {
    let base_str = base.map(|b| b.to_string_lossy().to_string());
    let mut reloaded = 0;
    crate::node_network::walk_all_nodes_mut(network, &mut |node| {
        let reads = node
            .data
            .file_paths()
            .iter()
            .filter_map(|p| resolve_data_path(base, p))
            .any(|p| files.contains(&p));
        if !reads {
            return;
        }
        let saver = registry.node_data_saver_for(&node.node_type_name);
        let loader = registry.node_data_loader_for(&node.node_type_name);
        let mut copy = node.data.clone_box();
        if let Ok(value) = saver(copy.as_mut(), None)
            && let Ok(data) = loader(&value, base_str.as_deref())
        {
            node.data = data;
            reloaded += 1;
        }
    });
    reloaded
}
