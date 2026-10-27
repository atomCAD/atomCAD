//! Moving the wires of one pin to another (`doc/design_move_wires.md`), as
//! `impl StructureDesigner`, in the shape of `splice_ops.rs`: a read-only
//! *plan* ([`StructureDesigner::plan_move_wires`]) that owns every rule (D9),
//! and a *commit* ([`StructureDesigner::move_wires`]) that re-plans and
//! performs the whole move as one undo step (D8).
//!
//! A move takes all of a pin's wires or none (D2), and each one is judged
//! like a fresh connect at the pin it lands on (D3). Held by their input end,
//! the wires keep their sources and change slot
//! ([`StructureDesigner::replace_wire_destination`]); held by their output
//! end, or dropped on an output (D16), they keep their slots and change source
//! ([`StructureDesigner::replace_wire_source`]). A move is a disconnect where
//! it leaves and a connect where it lands, and the commit runs the passes of
//! both (D17) through the Phase 1a helpers.

use crate::data_type::DataType;
use crate::node_network::{
    ArgumentKind, ConsumedPins, IncomingWire, SourcePin, Wire, WireAnchor, collect_consumers,
    record_reader_accepts_any_record,
};
use crate::splice_ops::{ReplaceWireSourceError, display_name, slot_accepts_source};
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::network_snapshot::NetworkSnapshotCommand;
use glam::f64::DVec2;
use std::collections::HashSet;

/// The undo-history label of a move of one wire (D8).
pub const MOVE_WIRE_DESCRIPTION: &str = "Move wire";

/// The undo-history label of a move of several wires (D8).
pub const MOVE_WIRES_DESCRIPTION: &str = "Move wires";

/// One pin of one node, addressed the way Flutter's `PinReference` addresses
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinAddress {
    /// Scope of the pin's owner node.
    pub scope: Vec<u64>,
    pub node_id: u64,
    pub side: PinSide,
}

/// Which pin of a node a [`PinAddress`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinSide {
    /// A wire destination: an external input, or an HOF's zone-output pin.
    Input { kind: ArgumentKind, index: usize },
    /// A wire source: a regular output (`-1` = function pin) or a zone input.
    Output(SourcePin),
}

impl PinAddress {
    /// The scope a wire's depth counts from, for an input: the owner's
    /// scope, plus the owner itself for a zone-output pin (its wires come
    /// from the body). For an output: where its value is visible, which is
    /// the owner's scope, or the owner's body for a zone input.
    fn effective_scope(&self) -> Vec<u64> {
        let mut scope = self.scope.clone();
        let inside = matches!(
            self.side,
            PinSide::Input {
                kind: ArgumentKind::ZoneOutput,
                ..
            } | PinSide::Output(SourcePin::ZoneInput { .. })
        );
        if inside {
            scope.push(self.node_id);
        }
        scope
    }

    fn is_output(&self) -> bool {
        matches!(self.side, PinSide::Output(_))
    }
}

/// One wire a pick-up holds ([`StructureDesigner::pin_wires`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldWire {
    /// Where the wire is stored, and the wire, as the splice operations
    /// address it.
    pub storage_scope: Vec<u64>,
    pub anchor: WireAnchor,
    /// The end that stays put: the source for an input-side pick-up, the
    /// consumer's input for an output-side one.
    pub fixed_end: PinAddress,
}

/// What a move would do: the wires it moves and the one it displaces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireMovePlan {
    /// The wires that will move, as `(storage scope, anchor)`, in order.
    pub wires: Vec<(Vec<u64>, WireAnchor)>,
    /// The wire a single input will displace (D4), if any.
    pub displaced: Option<(Vec<u64>, WireAnchor)>,
}

/// Why a move was refused. A refusal changes nothing and pushes no undo step.
/// The checks run in the order of the variants, and the first that fails is
/// the answer. `wire` is a label such as `cube → union.shapes`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireMoveRefusal {
    #[error("this network is read-only (linked library)")]
    ReadOnly,
    #[error("the pin no longer exists")]
    PinNotFound,
    /// `from` has no wires.
    #[error("the pin has no wires to move")]
    NoWires,
    /// Wires held by an output end, dropped on an input (D16).
    #[error("wires held by their output end can only be dropped on an output")]
    DirectionMismatch,
    /// `from == to`.
    #[error("the wires are already on this pin")]
    SamePin,
    /// More than one wire, dropped on a single input (D4) or, held by their
    /// input end, on an output (D16).
    #[error("{count} wires cannot go where only one fits")]
    TooManyWires { count: usize },
    /// A wire's source would not be in scope at the new destination, or a new
    /// source would not be in scope at a consumer (D3).
    #[error("{wire}: the source is not visible from there")]
    OutOfScope { wire: String },
    /// A wire's type does not convert to the new pin (D3).
    #[error("{wire}: {from_type} does not convert to {to_type}")]
    TypeMismatch {
        wire: String,
        from_type: String,
        to_type: String,
    },
    /// A target slot already reads the source node (D6).
    #[error("{wire}: the pin already reads that source")]
    DuplicateSource { wire: String },
}

/// One wire of a move, as it will land: the source it will read and the slot
/// it will be in. Resolved once by the plan, and used by the commit.
struct Landing {
    /// The wire as it is now, where it is stored now.
    storage_scope: Vec<u64>,
    anchor: WireAnchor,
    /// What the refusals call it.
    label: String,
    /// The source after the move: its node, pin, and the scope it is in.
    source_scope: Vec<u64>,
    source_node_id: u64,
    source_pin: SourcePin,
    /// The slot after the move, where it is stored, and the new depth.
    dest_scope: Vec<u64>,
    dest_node_id: u64,
    dest_kind: ArgumentKind,
    dest_index: usize,
    depth: u8,
    /// The source node the slot reads before the move, for a re-sourced
    /// wire; `None` for a wire that changes slot.
    replaced_source: Option<u64>,
}

impl Landing {
    /// The slot's effective scope: its storage scope, plus the HOF for a
    /// zone-output slot.
    fn dest_effective(&self) -> Vec<u64> {
        let mut scope = self.dest_scope.clone();
        if self.dest_kind == ArgumentKind::ZoneOutput {
            scope.push(self.dest_node_id);
        }
        scope
    }

    fn incoming(&self) -> IncomingWire {
        IncomingWire {
            source_node_id: self.source_node_id,
            source_pin: self.source_pin,
            source_scope_depth: self.depth,
        }
    }
}

/// What a pin is, when it exists.
struct ResolvedPin {
    /// An input's declared type, `None` for an output.
    data_type: Option<DataType>,
    /// An input that holds several wires (an array pin).
    multi: bool,
}

impl StructureDesigner {
    /// The wires a pick-up of `pin` would hold. An input's come in slot
    /// order. An output's are every wire reading it, in its scope or any body
    /// below (a capture of a node's output in its own body excepted, unless
    /// the pin is a zone input), in a stable order: scope depth first, then
    /// destination node id, then slot. Empty for an unconnected pin and for a
    /// pin that does not exist. Read-only.
    pub fn pin_wires(&self, pin: &PinAddress) -> Vec<HeldWire> {
        let Some(network) = self.get_scope_network(&pin.scope) else {
            return Vec::new();
        };
        let Some(node) = network.nodes.get(&pin.node_id) else {
            return Vec::new();
        };
        match pin.side {
            PinSide::Input { kind, index } => {
                let arguments = match kind {
                    ArgumentKind::External => &node.arguments,
                    ArgumentKind::ZoneOutput => &node.zone_output_arguments,
                };
                let Some(argument) = arguments.get(index) else {
                    return Vec::new();
                };
                let effective = pin.effective_scope();
                argument
                    .incoming_wires
                    .iter()
                    .filter(|incoming| incoming.source_scope_depth as usize <= effective.len())
                    .map(|incoming| {
                        let frame = effective.len() - incoming.source_scope_depth as usize;
                        let wire = Wire {
                            source_node_id: incoming.source_node_id,
                            source_pin: incoming.source_pin,
                            source_scope_depth: incoming.source_scope_depth,
                            destination_node_id: pin.node_id,
                            destination_argument_index: index,
                            destination_argument_kind: kind,
                        };
                        HeldWire {
                            storage_scope: pin.scope.clone(),
                            anchor: WireAnchor::from_wire(&wire, network),
                            fixed_end: PinAddress {
                                scope: effective[..frame].to_vec(),
                                node_id: incoming.source_node_id,
                                side: PinSide::Output(incoming.source_pin),
                            },
                        }
                    })
                    .collect()
            }
            PinSide::Output(source_pin) => {
                let mut consumers = Vec::new();
                collect_consumers(
                    network,
                    &mut pin.scope.clone(),
                    &pin.scope,
                    pin.node_id,
                    ConsumedPins::One(source_pin),
                    &mut consumers,
                );
                consumers.sort_by(|(a_scope, a), (b_scope, b)| {
                    (
                        a_scope.len(),
                        a_scope,
                        a.destination_node_id,
                        a.destination_argument_kind,
                        a.destination_argument_index,
                    )
                        .cmp(&(
                            b_scope.len(),
                            b_scope,
                            b.destination_node_id,
                            b.destination_argument_kind,
                            b.destination_argument_index,
                        ))
                });
                consumers
                    .into_iter()
                    .filter_map(|(storage_scope, wire)| {
                        let storage = self.get_scope_network(&storage_scope)?;
                        Some(HeldWire {
                            anchor: WireAnchor::from_wire(&wire, storage),
                            fixed_end: PinAddress {
                                scope: storage_scope.clone(),
                                node_id: wire.destination_node_id,
                                side: PinSide::Input {
                                    kind: wire.destination_argument_kind,
                                    index: wire.destination_argument_index,
                                },
                            },
                            storage_scope,
                        })
                    })
                    .collect()
            }
        }
    }

    /// Whether, and why not, the wires of `from` can move to `to`. Read-only.
    pub fn plan_move_wires(
        &self,
        from: &PinAddress,
        to: &PinAddress,
    ) -> Result<WireMovePlan, WireMoveRefusal> {
        self.plan_landings(from, to).map(|(plan, _)| plan)
    }

    /// Move the wires of `from` to `to` as one undo step ("Move wire" /
    /// "Move wires", D8). A refusal is the one [`Self::plan_move_wires`]
    /// gives now, and changes nothing.
    pub fn move_wires(
        &mut self,
        from: &PinAddress,
        to: &PinAddress,
    ) -> Result<(), WireMoveRefusal> {
        // 1. Re-plan.
        let (plan, landings) = self.plan_landings(from, to)?;
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or(WireMoveRefusal::PinNotFound)?;
        let before = self
            .snapshot_network(&network_name)
            .ok_or(WireMoveRefusal::PinNotFound)?;
        let rollback = self
            .node_type_registry
            .node_networks
            .get(&network_name)
            .ok_or(WireMoveRefusal::PinNotFound)?
            .clone();

        // Before anything changes: the disconnect half (the HOFs that expand
        // when their `f` wire leaves, with their compact footprints), the
        // connect half (each record reader's adoption, and the footprints of
        // the HOFs owning the bodies the wires land in).
        let changes_slot = !to.is_output();
        let disconnect_targets = if changes_slot {
            let removed: Vec<Wire> = self
                .get_scope_network(&from.scope)
                .map(|network| {
                    landings
                        .iter()
                        .filter_map(|landing| landing.anchor.resolve(network))
                        .collect()
                })
                .unwrap_or_default();
            self.plan_disconnect_reflow(&from.scope, &removed, &[])
        } else {
            Vec::new()
        };
        let adoptions: Vec<_> = landings
            .iter()
            .map(|landing| match landing.dest_kind {
                ArgumentKind::External => self
                    .plan_connect_landing(
                        &landing.dest_scope,
                        landing.dest_node_id,
                        landing.dest_index,
                        &landing.incoming(),
                    )
                    .and_then(|planned| planned.adoption),
                ArgumentKind::ZoneOutput => None,
            })
            .collect();
        let mut landing_bodies: Vec<(Vec<u64>, Vec<DVec2>)> = Vec::new();
        for landing in &landings {
            if !landing.dest_scope.is_empty()
                && !landing_bodies
                    .iter()
                    .any(|(scope, _)| *scope == landing.dest_scope)
            {
                let sizes = self.capture_body_owner_footprint_chain(&landing.dest_scope);
                landing_bodies.push((landing.dest_scope.clone(), sizes));
            }
        }

        // 2. Move the wires, and run the disconnect half where they leave
        //    (D17). Held by the input end, the wires come out of their slot
        //    first and the reflow runs before they land, so it sees the
        //    network a delete of them would leave; the wire a single-input
        //    target displaces goes as they land, as on a connect. Nothing can
        //    fail after a fresh plan; if something does, the network is put
        //    back.
        let moved: Result<(), (ReplaceWireSourceError, &Landing)> = if changes_slot {
            let mut detached = Vec::with_capacity(landings.len());
            for landing in &landings {
                match self.detach_wire(&landing.storage_scope, &landing.anchor) {
                    Ok(wire) => detached.push(wire),
                    Err(error) => {
                        self.node_type_registry
                            .node_networks
                            .insert(network_name, rollback);
                        return Err(refusal_for(error, landing));
                    }
                }
            }
            self.reflow_after_disconnect(&from.scope, &disconnect_targets);
            if let Some((scope, displaced)) = &plan.displaced {
                self.remove_wire(scope, displaced);
            }
            landings
                .iter()
                .zip(detached)
                .try_for_each(|(landing, wire)| {
                    self.attach_wire(
                        wire,
                        &landing.dest_scope,
                        landing.dest_node_id,
                        landing.dest_kind,
                        landing.dest_index,
                        landing.depth,
                    )
                    .map(|_| ())
                    .map_err(|error| (error, landing))
                })
        } else {
            landings.iter().try_for_each(|landing| {
                self.replace_wire_source(
                    &landing.storage_scope,
                    &landing.anchor,
                    landing.source_node_id,
                    landing.source_pin,
                    landing.depth,
                )
                .map(|_| ())
                .map_err(|error| (error, landing))
            })
        };
        if let Err((error, landing)) = moved {
            self.node_type_registry
                .node_networks
                .insert(network_name, rollback);
            return Err(refusal_for(error, landing));
        }

        // 3. The held wires leave the selection.
        let mut deselect: Vec<(Vec<u64>, WireAnchor)> = plan.wires.clone();
        deselect.extend(plan.displaced.clone());
        for (scope, anchor) in &deselect {
            if let Some(network) = self.get_scope_network_mut(scope) {
                network.selected_wires.retain(|wire| {
                    !(wire.destination_node_id == anchor.destination_node_id
                        && wire.destination_argument_kind == anchor.destination_argument_kind
                        && wire.destination_argument_index == anchor.destination_argument_index
                        && wire.source_node_id == anchor.source_node_id)
                });
            }
        }

        // 4. The rest of the passes for both ends (D17). Re-validation
        //    re-derives the `apply` / `map` layouts, repairs, and function
        //    mode; it is also what grows a landing node, so the body owners
        //    reflow after it, as a connect's do.
        for (landing, adoption) in landings.iter().zip(adoptions) {
            if let Some(data) = adoption {
                self.install_connect_adoption(&landing.dest_scope, landing.dest_node_id, data);
            }
        }
        self.validate_active_network();
        for (body, sizes) in &landing_bodies {
            if let Some((hof_id, parent)) = body.split_last() {
                self.reflow_for_footprint_change(parent, *hof_id, sizes);
            }
        }

        // Comments on the displaced wire, on a wire moved to another scope,
        // or on a wire the repair dropped, lose that anchor.
        let mut touched: Vec<Vec<u64>> = vec![from.scope.clone(), to.scope.clone()];
        let mut dirty: HashSet<u64> = HashSet::new();
        for landing in &landings {
            touched.push(landing.storage_scope.clone());
            touched.push(landing.dest_scope.clone());
            self.mark_node_data_changed_scoped(
                &landing.storage_scope,
                landing.anchor.destination_node_id,
            );
            self.mark_node_data_changed_scoped(&landing.dest_scope, landing.dest_node_id);
            for (scope, node_id) in [
                (&landing.storage_scope, landing.anchor.destination_node_id),
                (&landing.dest_scope, landing.dest_node_id),
                (&landing.source_scope, landing.source_node_id),
            ] {
                if scope.is_empty() {
                    dirty.insert(node_id);
                }
            }
        }
        for pin in [from, to] {
            if pin.scope.is_empty() {
                dirty.insert(pin.node_id);
            }
        }
        touched.sort();
        touched.dedup();
        for scope in &touched {
            if let Some(network) = self.get_scope_network_mut(scope) {
                crate::nodes::comment::drop_dangling_anchors(network);
            }
        }
        self.apply_node_display_policy(Some(&dirty));
        self.set_dirty(true);
        self.mark_full_refresh();

        // 5. One step.
        if let Some(after) = self.snapshot_network(&network_name) {
            let description = if landings.len() == 1 {
                MOVE_WIRE_DESCRIPTION
            } else {
                MOVE_WIRES_DESCRIPTION
            };
            self.push_command(NetworkSnapshotCommand {
                network_name,
                description: description.to_string(),
                before_snapshot: before,
                after_snapshot: after,
                call_sites: Vec::new(),
            });
        }
        Ok(())
    }

    /// The plan, with each wire resolved as it will land.
    fn plan_landings(
        &self,
        from: &PinAddress,
        to: &PinAddress,
    ) -> Result<(WireMovePlan, Vec<Landing>), WireMoveRefusal> {
        self.ensure_active_editable()
            .map_err(|_| WireMoveRefusal::ReadOnly)?;
        self.resolve_pin(from).ok_or(WireMoveRefusal::PinNotFound)?;
        let target = self.resolve_pin(to).ok_or(WireMoveRefusal::PinNotFound)?;
        let held = self.pin_wires(from);
        if held.is_empty() {
            return Err(WireMoveRefusal::NoWires);
        }
        if from.is_output() && !to.is_output() {
            return Err(WireMoveRefusal::DirectionMismatch);
        }
        if from == to {
            return Err(WireMoveRefusal::SamePin);
        }
        let count = held.len();
        let too_many = match to.side {
            PinSide::Input { .. } => !target.multi && count > 1,
            PinSide::Output(_) => !from.is_output() && count > 1,
        };
        if too_many {
            return Err(WireMoveRefusal::TooManyWires { count });
        }

        let landings: Vec<Landing> = held
            .iter()
            .map(|wire| self.landing(from, to, wire))
            .collect::<Option<_>>()
            .ok_or(WireMoveRefusal::PinNotFound)?;

        // The wire a single input target holds now, which the move displaces.
        let displaced = match to.side {
            PinSide::Input { .. } if !target.multi => self
                .pin_wires(to)
                .into_iter()
                .next()
                .map(|wire| (wire.storage_scope, wire.anchor)),
            _ => None,
        };

        // Each check over every wire before the next check (D3).
        for landing in &landings {
            if !self.landing_in_scope(landing) {
                return Err(WireMoveRefusal::OutOfScope {
                    wire: landing.label.clone(),
                });
            }
        }
        for landing in &landings {
            self.landing_type_fits(landing)?;
        }
        for landing in &landings {
            let slot = self
                .get_scope_network(&landing.dest_scope)
                .and_then(|network| network.nodes.get(&landing.dest_node_id))
                .and_then(|node| match landing.dest_kind {
                    ArgumentKind::External => node.arguments.get(landing.dest_index),
                    ArgumentKind::ZoneOutput => node.zone_output_arguments.get(landing.dest_index),
                });
            let accepts = match (slot, landing.replaced_source) {
                (None, _) => true,
                (Some(argument), Some(replaced)) => {
                    slot_accepts_source(argument, replaced, landing.source_node_id)
                }
                (Some(argument), None) => {
                    !argument.has_source(landing.source_node_id)
                        || displaced.as_ref().is_some_and(|(_, anchor)| {
                            anchor.source_node_id == landing.source_node_id
                        })
                }
            };
            if !accepts {
                return Err(WireMoveRefusal::DuplicateSource {
                    wire: landing.label.clone(),
                });
            }
        }

        let plan = WireMovePlan {
            wires: landings
                .iter()
                .map(|landing| (landing.storage_scope.clone(), landing.anchor.clone()))
                .collect(),
            displaced,
        };
        Ok((plan, landings))
    }

    /// Whether `pin` exists, and what it is.
    fn resolve_pin(&self, pin: &PinAddress) -> Option<ResolvedPin> {
        let node = self
            .get_scope_network(&pin.scope)?
            .nodes
            .get(&pin.node_id)?;
        let node_type = self.node_type_registry.get_node_type_for_node(node)?;
        match pin.side {
            PinSide::Input {
                kind: ArgumentKind::External,
                index,
            } => {
                let data_type = node_type.parameters.get(index)?.data_type.clone();
                node.arguments.get(index)?;
                Some(ResolvedPin {
                    multi: data_type.is_array(),
                    data_type: Some(data_type),
                })
            }
            PinSide::Input {
                kind: ArgumentKind::ZoneOutput,
                index,
            } => {
                let data_type = node_type.zone_output_pins.get(index)?.data_type.clone();
                Some(ResolvedPin {
                    multi: data_type.is_array(),
                    data_type: Some(data_type),
                })
            }
            PinSide::Output(SourcePin::NodeOutput { pin_index }) => (pin_index >= -1
                && pin_index < node_type.output_pin_count() as i32)
                .then_some(ResolvedPin {
                    data_type: None,
                    multi: false,
                }),
            PinSide::Output(SourcePin::ZoneInput { pin_index }) => {
                (pin_index < node_type.zone_input_pins.len()).then_some(ResolvedPin {
                    data_type: None,
                    multi: false,
                })
            }
        }
    }

    /// Where `wire`, held at `from`, lands when dropped on `to`.
    fn landing(&self, from: &PinAddress, to: &PinAddress, wire: &HeldWire) -> Option<Landing> {
        let label = self.wire_label(&wire.storage_scope, &wire.anchor)?;
        match (from.side, to.side) {
            // Held by the input end, onto an input: the source stays, the
            // slot changes.
            (PinSide::Input { .. }, PinSide::Input { kind, index }) => {
                let PinSide::Output(source_pin) = wire.fixed_end.side else {
                    return None;
                };
                let landing = Landing {
                    storage_scope: wire.storage_scope.clone(),
                    anchor: wire.anchor.clone(),
                    label,
                    source_scope: wire.fixed_end.scope.clone(),
                    source_node_id: wire.fixed_end.node_id,
                    source_pin,
                    dest_scope: to.scope.clone(),
                    dest_node_id: to.node_id,
                    dest_kind: kind,
                    dest_index: index,
                    depth: 0,
                    replaced_source: None,
                };
                Some(with_depth(landing))
            }
            // Onto an output: the slot stays, the source changes. Held by the
            // output end that is every consumer; by the input end, the one
            // wire (D16).
            (_, PinSide::Output(source_pin)) => {
                let (dest_scope, dest_node_id, dest_kind, dest_index) = match from.side {
                    PinSide::Input { kind, index } => {
                        (from.scope.clone(), from.node_id, kind, index)
                    }
                    PinSide::Output(_) => {
                        let PinSide::Input { kind, index } = wire.fixed_end.side else {
                            return None;
                        };
                        (
                            wire.fixed_end.scope.clone(),
                            wire.fixed_end.node_id,
                            kind,
                            index,
                        )
                    }
                };
                let landing = Landing {
                    storage_scope: wire.storage_scope.clone(),
                    anchor: wire.anchor.clone(),
                    label,
                    source_scope: to.scope.clone(),
                    source_node_id: to.node_id,
                    source_pin,
                    dest_scope,
                    dest_node_id,
                    dest_kind,
                    dest_index,
                    depth: 0,
                    replaced_source: Some(wire.anchor.source_node_id),
                };
                Some(with_depth(landing))
            }
            (PinSide::Output(_), PinSide::Input { .. }) => None,
        }
    }

    /// The scope rule of a fresh connect (D3, `doc/design_zones_ui.md`
    /// §"Computing source_scope_depth at wire creation"): the source's scope
    /// is a prefix of the slot's effective scope; a zone input is visible only
    /// inside its own HOF's body; and a body-return wire reads a node output
    /// of the body itself.
    fn landing_in_scope(&self, landing: &Landing) -> bool {
        let effective = landing.dest_effective();
        if !effective.starts_with(&landing.source_scope) {
            return false;
        }
        if let SourcePin::ZoneInput { .. } = landing.source_pin {
            let mut body = landing.source_scope.clone();
            body.push(landing.source_node_id);
            if !effective.starts_with(&body) {
                return false;
            }
        }
        if landing.dest_kind == ArgumentKind::ZoneOutput
            && (landing.depth != 0 || !matches!(landing.source_pin, SourcePin::NodeOutput { .. }))
        {
            return false;
        }
        true
    }

    /// The type rule of a fresh connect (D3): the source's type, resolved
    /// from the slot's effective scope, converts to the slot's declared type,
    /// or the slot is a record reader's `record` pin that takes any record.
    fn landing_type_fits(&self, landing: &Landing) -> Result<(), WireMoveRefusal> {
        let effective = landing.dest_effective();
        let pin = PinAddress {
            scope: landing.dest_scope.clone(),
            node_id: landing.dest_node_id,
            side: PinSide::Input {
                kind: landing.dest_kind,
                index: landing.dest_index,
            },
        };
        let target = self
            .resolve_pin(&pin)
            .and_then(|pin| pin.data_type)
            .ok_or(WireMoveRefusal::PinNotFound)?;
        let source_type = self.resolve_wire_source_type(
            &effective,
            landing.source_node_id,
            landing.source_pin,
            landing.depth,
        );
        let registry = &self.node_type_registry;
        let fits = source_type.as_ref().is_some_and(|source_type| {
            let reader = landing.dest_kind == ArgumentKind::External
                && self
                    .get_scope_network(&landing.dest_scope)
                    .is_some_and(|network| {
                        record_reader_accepts_any_record(
                            network,
                            self.scope_owner(&landing.dest_scope),
                            landing.dest_node_id,
                            landing.dest_index,
                            source_type,
                            registry,
                        )
                    });
            reader || DataType::can_be_converted_to(source_type, &target, registry)
        });
        if fits {
            return Ok(());
        }
        Err(WireMoveRefusal::TypeMismatch {
            wire: landing.label.clone(),
            from_type: source_type
                .map(|t| t.to_string())
                .unwrap_or_else(|| "an unresolved type".to_string()),
            to_type: target.to_string(),
        })
    }

    /// `source → destination.pin`, for the refusals.
    fn wire_label(&self, storage_scope: &[u64], anchor: &WireAnchor) -> Option<String> {
        let network = self.get_scope_network(storage_scope)?;
        let wire = anchor.resolve(network)?;
        let destination = network.nodes.get(&wire.destination_node_id)?;
        let node_type = self.node_type_registry.get_node_type_for_node(destination);
        let index = wire.destination_argument_index;
        let pin_name = node_type
            .and_then(|t| match wire.destination_argument_kind {
                ArgumentKind::External => t.parameters.get(index),
                ArgumentKind::ZoneOutput => t.zone_output_pins.get(index),
            })
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("input {index}"));
        let mut effective = storage_scope.to_vec();
        if wire.destination_argument_kind == ArgumentKind::ZoneOutput {
            effective.push(wire.destination_node_id);
        }
        let frame = effective
            .len()
            .saturating_sub(wire.source_scope_depth as usize);
        let source_name = self
            .get_scope_network(&effective[..frame])
            .and_then(|network| network.nodes.get(&wire.source_node_id))
            .map(display_name)
            .unwrap_or_else(|| wire.source_node_id.to_string());
        Some(format!(
            "{source_name} → {}.{pin_name}",
            display_name(destination)
        ))
    }

    /// Take `anchor`'s wire (stored in `scope`) out of its slot.
    fn remove_wire(&mut self, scope: &[u64], anchor: &WireAnchor) {
        let Some(network) = self.get_scope_network_mut(scope) else {
            return;
        };
        let Some(wire) = anchor.resolve(network) else {
            return;
        };
        let Some(node) = network.nodes.get_mut(&wire.destination_node_id) else {
            return;
        };
        let arguments = match wire.destination_argument_kind {
            ArgumentKind::External => &mut node.arguments,
            ArgumentKind::ZoneOutput => &mut node.zone_output_arguments,
        };
        if let Some(argument) = arguments.get_mut(wire.destination_argument_index) {
            argument.remove_source(wire.source_node_id);
        }
    }
}

/// The refusal for a primitive that failed after a fresh plan accepted: the
/// plan and the live network disagree.
fn refusal_for(error: ReplaceWireSourceError, landing: &Landing) -> WireMoveRefusal {
    match error {
        ReplaceWireSourceError::WireNotFound => WireMoveRefusal::PinNotFound,
        ReplaceWireSourceError::SourceAlreadyInSlot => WireMoveRefusal::DuplicateSource {
            wire: landing.label.clone(),
        },
    }
}

/// `landing` with its depth: the slot's effective scope's length minus the
/// source scope's. A source outside the slot's scope gets `0`, and the scope
/// check refuses it.
fn with_depth(mut landing: Landing) -> Landing {
    let effective = landing.dest_effective();
    landing.depth = effective
        .len()
        .checked_sub(landing.source_scope.len())
        .and_then(|depth| u8::try_from(depth).ok())
        .unwrap_or(0);
    landing
}
