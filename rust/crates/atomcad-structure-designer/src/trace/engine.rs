//! The trace engine (`doc/design_output_driven_editing.md`, The engine).
//!
//! Walks upstream depth-first from a viewport hit, calling
//! [`NodeData::trace`](crate::node_data::NodeData::trace) node by node, and
//! owns everything non-local: wires and array elements, match-by-position
//! links, custom-node instances (entered at the return node, left at a
//! `parameter`), deduplication, the row cap, and the fallback for nodes that
//! do not answer.

use std::cell::RefCell;
use std::collections::HashSet;

use glam::f64::DVec3;

use super::{
    Contribution, SURFACE_HIT_DELTA, TraceContext, TraceProbe, TraceSession, TraceStore, atom_at,
    atom_point_delta, atoms_of, element_of, sdf_at,
};
use crate::data_type::DataType;
use crate::evaluator::eval_memo::MemoCounts;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_network::{Node, NodeRef, SourcePin};
use crate::nodes::parameter::ParameterData;
use crate::structure_designer::StructureDesigner;

/// The maximum number of rows a trace lists; past it the report is marked
/// [`TraceReport::truncated`] ("… more").
pub const ROW_CAP: usize = 200;

/// A point counts as inside a geometry when its SDF is at most this (Å).
const CONTAINS_TOLERANCE: f64 = 0.01;

/// What a row says about its node. Only the engine creates `Unknown`.
#[derive(Debug, Clone, PartialEq)]
pub enum RowContribution {
    Traced(Contribution),
    /// The node has no `trace`, or the probe was lost upstream of it:
    /// "possibly contributed".
    Unknown,
}

/// Where a node sits during the trace: the chain of custom-node instances
/// entered from the top network, outermost first. Empty = the top network.
/// Each entry is (the network that holds the instance, the instance node).
pub type TraceFrame = Vec<(String, NodeRef)>;

#[derive(Debug, Clone, PartialEq)]
pub struct TraceRow {
    pub frame: TraceFrame,
    pub node: NodeRef,
    pub network: String,
    pub contribution: RowContribution,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceNode {
    pub row: TraceRow,
    /// Rows traced inside a custom-node instance. Empty for any other node.
    pub children: Vec<TraceNode>,
}

/// What the trace cost. Read by the Phase 1 measurement and by tests; the API
/// drops it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TraceStats {
    /// Entries filled in the trace store: one per distinct input or output
    /// entry the trace touched.
    pub store_fills: usize,
    /// Estimated bytes the trace store held at the end (it only grows).
    pub store_bytes: usize,
    /// The memo's own counts for the trace (`eval_memo::take`).
    pub memo: MemoCounts,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TraceReport {
    /// The top network's rows in walk order, each with its instance subtree.
    pub rows: Vec<TraceNode>,
    /// The walk stopped at [`ROW_CAP`] rows; a "… more" row follows the last.
    pub truncated: bool,
    pub stats: TraceStats,
}

/// What the ray hit on a displayed node's output pin.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceHitTarget {
    /// An atom, at the position the display output shows it.
    Atom { position: DVec3 },
    /// A bond between atoms at `position1` and `position2`, hit at `point`.
    /// Traced as the endpoint nearer to `point`.
    Bond {
        position1: DVec3,
        position2: DVec3,
        point: DVec3,
    },
    /// A point on a displayed geometry surface.
    Surface { point: DVec3 },
}

/// Where a trace starts: a displayed node, its pin, and what was hit.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceHit {
    pub node: NodeRef,
    pub pin: usize,
    pub target: TraceHitTarget,
}

/// Trace `hit` in the active network. The body of
/// [`StructureDesigner::trace_hit`].
pub(crate) fn trace_hit(designer: &mut StructureDesigner, hit: &TraceHit) -> TraceReport {
    let Some(network_name) = designer.active_node_network_name.clone() else {
        return TraceReport::default();
    };
    if !hit.node.is_top_level() {
        return TraceReport::default();
    }
    let ((rows, truncated, store_fills, store_bytes), memo) =
        designer.with_trace_eval_context(|evaluator, registry, context| {
            let Some(root) = registry.node_networks.get(&network_name) else {
                return (Vec::new(), false, 0, 0);
            };
            let session = TraceSession {
                evaluator,
                registry,
                eval_context: RefCell::new(context),
                store: RefCell::new(TraceStore::default()),
            };
            let mut walker = Walker {
                session: &session,
                arena: Vec::new(),
                roots: Vec::new(),
                visited: HashSet::new(),
                truncated: false,
            };
            let position = Position {
                stack: vec![NetworkStackElement::root(root)],
                frame: Vec::new(),
                sinks: vec![None],
            };
            walker.start(&position, hit);
            let rows = walker.finish();
            let truncated = walker.truncated;
            let store = session.store.borrow();
            (rows, truncated, store.fills, store.bytes)
        });
    TraceReport {
        rows,
        truncated,
        stats: TraceStats {
            store_fills,
            store_bytes,
            memo,
        },
    }
}

/// Where the walk is: the evaluator's network stack, the trace frame, and for
/// each frame level the instance row that receives the rows emitted there
/// (`None` = the report's top level).
#[derive(Clone)]
struct Position<'a> {
    stack: Vec<NetworkStackElement<'a>>,
    frame: TraceFrame,
    sinks: Vec<Option<usize>>,
}

impl Position<'_> {
    fn network_name(&self) -> String {
        self.stack
            .last()
            .unwrap()
            .node_network
            .node_type
            .name
            .clone()
    }
}

/// The dedup key: (frame, network, node, pin, element, probe kind, rounded
/// position). The frame keeps two instances of one network apart; the
/// network keeps node ids of different networks apart.
type VisitKey = (TraceFrame, String, u64, usize, Option<usize>, u8, [i64; 3]);

struct ArenaNode {
    row: TraceRow,
    children: Vec<usize>,
    /// A custom-node instance row, whose contribution is derived from its
    /// children once the walk is done.
    is_instance: bool,
}

struct Walker<'s, 'a> {
    session: &'s TraceSession<'a>,
    arena: Vec<ArenaNode>,
    roots: Vec<usize>,
    visited: HashSet<VisitKey>,
    truncated: bool,
}

impl<'a> Walker<'_, 'a> {
    // ------------------------------------------------------------------
    // Start of trace
    // ------------------------------------------------------------------

    fn start(&mut self, position: &Position<'a>, hit: &TraceHit) {
        let node_id = hit.node.node_id;
        if !position
            .stack
            .last()
            .unwrap()
            .node_network
            .nodes
            .contains_key(&node_id)
        {
            return;
        }
        let output = self
            .session
            .output(&position.stack, &position.frame, node_id, hit.pin);
        let (element, probe) = match &hit.target {
            TraceHitTarget::Atom { position: p } => start_atom(&output, *p),
            TraceHitTarget::Bond {
                position1,
                position2,
                point,
            } => {
                let nearer = if position1.distance(*point) <= position2.distance(*point) {
                    *position1
                } else {
                    *position2
                };
                start_atom(&output, nearer)
            }
            TraceHitTarget::Surface { point } => {
                let element = match output.as_ref() {
                    NetworkResult::Array(items) => items
                        .iter()
                        .enumerate()
                        .filter_map(|(i, v)| sdf_at(v, *point).map(|d| (i, d.abs())))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i),
                    _ => None,
                };
                (
                    element,
                    Some(TraceProbe::Point {
                        position: *point,
                        on_surface: true,
                        delta: SURFACE_HIT_DELTA,
                    }),
                )
            }
        };
        self.visit(position, node_id, hit.pin, element, probe);
    }

    // ------------------------------------------------------------------
    // The step loop
    // ------------------------------------------------------------------

    /// Trace `probe` (`None` = lost) at `node_id`'s output `pin`.
    fn visit(
        &mut self,
        position: &Position<'a>,
        node_id: u64,
        pin: usize,
        element: Option<usize>,
        probe: Option<TraceProbe>,
    ) {
        if self.truncated {
            return;
        }
        let network = position.stack.last().unwrap().node_network;
        let Some(node) = network.nodes.get(&node_id) else {
            return;
        };
        let key = visit_key(position, node_id, pin, element, probe.as_ref());
        if !self.visited.insert(key) {
            return;
        }
        if self.arena.len() >= ROW_CAP {
            self.truncated = true;
            return;
        }

        let registry = self.session.registry;
        if !registry
            .built_in_node_types
            .contains_key(&node.node_type_name)
            && let Some(child) = registry.node_networks.get(&node.node_type_name)
            && child.valid
            && let Some(return_node_id) = child.return_node_id
        {
            // A custom-node instance: enter the subnetwork at its return node,
            // the way the evaluator does.
            let row = self.emit(position, node_id, RowContribution::Unknown, true);
            let mut inner = position.clone();
            inner.stack =
                NetworkStackElement::enter_custom_network(&position.stack, child, node_id);
            inner
                .frame
                .push((position.network_name(), NodeRef::top(node_id)));
            inner.sinks.push(Some(row));
            self.visit(&inner, return_node_id, pin, element, probe);
            return;
        }

        if node.node_type_name == "parameter"
            && let Some(param) = node.data.as_any_ref().downcast_ref::<ParameterData>()
        {
            self.visit_parameter(position, node_id, param.param_index, element, probe);
            return;
        }

        let Some(probe) = probe else {
            // Lost: list without calling `trace`, continue through every
            // phase-typed input.
            self.emit(position, node_id, RowContribution::Unknown, false);
            for input_pin in phase_input_pins(registry, node) {
                self.follow_link(position, node_id, input_pin, None, None);
            }
            return;
        };

        let step = {
            let ctx = TraceContext {
                session: self.session,
                stack: &position.stack,
                frame: &position.frame,
                node_id,
            };
            node.data.trace(pin, element, &probe, &ctx)
        };
        match step {
            Some(step) => {
                self.emit(
                    position,
                    node_id,
                    RowContribution::Traced(step.contribution),
                    false,
                );
                for link in step.upstream {
                    self.follow_link(position, node_id, link.pin, link.element, Some(link.probe));
                }
            }
            None => self.fallback(position, node, node_id, pin, element, probe),
        }
    }

    /// A `parameter`: step back out to the instance's matching input pin when
    /// the call site wires it, else continue at the parameter's own default
    /// pin (which is what `ParameterData::eval` reads in that case).
    fn visit_parameter(
        &mut self,
        position: &Position<'a>,
        node_id: u64,
        param_index: usize,
        element: Option<usize>,
        probe: Option<TraceProbe>,
    ) {
        let contribution = if probe.is_some() {
            RowContribution::Traced(Contribution::Passed)
        } else {
            RowContribution::Unknown
        };
        self.emit(position, node_id, contribution, false);
        let in_zone_body = position.stack.last().is_some_and(|f| f.is_zone_body);
        if !in_zone_body
            && let Some((parent_stack, instance_id)) =
                NetworkStackElement::parameter_call_site(&position.stack)
        {
            let instance_wired = parent_stack
                .last()
                .unwrap()
                .node_network
                .nodes
                .get(&instance_id)
                .and_then(|instance| instance.arguments.get(param_index))
                .is_some_and(|argument| !argument.is_empty());
            if instance_wired {
                let mut outer = position.clone();
                outer.stack = parent_stack;
                outer.frame.pop();
                outer.sinks.pop();
                self.follow_link(&outer, instance_id, param_index, element, probe);
                return;
            }
        }
        self.follow_link(position, node_id, 0, element, probe);
    }

    // ------------------------------------------------------------------
    // Fallback for nodes without `trace`
    // ------------------------------------------------------------------

    fn fallback(
        &mut self,
        position: &Position<'a>,
        node: &Node,
        node_id: u64,
        pin: usize,
        element: Option<usize>,
        probe: TraceProbe,
    ) {
        let registry = self.session.registry;
        let phase_pins = phase_input_pins(registry, node);
        if phase_pins.is_empty() {
            // An origin: the label comes from the type of the traced pin.
            let network = position.stack.last().unwrap().node_network;
            let output_type = registry
                .resolve_output_type(node, network, pin as i32)
                .or_else(|| {
                    registry
                        .get_node_type_for_node(node)
                        .map(|nt| nt.get_output_pin_type(pin as i32))
                })
                .unwrap_or(DataType::None);
            let label = if base_type(&output_type) == &DataType::Blueprint {
                "shape"
            } else {
                "source"
            };
            self.emit(
                position,
                node_id,
                RowContribution::Traced(Contribution::Created(label)),
                false,
            );
            return;
        }

        self.emit(position, node_id, RowContribution::Unknown, false);
        for input_pin in phase_pins {
            let input = self
                .session
                .input(&position.stack, &position.frame, node_id, input_pin);
            if matches!(input.as_ref(), NetworkResult::None) {
                continue;
            }
            let is_array_pin = registry
                .get_node_param_data_type(node, input_pin)
                .is_array();
            let candidates: Vec<(Option<usize>, &NetworkResult)> = match input.as_ref() {
                NetworkResult::Array(items) if is_array_pin => items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (Some(i), v))
                    .collect(),
                value => vec![(None, value)],
            };
            let resynced: Vec<(Option<usize>, TraceProbe)> = candidates
                .into_iter()
                .filter_map(|(input_element, value)| {
                    self.resync(position, node_id, pin, element, &probe, value)
                        .map(|p| (input_element, p))
                })
                .collect();
            if resynced.is_empty() {
                // No match in any element: the probe is lost on this input.
                self.follow_link(position, node_id, input_pin, None, None);
            } else {
                for (input_element, input_probe) in resynced {
                    self.follow_link(
                        position,
                        node_id,
                        input_pin,
                        input_element,
                        Some(input_probe),
                    );
                }
            }
        }
    }

    /// Re-sync `probe` (on `node_id`'s output `pin`) to one input `value`,
    /// converting between atoms and geometry as the input's type requires.
    /// `None` = no match.
    fn resync(
        &self,
        position: &Position<'a>,
        node_id: u64,
        pin: usize,
        element: Option<usize>,
        probe: &TraceProbe,
        value: &NetworkResult,
    ) -> Option<TraceProbe> {
        match value {
            NetworkResult::Crystal(_) | NetworkResult::Molecule(_) => {
                let atoms = atoms_of(value)?;
                if let Some(id) = atom_at(atoms, probe.position()) {
                    let atom = atoms.get_atom(id)?;
                    return Some(TraceProbe::Atom {
                        id: Some(id),
                        position: atom.position,
                    });
                }
                // The geometry-shell fallback: a Point may still be inside the
                // input's geometry.
                match probe {
                    TraceProbe::Point { .. } if point_kept(value, probe) => Some(probe.clone()),
                    _ => None,
                }
            }
            NetworkResult::Blueprint(_) => {
                let point = match probe {
                    TraceProbe::Point { .. } => probe.clone(),
                    TraceProbe::Atom { id, position: p } => {
                        let output =
                            self.session
                                .output(&position.stack, &position.frame, node_id, pin);
                        let delta = element_of(&output, element)
                            .and_then(atoms_of)
                            .and_then(|atoms| id.map(|id| atom_point_delta(atoms, id)))
                            .unwrap_or(super::FALLBACK_ATOM_POINT_DELTA);
                        TraceProbe::Point {
                            position: *p,
                            on_surface: false,
                            delta,
                        }
                    }
                };
                point_kept(value, &point).then_some(point)
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Wire and array-element resolution
    // ------------------------------------------------------------------

    /// Follow input `pin` (element `element` of it, for an array pin) of
    /// `node_id` to the wire that delivers it and continue at its source.
    /// `probe: None` = lost; an `Atom` probe with `id: None` is matched by
    /// position in the source's output and dropped when nothing is there.
    fn follow_link(
        &mut self,
        position: &Position<'a>,
        node_id: u64,
        pin: usize,
        element: Option<usize>,
        probe: Option<TraceProbe>,
    ) {
        if self.truncated {
            return;
        }
        let registry = self.session.registry;
        let network = position.stack.last().unwrap().node_network;
        let Some(node) = network.nodes.get(&node_id) else {
            return;
        };
        let Some(argument) = node.arguments.get(pin) else {
            return;
        };
        // Wires that cross a zone boundary, iteration values and function-pin
        // sources are opaque in v1.
        let mut wires: Vec<(u64, usize)> = argument
            .incoming_wires
            .iter()
            .filter(|w| w.source_scope_depth == 0)
            .filter_map(|w| match w.source_pin {
                SourcePin::NodeOutput { pin_index } if pin_index >= 0 => {
                    Some((w.source_node_id, pin_index as usize))
                }
                _ => None,
            })
            .collect();
        if wires.is_empty() {
            return;
        }
        let expected_type = registry.get_node_param_data_type(node, pin);

        if !expected_type.is_array() {
            let (source_id, source_pin) = wires[0];
            self.continue_at_source(position, source_id, source_pin, None, probe);
            return;
        }

        // Mirror `evaluate_arg`'s merge: wires sorted by source node id, each
        // value converted to an array and concatenated.
        wires.sort_by_key(|(source_id, _)| *source_id);
        if probe.is_none() {
            for (source_id, source_pin) in wires {
                self.continue_at_source(position, source_id, source_pin, None, None);
            }
            return;
        }
        let spans: Vec<(u64, usize, usize, bool)> = wires
            .iter()
            .map(|&(source_id, source_pin)| {
                let (len, whole) = self.wire_span(position, source_id, source_pin, &expected_type);
                (source_id, source_pin, len, whole)
            })
            .collect();
        let wanted: Vec<usize> = match element {
            Some(i) => vec![i],
            None => (0..spans.iter().map(|s| s.2).sum()).collect(),
        };
        for i in wanted {
            let mut offset = 0;
            for &(source_id, source_pin, len, whole) in &spans {
                if i < offset + len {
                    let source_element = whole.then_some(i - offset);
                    self.continue_at_source(
                        position,
                        source_id,
                        source_pin,
                        source_element,
                        probe.clone(),
                    );
                    break;
                }
                offset += len;
            }
        }
    }

    /// How many elements of an array pin the wire from (`source_id`,
    /// `source_pin`) delivers, and whether it delivers a whole array (so an
    /// element index carries into the source) or one value.
    fn wire_span(
        &self,
        position: &Position<'a>,
        source_id: u64,
        source_pin: usize,
        expected_type: &DataType,
    ) -> (usize, bool) {
        let network = position.stack.last().unwrap().node_network;
        let source_type = network.nodes.get(&source_id).and_then(|source| {
            self.session
                .registry
                .resolve_output_type(source, network, source_pin as i32)
        });
        let value = self
            .session
            .output(&position.stack, &position.frame, source_id, source_pin);
        match value.as_ref() {
            NetworkResult::Array(items)
                if source_type
                    .as_ref()
                    .is_none_or(|t| array_depth(t) == array_depth(expected_type)) =>
            {
                (items.len(), true)
            }
            _ => (1, false),
        }
    }

    /// Resolve a match-by-position probe against the source's output, then
    /// continue at the source.
    fn continue_at_source(
        &mut self,
        position: &Position<'a>,
        source_id: u64,
        source_pin: usize,
        source_element: Option<usize>,
        probe: Option<TraceProbe>,
    ) {
        let probe = match probe {
            Some(TraceProbe::Atom {
                id: None,
                position: p,
            }) => {
                let output =
                    self.session
                        .output(&position.stack, &position.frame, source_id, source_pin);
                let matched = element_of(&output, source_element)
                    .and_then(atoms_of)
                    .and_then(|atoms| {
                        atom_at(atoms, p)
                            .and_then(|id| atoms.get_atom(id).map(|a| (id, a.position)))
                    });
                match matched {
                    Some((id, atom_position)) => Some(TraceProbe::Atom {
                        id: Some(id),
                        position: atom_position,
                    }),
                    // The thing did not come from this input: pruning, not
                    // loss.
                    None => return,
                }
            }
            other => other,
        };
        self.visit(position, source_id, source_pin, source_element, probe);
    }

    // ------------------------------------------------------------------
    // Rows
    // ------------------------------------------------------------------

    fn emit(
        &mut self,
        position: &Position<'a>,
        node_id: u64,
        contribution: RowContribution,
        is_instance: bool,
    ) -> usize {
        let index = self.arena.len();
        self.arena.push(ArenaNode {
            row: TraceRow {
                frame: position.frame.clone(),
                node: NodeRef::top(node_id),
                network: position.network_name(),
                contribution,
            },
            children: Vec::new(),
            is_instance,
        });
        match position.sinks.last().copied().flatten() {
            Some(parent) => self.arena[parent].children.push(index),
            None => self.roots.push(index),
        }
        index
    }

    /// Build the tree, giving each instance row the strongest contribution
    /// among its children.
    fn finish(&mut self) -> Vec<TraceNode> {
        let roots = std::mem::take(&mut self.roots);
        roots.into_iter().map(|i| self.build(i)).collect()
    }

    fn build(&mut self, index: usize) -> TraceNode {
        let child_indices = std::mem::take(&mut self.arena[index].children);
        let children: Vec<TraceNode> = child_indices.into_iter().map(|i| self.build(i)).collect();
        let mut row = self.arena[index].row.clone();
        if self.arena[index].is_instance {
            // The first (nearest) of the strongest children gives the label.
            row.contribution = children
                .iter()
                .map(|c| &c.row.contribution)
                .fold(None, |best: Option<&RowContribution>, c| match best {
                    Some(b) if strength(b) >= strength(c) => Some(b),
                    _ => Some(c),
                })
                .cloned()
                .unwrap_or(RowContribution::Unknown);
        }
        TraceNode { row, children }
    }
}

/// Resolve a hit atom by position against the real output (each element of it,
/// for an array). `None` probe = not found (lost).
fn start_atom(output: &NetworkResult, p: DVec3) -> (Option<usize>, Option<TraceProbe>) {
    let found = |value: &NetworkResult| {
        atoms_of(value).and_then(|atoms| {
            atom_at(atoms, p).and_then(|id| {
                atoms.get_atom(id).map(|a| TraceProbe::Atom {
                    id: Some(id),
                    position: a.position,
                })
            })
        })
    };
    match output {
        NetworkResult::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| found(v).map(|probe| (Some(i), Some(probe))))
            .unwrap_or((None, None)),
        value => (None, found(value)),
    }
}

fn visit_key(
    position: &Position,
    node_id: u64,
    pin: usize,
    element: Option<usize>,
    probe: Option<&TraceProbe>,
) -> VisitKey {
    let (kind, p) = match probe {
        None => (0u8, DVec3::ZERO),
        Some(TraceProbe::Atom { position: p, .. }) => (1, *p),
        Some(TraceProbe::Point { position: p, .. }) => (2, *p),
    };
    let rounded = [
        (p.x * 1e3).round() as i64,
        (p.y * 1e3).round() as i64,
        (p.z * 1e3).round() as i64,
    ];
    (
        position.frame.clone(),
        position.network_name(),
        node_id,
        pin,
        element,
        kind,
        rounded,
    )
}

/// Kept if the geometry contains the point, or, for an `on_surface` probe, if
/// it is within the probe's δ of the surface.
fn point_kept(value: &NetworkResult, probe: &TraceProbe) -> bool {
    let TraceProbe::Point {
        position,
        on_surface,
        delta,
    } = probe
    else {
        return false;
    };
    let Some(sdf) = sdf_at(value, *position) else {
        return false;
    };
    sdf <= CONTAINS_TOLERANCE || (*on_surface && sdf.abs() <= *delta)
}

/// Is `data_type` a phase type (or an array of one)? A node with such an
/// input pin is not an origin.
pub fn is_phase_type(data_type: &DataType) -> bool {
    match data_type {
        DataType::Blueprint
        | DataType::Crystal
        | DataType::Molecule
        | DataType::HasAtoms
        | DataType::HasStructure
        | DataType::HasFreeLinOps => true,
        DataType::Array(inner) => is_phase_type(inner),
        _ => false,
    }
}

fn phase_input_pins(
    registry: &crate::node_type_registry::NodeTypeRegistry,
    node: &Node,
) -> Vec<usize> {
    registry
        .get_node_type_for_node(node)
        .map(|node_type| {
            node_type
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, p)| is_phase_type(&p.data_type))
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default()
}

fn base_type(data_type: &DataType) -> &DataType {
    match data_type {
        DataType::Array(inner) | DataType::Iterator(inner) => base_type(inner),
        other => other,
    }
}

fn array_depth(data_type: &DataType) -> usize {
    match data_type {
        DataType::Array(inner) => 1 + array_depth(inner),
        _ => 0,
    }
}

fn strength(contribution: &RowContribution) -> u8 {
    match contribution {
        RowContribution::Traced(Contribution::Created(_)) => 3,
        RowContribution::Traced(Contribution::Modified(_)) => 2,
        RowContribution::Traced(Contribution::Passed) => 1,
        RowContribution::Unknown => 0,
    }
}
