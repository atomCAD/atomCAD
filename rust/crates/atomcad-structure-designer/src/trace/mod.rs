//! Output-driven editing: trace a thing visible in the viewport back to the
//! nodes that contributed to it (`doc/design_output_driven_editing.md`).
//!
//! The trace is computed **on demand**; nothing is recorded during normal
//! evaluation. Each node may answer a *local* question through
//! [`NodeData::trace`](crate::node_data::NodeData::trace) — "this thing is in
//! my output: where did it come from in my inputs, and what did I do to it?" —
//! and the engine in [`engine`] owns everything non-local: wires, array pins,
//! custom-node instances, deduplication and the fallback for nodes that do not
//! answer.
//!
//! This module holds the types a node sees ([`TraceProbe`], [`TraceStep`],
//! [`TraceLink`], [`Contribution`], [`TraceContext`]) and the helpers shared by
//! the engine and the node implementations.

pub mod engine;
pub mod pick;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use atomcad_crystolecule::atomic_constants::is_allowed_passivant;
use atomcad_crystolecule::atomic_structure::{Atom, AtomicStructure};
use atomcad_crystolecule::guided_placement::crystal_bond_length;
use atomcad_geo_tree::implicit_geometry::ImplicitGeometry3D;
use atomcad_util::memory_size_estimator::MemorySizeEstimator;
use glam::f64::DVec3;

use crate::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use crate::evaluator::network_result::NetworkResult;
use crate::node_network::NodeRef;
use crate::node_type_registry::NodeTypeRegistry;

pub use engine::{
    RowContribution, TraceFrame, TraceHit, TraceHitTarget, TraceNode, TraceReport, TraceRow,
    TraceStats,
};
pub use pick::TracePick;

/// Distance within which an atom is "the same atom" when a probe is matched by
/// position (start of trace, match-by-position links, the fallback's re-sync).
pub const ATOM_MATCH_RADIUS: f64 = 0.1;

/// The tie tolerance δ for a surface ray hit on a displayed Blueprint: the
/// point lies on the tessellated surface, so δ only has to cover the gap
/// between the mesh and the SDF (see Geometry booleans in the design doc).
pub const SURFACE_HIT_DELTA: f64 = 0.1;

/// δ for a Point probe made from an atom centre, in bond lengths (see
/// [`atom_point_delta`]).
///
/// One bond length is not enough. A filled atom lies anywhere from 0 to about
/// 2.5 Å inside each face near it: a diamond (111) layer period is a/√3 =
/// 2.06 Å, and single-bond removal makes an edge recede further. Measured at
/// the outermost atom of a tetrahedron edge, the two faces' signed distances
/// differ by up to 1.98 Å with single-bond removal and 2.45 Å without, so with
/// δ = 1.545 Å an edge atom would often list only one of its two faces.
pub const ATOM_POINT_DELTA_BONDS: f64 = 2.0;

/// The bond length assumed for an atom with no bond to a non-passivating atom
/// and no tabulated crystal bond length (see [`atom_point_delta`]).
pub const FALLBACK_BOND_LENGTH: f64 = 1.5;

/// δ for an atom centre whose bonds are unknown: [`ATOM_POINT_DELTA_BONDS`] ×
/// [`FALLBACK_BOND_LENGTH`].
pub const FALLBACK_ATOM_POINT_DELTA: f64 = ATOM_POINT_DELTA_BONDS * FALLBACK_BOND_LENGTH;

/// What is being traced: one thing visible in a node's output pin.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceProbe {
    /// An atom. A `Some` id is valid in the value being traced *only*.
    ///
    /// In a link a node returns, `id: None` means **"match by position"**:
    /// the engine looks the atom up by `position` in that input, and if
    /// nothing is there it drops the link silently. The thing did not come
    /// from that input, which is pruning, not loss.
    Atom { id: Option<u32>, position: DVec3 },
    /// A point of geometry. `on_surface` is true for a ray hit on a displayed
    /// surface, false for an atom centre handed down to a Blueprint.
    ///
    /// `delta` is the tie tolerance δ the geometry booleans use to decide
    /// which children decide the shape at this point. The node that creates
    /// the `Point` sets it, and every later step passes it on unchanged.
    Point {
        position: DVec3,
        on_surface: bool,
        delta: f64,
    },
}

impl TraceProbe {
    pub fn position(&self) -> DVec3 {
        match self {
            TraceProbe::Atom { position, .. } | TraceProbe::Point { position, .. } => *position,
        }
    }
}

/// What the node did to the probed thing. Shown as the row's label.
#[derive(Debug, Clone, PartialEq)]
pub enum Contribution {
    /// The node is where the thing comes from: "filled", "added", "shape",
    /// "source", "passivated".
    Created(&'static str),
    /// The node changed it: "moved", "edited", "relaxed", "replaced".
    Modified(&'static str),
    /// The thing went through unchanged; the node only routed it.
    Passed,
}

/// A node's answer to [`NodeData::trace`](crate::node_data::NodeData::trace).
#[derive(Debug, Clone, PartialEq)]
pub struct TraceStep {
    pub contribution: Contribution,
    /// Where the thing came from. Empty = this node is the origin.
    pub upstream: Vec<TraceLink>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceLink {
    /// Input pin index.
    pub pin: usize,
    /// Index into the pin's merged array value, for array pins. The engine
    /// maps it to the wire that delivered that element.
    pub element: Option<usize>,
    /// The probe rewritten into that input's terms.
    pub probe: TraceProbe,
}

/// Key of one trace-store entry: (frame, network, node, pin).
type StoreKey = (TraceFrame, String, u64, usize);

/// The trace-lifetime value store (see `TraceContext` in the design doc).
///
/// The memo does not lend values (`lookup` / `lookup_pin` return clones, and
/// an atomic payload is a deep copy), so every value the trace fetches is kept
/// here for the rest of the trace and handed out as an `Rc`: each value is
/// copied at most twice per trace, once as its consumer's input and once as its
/// own node's output.
#[derive(Default)]
pub(crate) struct TraceStore {
    outputs: HashMap<StoreKey, Rc<NetworkResult>>,
    inputs: HashMap<StoreKey, Rc<NetworkResult>>,
    pub(crate) fills: usize,
    /// Estimated bytes held, by the memo's size estimator.
    pub(crate) bytes: usize,
}

/// Everything one trace shares: the evaluator, the registry, the one
/// evaluation context and the store. Owned by the engine; every
/// [`TraceContext`] of the trace borrows it.
pub(crate) struct TraceSession<'a> {
    pub(crate) evaluator: &'a NetworkEvaluator,
    pub(crate) registry: &'a NodeTypeRegistry,
    pub(crate) eval_context: RefCell<&'a mut NetworkEvaluationContext>,
    pub(crate) store: RefCell<TraceStore>,
}

impl<'a> TraceSession<'a> {
    /// The value on input pin `pin` of `node_id` (top of `stack`), the same
    /// value `eval` sees: `evaluate_arg`, which applies the pin's conversion
    /// and merges array pins.
    pub(crate) fn input(
        &self,
        stack: &[NetworkStackElement<'a>],
        frame: &TraceFrame,
        node_id: u64,
        pin: usize,
    ) -> Rc<NetworkResult> {
        let key = store_key(stack, frame, node_id, pin);
        if let Some(value) = self.store.borrow().inputs.get(&key) {
            return value.clone();
        }
        let value = {
            let mut context = self.eval_context.borrow_mut();
            self.evaluator
                .evaluate_arg(stack, node_id, self.registry, &mut context, pin)
        };
        let value = Rc::new(value);
        let mut store = self.store.borrow_mut();
        store.fills += 1;
        store.bytes += value.estimate_memory_bytes();
        store.inputs.insert(key, value.clone());
        value
    }

    /// `node_id`'s own evaluated output on `pin` (top of `stack`).
    pub(crate) fn output(
        &self,
        stack: &[NetworkStackElement<'a>],
        frame: &TraceFrame,
        node_id: u64,
        pin: usize,
    ) -> Rc<NetworkResult> {
        let key = store_key(stack, frame, node_id, pin);
        if let Some(value) = self.store.borrow().outputs.get(&key) {
            return value.clone();
        }
        let value = {
            let mut context = self.eval_context.borrow_mut();
            self.evaluator.evaluate(
                stack,
                node_id,
                pin as i32,
                self.registry,
                false,
                &mut context,
            )
        };
        let value = Rc::new(value);
        let mut store = self.store.borrow_mut();
        store.fills += 1;
        store.bytes += value.estimate_memory_bytes();
        store.outputs.insert(key, value.clone());
        value
    }
}

fn store_key(
    stack: &[NetworkStackElement],
    frame: &TraceFrame,
    node_id: u64,
    pin: usize,
) -> StoreKey {
    let network = stack
        .last()
        .map(|f| f.node_network.node_type.name.clone())
        .unwrap_or_default();
    (frame.clone(), network, node_id, pin)
}

/// A node's only window onto the evaluator during a trace.
///
/// The methods take `&self` and return `Rc`s, so a node can hold several
/// values at once (a boolean holds its children and its combined output; an
/// `atom_edit` holds its base while it reads its output).
pub struct TraceContext<'t, 'a> {
    pub(crate) session: &'t TraceSession<'a>,
    pub(crate) stack: &'t [NetworkStackElement<'a>],
    pub(crate) frame: &'t TraceFrame,
    pub(crate) node_id: u64,
}

impl TraceContext<'_, '_> {
    /// The evaluated value on input pin `pin`, the same value `eval` sees.
    /// Array pins return the merged `NetworkResult::Array`.
    pub fn input(&self, pin: usize) -> Rc<NetworkResult> {
        self.session
            .input(self.stack, self.frame, self.node_id, pin)
    }

    /// An owned copy of [`input`](Self::input), for a node's parameter
    /// helper shared with `eval` (which reads pins by value). Use it for the
    /// small parameter pins only: an atomic input would be a deep copy.
    pub fn input_value(&self, pin: usize) -> NetworkResult {
        self.input(pin).as_ref().clone()
    }

    /// This node's own evaluated output on `pin`.
    pub fn output(&self, pin: usize) -> Rc<NetworkResult> {
        self.session
            .output(self.stack, self.frame, self.node_id, pin)
    }

    /// The node being traced, as addressed in its own network.
    pub fn node_ref(&self) -> NodeRef {
        NodeRef::top(self.node_id)
    }
}

/// Indices of `values` within `delta` of `combined`.
pub fn deciding_children(values: &[f64], combined: f64, delta: f64) -> Vec<usize> {
    values
        .iter()
        .enumerate()
        .filter(|(_, v)| (*v - combined).abs() <= delta)
        .map(|(i, _)| i)
        .collect()
}

/// The signed distance at `point` of each element of `value`, an array of
/// geometry values (a geometry boolean's merged array pin). `None` when
/// `value` is not an array or an element carries no 3D geometry.
pub fn element_sdfs(value: &NetworkResult, point: DVec3) -> Option<Vec<f64>> {
    match value {
        NetworkResult::Array(items) => items.iter().map(|v| sdf_at(v, point)).collect(),
        _ => None,
    }
}

/// Links to elements `elements` of input pin `pin`, each carrying `probe`
/// unchanged (a geometry boolean routes the point without moving it).
pub fn element_links(pin: usize, elements: Vec<usize>, probe: &TraceProbe) -> Vec<TraceLink> {
    elements
        .into_iter()
        .map(|i| TraceLink {
            pin,
            element: Some(i),
            probe: probe.clone(),
        })
        .collect()
}

/// Signed distance of one geometry value at `point` (3D), via
/// `implicit_eval_3d`. `None` for a value that carries no 3D geometry.
pub fn sdf_at(value: &NetworkResult, point: DVec3) -> Option<f64> {
    let geo = match value {
        NetworkResult::Blueprint(b) => &b.geo_tree_root,
        NetworkResult::Crystal(c) => c.geo_tree_root.as_ref()?,
        NetworkResult::Molecule(m) => m.geo_tree_root.as_ref()?,
        _ => return None,
    };
    Some(geo.implicit_eval_3d(&point))
}

/// Is `atom` a passivating terminator: an atom of a passivant element (H or a
/// halogen) with exactly one bond?
///
/// Structural on purpose. The `is_hydrogen_passivation` flag cannot answer
/// this: lattice fill flags none of its terminators, and surface
/// reconstruction flags the dimer *hosts* instead
/// (`doc/design_halogen_passivation.md` D5).
pub fn is_terminator(atom: &Atom) -> bool {
    is_allowed_passivant(atom.atomic_number) && atom.bonds.len() == 1
}

/// δ for a Point probe made from an atom centre: [`ATOM_POINT_DELTA_BONDS`]
/// times the length of the atom's shortest bond to an atom that is not a
/// terminator ([`is_terminator`]); for an atom with no such bond,
/// `crystal_bond_length(z, z)`, else [`FALLBACK_BOND_LENGTH`].
pub fn atom_point_delta(atoms: &AtomicStructure, atom_id: u32) -> f64 {
    ATOM_POINT_DELTA_BONDS * atom_bond_length(atoms, atom_id)
}

/// The bond length [`atom_point_delta`] scales.
fn atom_bond_length(atoms: &AtomicStructure, atom_id: u32) -> f64 {
    let Some(atom) = atoms.get_atom(atom_id) else {
        return FALLBACK_BOND_LENGTH;
    };
    let shortest = atom
        .bonds
        .iter()
        .filter_map(|bond| atoms.get_atom(bond.other_atom_id()))
        .filter(|other| !is_terminator(other))
        .map(|other| other.position.distance(atom.position))
        .fold(None, |acc: Option<f64>, d| {
            Some(acc.map_or(d, |a| a.min(d)))
        });
    shortest
        .or_else(|| crystal_bond_length(atom.atomic_number, atom.atomic_number))
        .unwrap_or(FALLBACK_BOND_LENGTH)
}

/// `trace` for a node that keeps atom ids and changes only positions (the
/// movement nodes, `relax`): an atom on output pin 0 is the input atom with the
/// same id, reported `Modified(label)`. `None` for any other pin (a `diff`
/// output), a `Point` probe, or an id the input does not have.
pub fn trace_same_atom_id(
    pin: usize,
    element: Option<usize>,
    probe: &TraceProbe,
    ctx: &TraceContext,
    label: &'static str,
) -> Option<TraceStep> {
    let TraceProbe::Atom { id: Some(id), .. } = probe else {
        return None;
    };
    if pin != 0 || element.is_some() {
        return None;
    }
    let input = ctx.input(0);
    let atom = atoms_of(&input)?.get_atom(*id)?;
    Some(TraceStep {
        contribution: Contribution::Modified(label),
        upstream: vec![TraceLink {
            pin: 0,
            element: None,
            probe: TraceProbe::Atom {
                id: Some(*id),
                position: atom.position,
            },
        }],
    })
}

/// `trace` for a node that moves geometry rigidly (`geo_trans`, the movement
/// nodes): a `Point` on output pin 0 is the point `map_back` maps it to in
/// input 0, with `on_surface` and `delta` unchanged (the motion is rigid, so δ
/// holds in either frame), reported `Modified("moved")`.
///
/// `map_back` is called only for a `Point` on pin 0; it returns `None` when the
/// node's pins cannot be resolved, and the fallback takes over. `None` too for
/// an `Atom` probe, another pin (a `diff` output) or an array element.
pub fn trace_moved_point(
    pin: usize,
    element: Option<usize>,
    probe: &TraceProbe,
    map_back: impl FnOnce(DVec3) -> Option<DVec3>,
) -> Option<TraceStep> {
    let TraceProbe::Point {
        position,
        on_surface,
        delta,
    } = probe
    else {
        return None;
    };
    if pin != 0 || element.is_some() {
        return None;
    }
    Some(TraceStep {
        contribution: Contribution::Modified("moved"),
        upstream: vec![TraceLink {
            pin: 0,
            element: None,
            probe: TraceProbe::Point {
                position: map_back(*position)?,
                on_surface: *on_surface,
                delta: *delta,
            },
        }],
    })
}

/// The atomic structure a value carries, if any.
pub fn atoms_of(value: &NetworkResult) -> Option<&AtomicStructure> {
    match value {
        NetworkResult::Crystal(c) => Some(&c.atoms),
        NetworkResult::Molecule(m) => Some(&m.atoms),
        _ => None,
    }
}

/// The id of the atom nearest `position` within [`ATOM_MATCH_RADIUS`].
pub fn atom_at(atoms: &AtomicStructure, position: DVec3) -> Option<u32> {
    atoms
        .get_atoms_in_radius(&position, ATOM_MATCH_RADIUS)
        .into_iter()
        .filter_map(|id| {
            atoms
                .get_atom(id)
                .map(|a| (id, a.position.distance(position)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// `value`, or element `element` of it when `value` is an array.
pub fn element_of(value: &NetworkResult, element: Option<usize>) -> Option<&NetworkResult> {
    match (value, element) {
        (_, None) => Some(value),
        (NetworkResult::Array(items), Some(i)) => items.get(i),
        _ => None,
    }
}
