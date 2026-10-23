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
//! Phase 0 is the **wire drop**: a wire dragged off a pin and dropped in empty
//! space adds a node there and connects it. Planning first means a cancelled
//! dialog leaves the network untouched, and the commit is one step.

use crate::data_type::DataType;
use crate::declared_input_ops::AddInputPlan;
use crate::declared_inputs::NameSource;
use crate::node_data::DragDirection;
use crate::node_network::{Argument, ArgumentKind, SourcePin};
use crate::node_type_registry::allowed_in_zone_body;
use crate::splice_plan::{Arrangement, InputChoice, PlannedNode, SpliceSource, plan_arrangements};
use crate::structure_designer::{DragSource, StructureDesigner};
use glam::f64::DVec2;
use std::collections::HashSet;

/// The undo-history label of a wire drop that adds a node.
pub const ADD_NODE_DESCRIPTION: &str = "Add node";

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
