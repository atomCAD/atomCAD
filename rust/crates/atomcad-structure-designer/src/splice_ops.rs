//! The splice operations of `doc/design_insert_node_on_wire.md`, as
//! `impl StructureDesigner`: each is a read-only *plan* (which arrangements
//! are valid) and a *commit* that re-plans, checks the caller's choice against
//! the fresh plan and performs the whole edit as one undo step (D1, D8).
//!
//! The plans resolve the live types and hand everything else to the pure
//! planner (`splice_plan.rs`). Commits add their nodes through
//! [`StructureDesigner::add_node_core`] inside their own snapshot, never
//! through the recording add paths.
//!
//! Three operations live here:
//!
//! - the **wire drop** (Phase 0): a wire dragged off a pin and dropped in
//!   empty space adds a node there and connects it. Planning first means a
//!   cancelled dialog leaves the network untouched, and the commit is one
//!   step;
//! - **insertion** (Phase 1): a node spliced into an existing wire. The source
//!   is wired into the new node, and the old wire's slot is re-pointed at the
//!   new node through [`StructureDesigner::replace_wire_source`], the one
//!   primitive that edits an existing wire (D5, D11);
//! - **dissolve** (Phase 3): the inverse. A node is removed and every wire
//!   that read it is re-pointed, through the same primitive, at the one input
//!   wire's source that fits them all (D13, D14). A plan, not a dialog: when
//!   there is not exactly one answer, the refusal says why (D15).

use crate::data_type::DataType;
use crate::declared_input_ops::AddInputPlan;
use crate::declared_inputs::NameSource;
use crate::layout;
use crate::node_data::DragDirection;
use crate::node_network::{
    Argument, ArgumentKind, IncomingWire, Node, NodeNetwork, SourcePin, Wire, WireAnchor,
};
use crate::node_type_registry::{
    NodeTypeRegistry, accepts_drag_source, allowed_in_zone_body, offered_in_add_node,
};
use crate::nodes::comment::{CommentAnchor, CommentData};
use crate::splice_plan::{
    Arrangement, ArrangementInput, ArrangementOutput, InputChoice, InsertionScope, PlannedNode,
    SourceKind, SpliceSource, insertion_scope, plan_arrangements,
};
use crate::structure_designer::{DragSource, StructureDesigner};
use crate::text_format::snapshot_node_positions;
use crate::undo::commands::network_snapshot::NetworkSnapshotCommand;
use glam::f64::DVec2;
use std::collections::HashSet;

/// The undo-history label of a wire drop that adds a node.
pub const ADD_NODE_DESCRIPTION: &str = "Add node";

/// The undo-history label of an insertion (D8).
pub const INSERT_NODE_DESCRIPTION: &str = "Insert node";

/// Why a splice request was refused. A refusal changes nothing and pushes no
/// undo step. An empty plan is not a refusal: it is the answer "cannot be
/// spliced here".
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SpliceRefusal {
    #[error("this network is read-only (linked library)")]
    ReadOnly,
    #[error("the wire no longer exists")]
    WireNotFound,
    #[error("the dragged pin no longer exists")]
    PinNotFound,
    #[error("the scope no longer exists")]
    ScopeNotFound,
    #[error("unknown node type {0}")]
    UnknownNodeType(String),
    #[error("the node is not in the insertion scope")]
    NodeNotFound,
    #[error("only an unconnected node can be dropped onto a wire")]
    NodeHasWires,
    #[error("the node cannot reach this wire from its scope")]
    WrongScope,
    #[error("this scope cannot see the dragged pin")]
    ScopeNotVisible,
    #[error("the network changed; try again")]
    StaleArrangement,
    #[error("the inserted node's output no longer fits")]
    ResultMismatch,
}

/// The node an insertion splices into a wire.
///
/// Phase 1 of `doc/design_insert_node_on_wire.md` splices only new nodes; an
/// existing, wireless node (`Existing(id)`, the drag-onto-wire gesture) joins
/// in Phase 4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeSource {
    /// A new node of the named type.
    New(String),
}

/// Why [`StructureDesigner::replace_wire_source`] refused. On either, nothing
/// has changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceWireSourceError {
    /// The anchor does not resolve in the given scope.
    WireNotFound,
    /// The slot already holds a wire from the new source node, and
    /// `Argument::incoming_wires` holds at most one per source.
    SourceAlreadyInSlot,
}

/// Read-only twin of [`StructureDesigner::replace_wire_source`], for plans:
/// would replacing the wire from `replaced_source` with one from `new_source`
/// keep `argument`'s one-wire-per-source invariant? (It must already hold a
/// wire from `replaced_source`.)
pub fn slot_accepts_source(argument: &Argument, replaced_source: u64, new_source: u64) -> bool {
    argument.has_source(replaced_source)
        && (new_source == replaced_source || !argument.has_source(new_source))
}

/// The live half of an insertion, resolved once: the wire, where the new node
/// goes, and the types on both sides of it.
struct ResolvedInsert {
    network_name: String,
    /// The wire being spliced, as stored in its scope.
    wire: Wire,
    /// `S` and the two new wires' depths (D6).
    placement: InsertionScope,
    source_type: DataType,
    node_name: Option<String>,
    pin_name: Option<String>,
    prefer_pin: bool,
    /// The destination slot's declared type, which the new node's output must
    /// convert to.
    target: DataType,
}

impl ResolvedInsert {
    fn source(&self) -> SpliceSource<'_> {
        SpliceSource {
            data_type: self.source_type.clone(),
            name: NameSource {
                node_name: self.node_name.as_deref(),
                pin_name: self.pin_name.as_deref(),
                prefer_pin: self.prefer_pin,
            },
        }
    }

    fn drag(&self) -> DragSource {
        DragSource {
            source_type: self.source_type.clone(),
            direction: DragDirection::FromOutput,
        }
    }
}

/// The pin a wire was dragged from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraggedPin {
    /// A node output or an HOF's zone-input pin (`element`, `acc`).
    Output { node_id: u64, pin: SourcePin },
    /// An input slot, of either argument kind.
    Input {
        node_id: u64,
        kind: ArgumentKind,
        index: usize,
    },
}

/// What a wire drop knows about its dragged pin, resolved once in the live
/// network.
struct WireDrop {
    network_name: String,
    /// What a new node is adapted to: the dragged pin's type and direction.
    drag: DragSource,
    side: DropSide,
}

enum DropSide {
    /// Off an output: the source's type and what an open port names an input
    /// after, and the depth of the new wire into the new node.
    FromOutput {
        source_type: DataType,
        node_name: Option<String>,
        pin_name: Option<String>,
        prefer_pin: bool,
        depth: u8,
    },
    /// Off an input: the slot's declared type, which the new node's output
    /// must convert to, and the depth of the new wire out of the new node.
    FromInput { target: DataType, depth: u8 },
}

/// Whether `prefix` is a prefix of `scope` (`prefix ⊑ scope`).
fn is_prefix(prefix: &[u64], scope: &[u64]) -> bool {
    scope.starts_with(prefix)
}

impl StructureDesigner {
    /// The arrangements a new node of `node_type` offers when a wire dragged
    /// off `pin` (on a node in `pin_scope`) is dropped in `drop_scope`. Off an
    /// output each arrangement names the input the wire would enter; off an
    /// input, the output that would feed the slot — every non-function
    /// output, not only pin 0. Read-only.
    ///
    /// Refuses `ReadOnly`, then a missing scope or pin, then
    /// `ScopeNotVisible`: off an output, `drop_scope` must lie at or below
    /// the scope where the source value is visible; off an input, at or above
    /// the slot's effective scope. An unknown type is `UnknownNodeType`; a type
    /// that may not live in a body gets an empty plan in a body.
    pub fn plan_wire_drop(
        &self,
        pin_scope: &[u64],
        pin: &DraggedPin,
        drop_scope: &[u64],
        node_type: &str,
    ) -> Result<Vec<Arrangement>, SpliceRefusal> {
        let Some(drop) = self.resolve_wire_drop(pin_scope, pin, drop_scope)? else {
            return Ok(Vec::new());
        };
        self.plan_resolved_wire_drop(&drop, drop_scope, node_type)
    }

    /// Add a node of `node_type` at `position` in `drop_scope` and make the
    /// one wire between it and `pin` that `arrangement` describes, as one undo
    /// step ("Add node"). The arrangement must be one the plan offers now,
    /// else `StaleArrangement`. Returns the new node's id.
    ///
    /// The wire follows D6's depth convention, and an already-wired slot
    /// behaves exactly as in a plain connect: a single-wire slot is replaced,
    /// an array slot gains a wire.
    pub fn add_node_and_connect(
        &mut self,
        pin_scope: &[u64],
        pin: &DraggedPin,
        drop_scope: &[u64],
        node_type: &str,
        position: DVec2,
        arrangement: &Arrangement,
    ) -> Result<u64, SpliceRefusal> {
        // 1. Re-plan; the caller's arrangement must still be in it.
        let Some(drop) = self.resolve_wire_drop(pin_scope, pin, drop_scope)? else {
            return Err(SpliceRefusal::StaleArrangement);
        };
        let plan = self.plan_resolved_wire_drop(&drop, drop_scope, node_type)?;
        if !plan.contains(arrangement) {
            return Err(SpliceRefusal::StaleArrangement);
        }

        // 2. Snapshot, and the owning HOF's footprint chain for a body drop.
        let before = self
            .snapshot_network(&drop.network_name)
            .ok_or(SpliceRefusal::ScopeNotFound)?;
        let old_owner_sizes = self.capture_body_owner_footprint_chain(drop_scope);

        // 3. Add the node, adapted as the plan's was.
        let node_id = self.add_node_core(drop_scope, node_type, position, Some(&drop.drag));
        if node_id == 0 {
            return Err(SpliceRefusal::ScopeNotFound);
        }

        // 4. The wire.
        match (&drop.side, pin) {
            (
                DropSide::FromOutput {
                    source_type, depth, ..
                },
                DraggedPin::Output {
                    node_id: source_node_id,
                    pin: source_pin,
                },
            ) => {
                let input = arrangement
                    .input
                    .as_ref()
                    .ok_or(SpliceRefusal::StaleArrangement)?;
                match input.choice {
                    InputChoice::Pin(index) => {
                        let is_multi = self.input_pin_is_multi(drop_scope, node_id, index);
                        if let Some(network) = self.get_scope_network_mut(drop_scope) {
                            network.connect_wire(
                                *source_node_id,
                                *source_pin,
                                *depth,
                                node_id,
                                index,
                                is_multi,
                            );
                        }
                    }
                    InputChoice::OpenPort => {
                        let plan = AddInputPlan {
                            network_name: drop.network_name.clone(),
                            data_type: source_type.clone(),
                            name: input.name.clone(),
                            source_name: None,
                        };
                        self.append_declared_input_and_connect(
                            drop_scope,
                            node_id,
                            &plan,
                            *source_node_id,
                            *source_pin,
                            *depth,
                        )
                        .map_err(|_| SpliceRefusal::StaleArrangement)?;
                    }
                }
            }
            (
                DropSide::FromInput { depth, .. },
                DraggedPin::Input {
                    node_id: dest_node_id,
                    kind,
                    index,
                },
            ) => {
                let output = arrangement
                    .output
                    .as_ref()
                    .ok_or(SpliceRefusal::StaleArrangement)?;
                let source_pin = SourcePin::NodeOutput {
                    pin_index: output.pin,
                };
                self.connect_into_slot(
                    pin_scope,
                    *dest_node_id,
                    *kind,
                    *index,
                    node_id,
                    source_pin,
                    *depth,
                );
            }
            _ => return Err(SpliceRefusal::StaleArrangement),
        }

        // 5. Display, as a plain add and connect do it.
        if drop_scope.is_empty() {
            let mut dirty: HashSet<u64> = HashSet::from([node_id]);
            if pin_scope.is_empty() {
                dirty.insert(match pin {
                    DraggedPin::Output { node_id, .. } | DraggedPin::Input { node_id, .. } => {
                        *node_id
                    }
                });
            }
            self.pending_changes.mark_node_visibility_changed(node_id);
            self.apply_node_display_policy(Some(&dirty));
        }
        self.apply_default_all_pin_display(drop_scope, node_type, node_id);

        // 6. A body that grew grows its owner; the moves are inside the
        //    after-snapshot.
        if let Some((hof_id, parent)) = drop_scope.split_last() {
            let parent = parent.to_vec();
            self.reflow_for_footprint_change(&parent, *hof_id, &old_owner_sizes);
        }
        if let DraggedPin::Input {
            node_id: dest_node_id,
            ..
        } = pin
        {
            self.mark_node_data_changed_scoped(pin_scope, *dest_node_id);
        }

        // 7. Revalidate, mark dirty, push one step.
        if let Some(command) = self.finish_network_edit(
            drop_scope,
            node_id,
            &drop.network_name,
            before,
            ADD_NODE_DESCRIPTION,
        ) {
            self.push_command(command);
        }
        Ok(node_id)
    }

    /// The live half of a wire drop: every refusal, and the dragged pin's
    /// type and name, resolved once. `Ok(None)` when the pin's type cannot be
    /// resolved, which plans to nothing.
    fn resolve_wire_drop(
        &self,
        pin_scope: &[u64],
        pin: &DraggedPin,
        drop_scope: &[u64],
    ) -> Result<Option<WireDrop>, SpliceRefusal> {
        self.ensure_active_editable()
            .map_err(|_| SpliceRefusal::ReadOnly)?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or(SpliceRefusal::ScopeNotFound)?;
        if self.get_scope_network(drop_scope).is_none() {
            return Err(SpliceRefusal::ScopeNotFound);
        }
        let pin_network = self
            .get_scope_network(pin_scope)
            .ok_or(SpliceRefusal::ScopeNotFound)?;
        let registry = &self.node_type_registry;

        match *pin {
            DraggedPin::Output {
                node_id,
                pin: source_pin,
            } => {
                let node = pin_network
                    .nodes
                    .get(&node_id)
                    .ok_or(SpliceRefusal::PinNotFound)?;
                let node_type = registry
                    .get_node_type_for_node(node)
                    .ok_or(SpliceRefusal::PinNotFound)?;
                // Where the value is visible, and the wire's frame: a node
                // output from its own scope, a zone input only inside the
                // HOF's body, with its depth counted from the HOF's scope.
                let (visible, frame_depth) = match source_pin {
                    SourcePin::NodeOutput { pin_index } => {
                        if pin_index >= node_type.output_pin_count() as i32 {
                            return Err(SpliceRefusal::PinNotFound);
                        }
                        (pin_scope.to_vec(), 0u8)
                    }
                    SourcePin::ZoneInput { pin_index } => {
                        if pin_index >= node_type.zone_input_pins.len() {
                            return Err(SpliceRefusal::PinNotFound);
                        }
                        let mut body = pin_scope.to_vec();
                        body.push(node_id);
                        (body, 1u8)
                    }
                };
                if !is_prefix(&visible, drop_scope) {
                    return Err(SpliceRefusal::ScopeNotVisible);
                }
                let Some(source_type) =
                    self.resolve_wire_source_type(&visible, node_id, source_pin, frame_depth)
                else {
                    return Ok(None);
                };
                if source_type == DataType::None {
                    return Ok(None);
                }
                let (node_name, pin_name, prefer_pin) = self
                    .open_port_name_source(&visible, node_id, source_pin, frame_depth)
                    .ok_or(SpliceRefusal::PinNotFound)?;
                Ok(Some(WireDrop {
                    network_name,
                    drag: DragSource {
                        source_type: source_type.clone(),
                        direction: DragDirection::FromOutput,
                    },
                    side: DropSide::FromOutput {
                        source_type,
                        node_name,
                        pin_name,
                        prefer_pin,
                        depth: (drop_scope.len() - pin_scope.len()) as u8,
                    },
                }))
            }
            DraggedPin::Input {
                node_id,
                kind,
                index,
            } => {
                let node = pin_network
                    .nodes
                    .get(&node_id)
                    .ok_or(SpliceRefusal::PinNotFound)?;
                let node_type = registry
                    .get_node_type_for_node(node)
                    .ok_or(SpliceRefusal::PinNotFound)?;
                // The slot's effective scope, its declared type, and the type
                // a new node is adapted to: an input pin's concrete drag hint
                // where its declared type is lossy (`map.f`).
                let (effective, target, drag_type) = match kind {
                    ArgumentKind::External => {
                        let parameter = node_type
                            .parameters
                            .get(index)
                            .ok_or(SpliceRefusal::PinNotFound)?;
                        let hint = node.data.drag_hint_for_input_pin(index);
                        (
                            pin_scope.to_vec(),
                            parameter.data_type.clone(),
                            hint.unwrap_or_else(|| parameter.data_type.clone()),
                        )
                    }
                    ArgumentKind::ZoneOutput => {
                        let zone_pin = node_type
                            .zone_output_pins
                            .get(index)
                            .ok_or(SpliceRefusal::PinNotFound)?;
                        let mut body = pin_scope.to_vec();
                        body.push(node_id);
                        (body, zone_pin.data_type.clone(), zone_pin.data_type.clone())
                    }
                };
                if !is_prefix(drop_scope, &effective) {
                    return Err(SpliceRefusal::ScopeNotVisible);
                }
                Ok(Some(WireDrop {
                    network_name,
                    drag: DragSource {
                        source_type: drag_type,
                        direction: DragDirection::FromInput,
                    },
                    side: DropSide::FromInput {
                        target,
                        depth: (effective.len() - drop_scope.len()) as u8,
                    },
                }))
            }
        }
    }

    /// The planner half of a wire drop, over an already resolved pin.
    fn plan_resolved_wire_drop(
        &self,
        drop: &WireDrop,
        drop_scope: &[u64],
        node_type: &str,
    ) -> Result<Vec<Arrangement>, SpliceRefusal> {
        let registry = &self.node_type_registry;
        let planned = PlannedNode::new_of_type(registry, node_type, Some(&drop.drag))
            .ok_or_else(|| SpliceRefusal::UnknownNodeType(node_type.to_string()))?;
        if !drop_scope.is_empty() && !allowed_in_zone_body(node_type) {
            return Ok(Vec::new());
        }
        Ok(match &drop.side {
            DropSide::FromOutput {
                source_type,
                node_name,
                pin_name,
                prefer_pin,
                ..
            } => {
                let source = SpliceSource {
                    data_type: source_type.clone(),
                    name: NameSource {
                        node_name: node_name.as_deref(),
                        pin_name: pin_name.as_deref(),
                        prefer_pin: *prefer_pin,
                    },
                };
                plan_arrangements(registry, &planned, Some(&source), None, false)
            }
            DropSide::FromInput { target, .. } => {
                plan_arrangements(registry, &planned, None, Some(target), false)
            }
        })
    }

    /// Whether input `index` of `node_id` in `scope` is an array pin (one
    /// that collects several wires).
    fn input_pin_is_multi(&self, scope: &[u64], node_id: u64, index: usize) -> bool {
        self.get_scope_network(scope)
            .and_then(|network| network.nodes.get(&node_id))
            .and_then(|node| self.node_type_registry.get_node_type_for_node(node))
            .and_then(|node_type| node_type.parameters.get(index))
            .is_some_and(|parameter| parameter.data_type.is_array())
    }

    /// Connect a source into the slot `(dest_node_id, kind, index)` stored in
    /// `dest_scope`: a node's `arguments` for `External`, an HOF's
    /// `zone_output_arguments` for `ZoneOutput`. A single-wire slot is
    /// replaced, an array slot gains the wire.
    #[allow(clippy::too_many_arguments)]
    fn connect_into_slot(
        &mut self,
        dest_scope: &[u64],
        dest_node_id: u64,
        kind: ArgumentKind,
        index: usize,
        source_node_id: u64,
        source_pin: SourcePin,
        depth: u8,
    ) {
        match kind {
            ArgumentKind::External => {
                let is_multi = self.input_pin_is_multi(dest_scope, dest_node_id, index);
                if let Some(network) = self.get_scope_network_mut(dest_scope) {
                    network.connect_wire(
                        source_node_id,
                        source_pin,
                        depth,
                        dest_node_id,
                        index,
                        is_multi,
                    );
                }
            }
            ArgumentKind::ZoneOutput => {
                let is_multi = self
                    .get_scope_network(dest_scope)
                    .and_then(|network| network.nodes.get(&dest_node_id))
                    .and_then(|node| self.node_type_registry.get_node_type_for_node(node))
                    .and_then(|node_type| node_type.zone_output_pins.get(index))
                    .is_some_and(|pin| pin.data_type.is_array());
                let Some(hof) = self
                    .get_scope_network_mut(dest_scope)
                    .and_then(|network| network.nodes.get_mut(&dest_node_id))
                else {
                    return;
                };
                while hof.zone_output_arguments.len() <= index {
                    hof.zone_output_arguments.push(Argument::new());
                }
                let argument = &mut hof.zone_output_arguments[index];
                if !is_multi && !argument.is_empty() {
                    argument.clear();
                }
                argument.set_source_full(source_node_id, source_pin, depth);
            }
        }
    }
}

impl StructureDesigner {
    /// The arrangements `node` offers spliced into `wire` (stored in
    /// `wire_scope`), when the user clicked in `clicked_scope`. Each names the
    /// input the wire's source enters and the output that takes over the
    /// wire's slot. Read-only.
    ///
    /// Refuses `ReadOnly`, then `ScopeNotFound` / `WireNotFound`, then an
    /// unknown type. An empty list is the answer "cannot be spliced here": a
    /// source whose type cannot be resolved, a type not allowed in a body when
    /// the insertion scope is one, or no `(input, output)` pair that fits.
    pub fn plan_insert_on_wire(
        &self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        clicked_scope: &[u64],
        node: &NodeSource,
    ) -> Result<Vec<Arrangement>, SpliceRefusal> {
        let Some(insert) = self.resolve_insert(wire_scope, wire, clicked_scope)? else {
            return Ok(Vec::new());
        };
        self.plan_resolved_insert(&insert, node)
    }

    /// The names of the node types an insertion into `wire` offers (D10): the
    /// types the Add Node popup offers for an output drag of the wire's
    /// source type ([`accepts_drag_source`]) whose plan is non-empty. Sorted
    /// by name. Read-only; refuses as [`Self::plan_insert_on_wire`] does.
    pub fn insertable_node_type_names(
        &self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        clicked_scope: &[u64],
    ) -> Result<Vec<String>, SpliceRefusal> {
        let Some(insert) = self.resolve_insert(wire_scope, wire, clicked_scope)? else {
            return Ok(Vec::new());
        };
        let registry = &self.node_type_registry;
        let built_in = registry
            .built_in_node_types
            .values()
            .filter(|node_type| node_type.public);
        let custom = registry
            .node_networks
            .values()
            .map(|network| &network.node_type)
            .filter(|node_type| offered_in_add_node(registry, &node_type.name));
        let mut names: Vec<String> = built_in
            .chain(custom)
            .filter(|node_type| {
                accepts_drag_source(
                    node_type,
                    &insert.source_type,
                    DragDirection::FromOutput,
                    registry,
                )
            })
            .filter(|node_type| {
                self.plan_resolved_insert(&insert, &NodeSource::New(node_type.name.clone()))
                    .is_ok_and(|plan| !plan.is_empty())
            })
            .map(|node_type| node_type.name.clone())
            .collect();
        names.sort();
        Ok(names)
    }

    /// Splice `node` into `wire` with `arrangement`, as one undo step
    /// ("Insert node", D8): the wire's source is connected to the chosen
    /// input, the chosen output takes over the wire's slot (D5) and its comment
    /// anchors (D11), and the new node is placed by the incremental layout
    /// (D7). The arrangement must be one the plan offers now, else
    /// `StaleArrangement`. The new node becomes the selected, active node.
    /// Returns its id.
    ///
    /// Every refusal changes nothing and pushes no undo step.
    pub fn insert_node_on_wire(
        &mut self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        clicked_scope: &[u64],
        node: &NodeSource,
        arrangement: &Arrangement,
    ) -> Result<u64, SpliceRefusal> {
        // 1. Re-plan; the caller's arrangement must still be in it.
        let Some(insert) = self.resolve_insert(wire_scope, wire, clicked_scope)? else {
            return Err(SpliceRefusal::StaleArrangement);
        };
        let plan = self.plan_resolved_insert(&insert, node)?;
        if !plan.contains(arrangement) {
            return Err(SpliceRefusal::StaleArrangement);
        }
        let (Some(input), Some(output)) = (&arrangement.input, &arrangement.output) else {
            return Err(SpliceRefusal::StaleArrangement);
        };
        self.splice(wire_scope, wire, &insert, node, input, output)
    }

    /// Steps 2–8 of [`Self::insert_node_on_wire`], over a checked
    /// arrangement: the one splice every node source goes through.
    fn splice(
        &mut self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        insert: &ResolvedInsert,
        node: &NodeSource,
        input: &ArrangementInput,
        output: &ArrangementOutput,
    ) -> Result<u64, SpliceRefusal> {
        let scope = insert.placement.scope.clone();

        // 2. Snapshot for undo, a copy to roll back to, and the layout's
        //    pre-state.
        let before = self
            .snapshot_network(&insert.network_name)
            .ok_or(SpliceRefusal::ScopeNotFound)?;
        let (rollback, before_positions, before_wires) = {
            let network = self
                .node_type_registry
                .node_networks
                .get(&insert.network_name)
                .ok_or(SpliceRefusal::ScopeNotFound)?;
            (
                network.clone(),
                snapshot_node_positions(network, &self.node_type_registry),
                layout::collect_all_wires(network),
            )
        };

        // 3. The node, adapted to the source as the plan's was. Its position
        //    is a placeholder; the layout places it (step 6).
        let NodeSource::New(node_type) = node;
        let node_id = self.add_node_core(&scope, node_type, DVec2::ZERO, Some(&insert.drag()));
        if node_id == 0 {
            return Err(SpliceRefusal::ScopeNotFound);
        }

        // 4. The source into the chosen input, then 5. the downstream half
        //    takes the old wire's slot and its anchors. Neither can fail
        //    after a fresh plan; if one does, the network is put back.
        let spliced = self.connect_splice_input(&scope, node_id, insert, input)
            && self
                .replace_wire_source(
                    wire_scope,
                    wire,
                    node_id,
                    SourcePin::NodeOutput {
                        pin_index: output.pin,
                    },
                    insert.placement.depth_out,
                )
                .is_ok();
        if !spliced {
            self.node_type_registry
                .node_networks
                .insert(insert.network_name.clone(), rollback);
            return Err(SpliceRefusal::StaleArrangement);
        }

        // 6. Place the node: the incremental layout puts an added block with
        //    one upstream and one downstream anchor between them, and makes
        //    room to the right when there is none (D7).
        let algorithm = self.preferences.layout_preferences.layout_algorithm.into();
        let registry_ptr = &self.node_type_registry as *const NodeTypeRegistry;
        // SAFETY: as in `ai_text_edit`: the layout reads node types through
        // the shared reference while it moves the nodes of one network.
        unsafe {
            if let Some(network) = self
                .node_type_registry
                .node_networks
                .get_mut(&insert.network_name)
            {
                layout::layout_incremental(
                    network,
                    &*registry_ptr,
                    &before_positions,
                    &before_wires,
                    algorithm,
                );
            }
        }

        // 7. The live network must agree with the scratch one the plan used.
        if !self.output_fits(&scope, node_id, output.pin, &insert.target) {
            self.node_type_registry
                .node_networks
                .insert(insert.network_name.clone(), rollback);
            return Err(SpliceRefusal::ResultMismatch);
        }

        // 8. Display, selection, revalidate, one step.
        if scope.is_empty() {
            let mut dirty: HashSet<u64> = HashSet::from([node_id]);
            if wire_scope.is_empty() {
                dirty.insert(insert.wire.destination_node_id);
            }
            self.pending_changes.mark_node_visibility_changed(node_id);
            self.apply_node_display_policy(Some(&dirty));
        }
        self.apply_default_all_pin_display(&scope, node_type, node_id);
        self.mark_node_data_changed_scoped(wire_scope, insert.wire.destination_node_id);
        self.select_node_scoped(&scope, node_id);
        if let Some(command) = self.finish_network_edit(
            &scope,
            node_id,
            &insert.network_name,
            before,
            INSERT_NODE_DESCRIPTION,
        ) {
            self.push_command(command);
        }
        Ok(node_id)
    }

    /// Step 4 of the splice: wire the source into `input` of the new node in
    /// `scope`, with D6's `depth_in`. Through the open port the input is
    /// declared first and made the node's identity (D9), as the plan did.
    fn connect_splice_input(
        &mut self,
        scope: &[u64],
        node_id: u64,
        insert: &ResolvedInsert,
        input: &ArrangementInput,
    ) -> bool {
        let source_node_id = insert.wire.source_node_id;
        let source_pin = insert.wire.source_pin;
        let depth_in = insert.placement.depth_in;
        match input.choice {
            InputChoice::Pin(index) => {
                let is_multi = self.input_pin_is_multi(scope, node_id, index);
                let Some(network) = self.get_scope_network_mut(scope) else {
                    return false;
                };
                network.connect_wire(
                    source_node_id,
                    source_pin,
                    depth_in,
                    node_id,
                    index,
                    is_multi,
                );
                true
            }
            InputChoice::OpenPort => {
                let plan = AddInputPlan {
                    network_name: insert.network_name.clone(),
                    data_type: insert.source_type.clone(),
                    name: input.name.clone(),
                    source_name: None,
                };
                let Ok(added) = self.append_declared_input_and_connect(
                    scope,
                    node_id,
                    &plan,
                    source_node_id,
                    source_pin,
                    depth_in,
                ) else {
                    return false;
                };
                if let Some(declared) = self
                    .get_scope_network_mut(scope)
                    .and_then(|network| network.nodes.get_mut(&node_id))
                    .and_then(|node| node.data.declared_inputs_mut())
                {
                    declared.make_pass_through(added.pin_id);
                }
                self.rederive_node_type(scope, node_id);
                true
            }
        }
    }

    /// Replace the source of `wire` (stored in `wire_scope`) in its slot,
    /// keeping its position among the slot's `incoming_wires` (D5), and
    /// retarget every comment in `wire_scope` anchored to it (D11). Returns
    /// the wire's new address. On `Err` nothing has changed.
    ///
    /// The one place the splice operations edit an existing wire; neither
    /// touches `incoming_wires` or comment anchors directly. Records nothing:
    /// the caller's snapshot covers it.
    pub fn replace_wire_source(
        &mut self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        source_node_id: u64,
        source_pin: SourcePin,
        source_scope_depth: u8,
    ) -> Result<WireAnchor, ReplaceWireSourceError> {
        let network = self
            .get_scope_network_mut(wire_scope)
            .ok_or(ReplaceWireSourceError::WireNotFound)?;
        let resolved = wire
            .resolve(network)
            .ok_or(ReplaceWireSourceError::WireNotFound)?;

        // The comments anchored to this wire, found before it changes.
        let designates = |anchor: &CommentAnchor| match anchor {
            CommentAnchor::Wire(anchor) => anchor.resolve(network).is_some_and(|other| {
                other.destination_node_id == resolved.destination_node_id
                    && other.destination_argument_kind == resolved.destination_argument_kind
                    && other.destination_argument_index == resolved.destination_argument_index
                    && other.source_node_id == resolved.source_node_id
            }),
            CommentAnchor::Node(_) => false,
        };
        let mut anchored: Vec<(u64, usize)> = Vec::new();
        for (id, node) in &network.nodes {
            if let Some(comment) = node.data.as_any_ref().downcast_ref::<CommentData>() {
                for (index, anchor) in comment.anchors.iter().enumerate() {
                    if designates(anchor) {
                        anchored.push((*id, index));
                    }
                }
            }
        }

        let destination = network
            .nodes
            .get_mut(&resolved.destination_node_id)
            .ok_or(ReplaceWireSourceError::WireNotFound)?;
        let arguments = match resolved.destination_argument_kind {
            ArgumentKind::External => &mut destination.arguments,
            ArgumentKind::ZoneOutput => &mut destination.zone_output_arguments,
        };
        let argument = arguments
            .get_mut(resolved.destination_argument_index)
            .ok_or(ReplaceWireSourceError::WireNotFound)?;
        if !slot_accepts_source(argument, resolved.source_node_id, source_node_id) {
            return Err(ReplaceWireSourceError::SourceAlreadyInSlot);
        }
        let position = argument
            .incoming_wires
            .iter()
            .position(|w| w.source_node_id == resolved.source_node_id)
            .ok_or(ReplaceWireSourceError::WireNotFound)?;
        argument.incoming_wires[position] = IncomingWire {
            source_node_id,
            source_pin,
            source_scope_depth,
        };

        for (comment_id, index) in anchored {
            if let Some(CommentAnchor::Wire(anchor)) = network
                .nodes
                .get_mut(&comment_id)
                .and_then(|node| node.data.as_any_mut().downcast_mut::<CommentData>())
                .and_then(|comment| comment.anchors.get_mut(index))
            {
                anchor.source_node_id = source_node_id;
            }
        }

        let new_wire = Wire {
            source_node_id,
            source_pin,
            source_scope_depth,
            ..resolved
        };
        Ok(WireAnchor::from_wire(&new_wire, network))
    }

    /// The live half of an insertion: every refusal, `S`, and the types on
    /// both sides of the wire. `Ok(None)` when the source's type cannot be
    /// resolved, which plans to nothing.
    fn resolve_insert(
        &self,
        wire_scope: &[u64],
        wire: &WireAnchor,
        clicked_scope: &[u64],
    ) -> Result<Option<ResolvedInsert>, SpliceRefusal> {
        // 1. Read-only, then the wire.
        self.ensure_active_editable()
            .map_err(|_| SpliceRefusal::ReadOnly)?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or(SpliceRefusal::ScopeNotFound)?;
        let network = self
            .get_scope_network(wire_scope)
            .ok_or(SpliceRefusal::ScopeNotFound)?;
        let resolved = wire.resolve(network).ok_or(SpliceRefusal::WireNotFound)?;
        let registry = &self.node_type_registry;
        let destination_type = network
            .nodes
            .get(&resolved.destination_node_id)
            .and_then(|destination| registry.get_node_type_for_node(destination))
            .ok_or(SpliceRefusal::WireNotFound)?;

        // 2. D6. A wire's depth counts up from the destination's effective
        //    scope `dst`: the storage scope, or for a body-return wire the
        //    HOF's body, where its source lives.
        let index = resolved.destination_argument_index;
        let (dst, target) = match resolved.destination_argument_kind {
            ArgumentKind::External => (
                wire_scope.to_vec(),
                destination_type.parameters.get(index).map(|p| &p.data_type),
            ),
            ArgumentKind::ZoneOutput => {
                let mut body = wire_scope.to_vec();
                body.push(resolved.destination_node_id);
                (
                    body,
                    destination_type
                        .zone_output_pins
                        .get(index)
                        .map(|p| &p.data_type),
                )
            }
        };
        let target = target.ok_or(SpliceRefusal::WireNotFound)?.clone();
        let depth = resolved.source_scope_depth as usize;
        if depth > dst.len() {
            return Err(SpliceRefusal::WireNotFound);
        }
        let frame = &dst[..dst.len() - depth];
        let kind = match resolved.source_pin {
            SourcePin::NodeOutput { .. } => SourceKind::NodeOutput,
            SourcePin::ZoneInput { .. } => SourceKind::ZoneInput,
        };
        let placement =
            insertion_scope(kind, frame, &dst, clicked_scope).ok_or(SpliceRefusal::WireNotFound)?;

        // 3. The source's type and name, seen from `dst`.
        let Some(source_type) = self.resolve_wire_source_type(
            &dst,
            resolved.source_node_id,
            resolved.source_pin,
            resolved.source_scope_depth,
        ) else {
            return Ok(None);
        };
        if source_type == DataType::None {
            return Ok(None);
        }
        let (node_name, pin_name, prefer_pin) = self
            .open_port_name_source(
                &dst,
                resolved.source_node_id,
                resolved.source_pin,
                resolved.source_scope_depth,
            )
            .ok_or(SpliceRefusal::WireNotFound)?;
        Ok(Some(ResolvedInsert {
            network_name,
            wire: resolved,
            placement,
            source_type,
            node_name,
            pin_name,
            prefer_pin,
            target,
        }))
    }

    /// Steps 4–5 of the insert plan, over an already resolved wire.
    fn plan_resolved_insert(
        &self,
        insert: &ResolvedInsert,
        node: &NodeSource,
    ) -> Result<Vec<Arrangement>, SpliceRefusal> {
        let registry = &self.node_type_registry;
        let NodeSource::New(node_type) = node;
        let planned = PlannedNode::new_of_type(registry, node_type, Some(&insert.drag()))
            .ok_or_else(|| SpliceRefusal::UnknownNodeType(node_type.clone()))?;
        if !insert.placement.scope.is_empty() && !allowed_in_zone_body(node_type) {
            return Ok(Vec::new());
        }
        let source = insert.source();
        Ok(plan_arrangements(
            registry,
            &planned,
            Some(&source),
            Some(&insert.target),
            true,
        ))
    }

    /// Whether output `pin` of `node_id` in `scope`, resolved in the live
    /// network with its scope chain, converts to `target`.
    fn output_fits(&self, scope: &[u64], node_id: u64, pin: i32, target: &DataType) -> bool {
        let Some(network) = self.get_scope_network(scope) else {
            return false;
        };
        let Some((ancestors, ancestor_hof_ids)) = self.get_scope_ancestors(scope) else {
            return false;
        };
        let Some(node) = network.nodes.get(&node_id) else {
            return false;
        };
        let registry = &self.node_type_registry;
        registry
            .resolve_output_type_scoped(node, network, pin, &ancestors, &ancestor_hof_ids)
            .is_some_and(|data_type| DataType::can_be_converted_to(&data_type, target, registry))
    }
}

// ---------------------------------------------------------------------------
// Dissolve
// ---------------------------------------------------------------------------

/// The undo-history label of a dissolve (D8).
pub const DISSOLVE_NODE_DESCRIPTION: &str = "Dissolve node";

/// The pins of a node that hold dissolve candidates, in pin order, each with
/// its number of candidate wires. `Display` is the tail of the
/// [`DissolveRefusal::SeveralCandidates`] message:
/// `2 input wires could replace it (base, sub)`, where a pin holding several
/// reads `3 wires into shapes`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateList(pub Vec<(String, usize)>);

impl std::fmt::Display for CandidateList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total: usize = self.0.iter().map(|(_, count)| count).sum();
        let pins: Vec<String> = self
            .0
            .iter()
            .map(|(pin, count)| {
                if *count == 1 {
                    pin.clone()
                } else {
                    format!("{count} wires into {pin}")
                }
            })
            .collect();
        write!(
            f,
            "{total} input wires could replace it ({})",
            pins.join(", ")
        )
    }
}

/// The one way to dissolve a node: the input wire whose source takes over,
/// and every wire that read the node's outputs.
#[derive(Clone, Debug, PartialEq)]
pub struct DissolvePlan {
    /// The one input wire into `N` whose source is kept (stored in `N`'s
    /// scope).
    pub candidate: Wire,
    /// `(storage scope, wire)` for each wire to re-point. A body-return
    /// consumer is stored in the HOF's network, like any `ZoneOutput` wire.
    pub consumers: Vec<(Vec<u64>, WireAnchor)>,
}

/// Why a node cannot be dissolved. One variant per row of the design's
/// checks table, in check order; `Display` is the snackbar text, with the
/// node's name (its `custom_name`, else its type name) filled in.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DissolveRefusal {
    #[error("Can't dissolve {node}: this network is read-only (linked library)")]
    ReadOnly { node: String },
    #[error("Can't dissolve {node}: nothing uses its output — use Delete")]
    NoConsumer { node: String },
    #[error("Can't dissolve {node}: it is used as a function")]
    UsedAsFunction { node: String },
    #[error("Can't dissolve {node}: it is the network's return node")]
    ReturnNode { node: String },
    #[error("Can't dissolve {node}: no input wire fits where its output goes")]
    NoCandidate { node: String },
    #[error("Can't dissolve {node}: {candidates}")]
    SeveralCandidates {
        node: String,
        candidates: CandidateList,
    },
    #[error("Can't dissolve {node}: {consumer} would receive {source_name} twice")]
    DuplicateSource {
        node: String,
        consumer: String,
        /// The candidate's source. (Not `source`: thiserror reads a field of
        /// that name as the error's cause.)
        source_name: String,
    },
    #[error("Can't dissolve: the node no longer exists")]
    NodeNotFound,
}

/// What the dissolve messages call a node.
fn display_name(node: &Node) -> String {
    node.custom_name
        .clone()
        .unwrap_or_else(|| node.node_type_name.clone())
}

/// Every wire stored in `network` (at `scope`) or in a body below it whose
/// source is an output of node `node_id` in `node_scope`, as `(storage scope,
/// wire)`, in a stable order. The function pin's wires are included. Node
/// `node_id` itself and its body are skipped: they go with it.
fn collect_consumers(
    network: &NodeNetwork,
    scope: &mut Vec<u64>,
    node_scope: &[u64],
    node_id: u64,
    out: &mut Vec<(Vec<u64>, Wire)>,
) {
    let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
    ids.sort_unstable();
    let at_node_scope = scope.as_slice() == node_scope;
    for id in &ids {
        if at_node_scope && *id == node_id {
            continue;
        }
        let node = &network.nodes[id];
        let lists = [
            (ArgumentKind::External, &node.arguments),
            (ArgumentKind::ZoneOutput, &node.zone_output_arguments),
        ];
        for (kind, arguments) in lists {
            // A wire's depth counts up from the destination's effective
            // scope: the body, for a body-return wire.
            let dst_len = scope.len() + usize::from(kind == ArgumentKind::ZoneOutput);
            for (index, argument) in arguments.iter().enumerate() {
                for incoming in &argument.incoming_wires {
                    let depth = incoming.source_scope_depth as usize;
                    let reads_node = incoming.source_node_id == node_id
                        && matches!(incoming.source_pin, SourcePin::NodeOutput { .. })
                        && depth <= dst_len
                        && dst_len - depth == node_scope.len();
                    if reads_node {
                        out.push((
                            scope.clone(),
                            Wire {
                                source_node_id: incoming.source_node_id,
                                source_pin: incoming.source_pin,
                                source_scope_depth: incoming.source_scope_depth,
                                destination_node_id: *id,
                                destination_argument_index: index,
                                destination_argument_kind: kind,
                            },
                        ));
                    }
                }
            }
        }
    }
    for id in ids {
        if at_node_scope && id == node_id {
            continue;
        }
        if let Some(body) = network.nodes[&id].zone.as_deref() {
            scope.push(id);
            collect_consumers(body, scope, node_scope, node_id, out);
            scope.pop();
        }
    }
}

impl StructureDesigner {
    /// How node `node_id` in `scope` would be dissolved: removed, with the one
    /// input wire that fits every consumer joined to whatever its outputs fed
    /// (D13, D14). Read-only. The checks run in the order of
    /// [`DissolveRefusal`]'s variants, and the first that fails is the answer.
    ///
    /// "No consumer" counts every wire leaving the node, its function pin's
    /// included, so a node used only as a function is refused as
    /// `UsedAsFunction` rather than sent to Delete.
    pub fn plan_dissolve_node(
        &self,
        scope: &[u64],
        node_id: u64,
    ) -> Result<DissolvePlan, DissolveRefusal> {
        let network = self
            .get_scope_network(scope)
            .ok_or(DissolveRefusal::NodeNotFound)?;
        let node = network
            .nodes
            .get(&node_id)
            .ok_or(DissolveRefusal::NodeNotFound)?;
        let name = display_name(node);
        if self.ensure_active_editable().is_err() {
            return Err(DissolveRefusal::ReadOnly { node: name });
        }

        // The consumers, walked from `N`'s scope down: no shallower scope can
        // see `N`.
        let mut consumers = Vec::new();
        collect_consumers(network, &mut scope.to_vec(), scope, node_id, &mut consumers);
        if consumers.is_empty() {
            return Err(DissolveRefusal::NoConsumer { node: name });
        }
        if consumers
            .iter()
            .any(|(_, wire)| wire.source_pin == SourcePin::NodeOutput { pin_index: -1 })
        {
            return Err(DissolveRefusal::UsedAsFunction { node: name });
        }
        if network.return_node_id == Some(node_id) {
            return Err(DissolveRefusal::ReturnNode { node: name });
        }

        // Each consumer's pin type (`None` when it cannot be read, which no
        // candidate fits).
        let registry = &self.node_type_registry;
        let destination_of = |storage_scope: &[u64], wire: &Wire| {
            self.get_scope_network(storage_scope)
                .and_then(|network| network.nodes.get(&wire.destination_node_id))
        };
        let pin_types: Vec<Option<DataType>> = consumers
            .iter()
            .map(|(storage_scope, wire)| {
                let node_type =
                    registry.get_node_type_for_node(destination_of(storage_scope, wire)?)?;
                let index = wire.destination_argument_index;
                match wire.destination_argument_kind {
                    ArgumentKind::External => {
                        node_type.parameters.get(index).map(|p| p.data_type.clone())
                    }
                    ArgumentKind::ZoneOutput => node_type
                        .zone_output_pins
                        .get(index)
                        .map(|p| p.data_type.clone()),
                }
            })
            .collect();

        // The candidates: wires into `N` (each wire of an array pin on its
        // own) whose source fits every consumer.
        let node_type = registry.get_node_type_for_node(node);
        let mut candidates: Vec<Wire> = Vec::new();
        let mut per_pin: Vec<(String, usize)> = Vec::new();
        for (index, argument) in node.arguments.iter().enumerate() {
            let mut count = 0;
            for incoming in &argument.incoming_wires {
                let Some(source_type) = self.resolve_wire_source_type(
                    scope,
                    incoming.source_node_id,
                    incoming.source_pin,
                    incoming.source_scope_depth,
                ) else {
                    continue;
                };
                let fits = pin_types.iter().all(|pin_type| {
                    pin_type.as_ref().is_some_and(|pin_type| {
                        DataType::can_be_converted_to(&source_type, pin_type, registry)
                    })
                });
                if fits {
                    count += 1;
                    candidates.push(Wire {
                        source_node_id: incoming.source_node_id,
                        source_pin: incoming.source_pin,
                        source_scope_depth: incoming.source_scope_depth,
                        destination_node_id: node_id,
                        destination_argument_index: index,
                        destination_argument_kind: ArgumentKind::External,
                    });
                }
            }
            if count > 0 {
                let pin = node_type
                    .and_then(|t| t.parameters.get(index))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("input {index}"));
                per_pin.push((pin, count));
            }
        }
        let candidate = match candidates.len() {
            0 => return Err(DissolveRefusal::NoCandidate { node: name }),
            1 => candidates.remove(0),
            _ => {
                return Err(DissolveRefusal::SeveralCandidates {
                    node: name,
                    candidates: CandidateList(per_pin),
                });
            }
        };

        // Every consumer's slot must keep one wire per source node.
        for (storage_scope, wire) in &consumers {
            let destination =
                destination_of(storage_scope, wire).ok_or(DissolveRefusal::NodeNotFound)?;
            let arguments = match wire.destination_argument_kind {
                ArgumentKind::External => &destination.arguments,
                ArgumentKind::ZoneOutput => &destination.zone_output_arguments,
            };
            let accepts = arguments
                .get(wire.destination_argument_index)
                .is_some_and(|argument| {
                    slot_accepts_source(argument, node_id, candidate.source_node_id)
                });
            if !accepts {
                return Err(self.duplicate_source(scope, node_id, &candidate, storage_scope, wire));
            }
        }

        let consumers = consumers
            .iter()
            .map(|(storage_scope, wire)| {
                let storage = self
                    .get_scope_network(storage_scope)
                    .ok_or(DissolveRefusal::NodeNotFound)?;
                Ok((storage_scope.clone(), WireAnchor::from_wire(wire, storage)))
            })
            .collect::<Result<_, DissolveRefusal>>()?;
        Ok(DissolvePlan {
            candidate,
            consumers,
        })
    }

    /// Dissolve node `node_id` in `scope` as one undo step ("Dissolve node",
    /// D8): every consumer is re-pointed at the candidate's source in its own
    /// slot (D5), with its comment anchors (D11), and the node (with its body)
    /// is deleted. Nothing else moves. A refusal is the one
    /// [`Self::plan_dissolve_node`] gives now, and changes nothing.
    pub fn dissolve_node(&mut self, scope: &[u64], node_id: u64) -> Result<(), DissolveRefusal> {
        let plan = self.plan_dissolve_node(scope, node_id)?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or(DissolveRefusal::NodeNotFound)?;

        // 1. Snapshot for undo, and a copy to roll back to. Dissolving a
        //    `parameter` takes a pin off the network, and the call-site repair
        //    then drops the instances' wires into it in other networks: the
        //    undo step puts them back from `call_sites`.
        let before = self
            .snapshot_network(&network_name)
            .ok_or(DissolveRefusal::NodeNotFound)?;
        let dissolves_parameter = self
            .get_scope_network(scope)
            .and_then(|network| network.nodes.get(&node_id))
            .is_some_and(|node| node.node_type_name == "parameter");
        let call_sites = if dissolves_parameter {
            crate::network_validator::capture_call_sites(&self.node_type_registry, &network_name)
        } else {
            Vec::new()
        };
        let rollback = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .ok_or(DissolveRefusal::NodeNotFound)?
            .clone();

        // 2. Re-point each consumer. Its depth counts up to `N`'s scope, the
        //    candidate's from there to the source's frame; the sum counts the
        //    whole way.
        let candidate = plan.candidate;
        for (storage_scope, anchor) in &plan.consumers {
            let consumer = self
                .get_scope_network(storage_scope)
                .and_then(|network| anchor.resolve(network));
            let replaced = consumer.as_ref().is_some_and(|wire| {
                self.replace_wire_source(
                    storage_scope,
                    anchor,
                    candidate.source_node_id,
                    candidate.source_pin,
                    wire.source_scope_depth + candidate.source_scope_depth,
                )
                .is_ok()
            });
            if !replaced {
                // The plan checked every slot, so the two disagree.
                let refusal = match &consumer {
                    Some(wire) => {
                        self.duplicate_source(scope, node_id, &candidate, storage_scope, wire)
                    }
                    None => DissolveRefusal::NodeNotFound,
                };
                self.node_type_registry
                    .node_networks
                    .insert(network_name, rollback);
                return Err(refusal);
            }
        }

        // 3. Delete the node and its body. Every wire out of it now reads the
        //    candidate's source, and the wires into it are stored on it.
        let mut dirty: HashSet<u64> = HashSet::new();
        if scope.is_empty() {
            dirty.extend(
                plan.consumers
                    .iter()
                    .filter(|(storage_scope, _)| storage_scope.is_empty())
                    .map(|(_, anchor)| anchor.destination_node_id),
            );
            if candidate.source_scope_depth == 0 {
                dirty.insert(candidate.source_node_id);
            }
        }
        if let Some(network) = self.get_scope_network_mut(scope) {
            network.nodes.remove(&node_id);
            network.displayed_nodes.remove(&node_id);
            network.selected_node_ids.remove(&node_id);
            network
                .selected_wires
                .retain(|w| w.source_node_id != node_id && w.destination_node_id != node_id);
            if network.active_node_id == Some(node_id) {
                network.active_node_id = None;
            }
            // A comment on `N` or on the candidate wire loses that anchor.
            crate::nodes::comment::drop_dangling_anchors(network);
        }

        // 4. Display, revalidate, one step.
        for (storage_scope, anchor) in &plan.consumers {
            self.mark_node_data_changed_scoped(storage_scope, anchor.destination_node_id);
        }
        if !dirty.is_empty() {
            self.apply_node_display_policy(Some(&dirty));
        }
        self.validate_active_network();
        self.set_dirty(true);
        self.mark_full_refresh();
        if let Some(after) = self.snapshot_network(&network_name) {
            self.push_command(NetworkSnapshotCommand {
                network_name,
                description: DISSOLVE_NODE_DESCRIPTION.to_string(),
                before_snapshot: before,
                after_snapshot: after,
                call_sites,
            });
        }
        Ok(())
    }

    /// The `DuplicateSource` refusal for dissolving `node_id` (in `scope`)
    /// into `consumer` (stored in `storage_scope`): the consumer named first,
    /// then the candidate's source.
    fn duplicate_source(
        &self,
        scope: &[u64],
        node_id: u64,
        candidate: &Wire,
        storage_scope: &[u64],
        consumer: &Wire,
    ) -> DissolveRefusal {
        let name_in = |scope: &[u64], id: u64| {
            self.get_scope_network(scope)
                .and_then(|network| network.nodes.get(&id))
                .map(display_name)
                .unwrap_or_default()
        };
        let frame_len = scope
            .len()
            .saturating_sub(candidate.source_scope_depth as usize);
        DissolveRefusal::DuplicateSource {
            node: name_in(scope, node_id),
            consumer: name_in(storage_scope, consumer.destination_node_id),
            source_name: name_in(&scope[..frame_len], candidate.source_node_id),
        }
    }
}
