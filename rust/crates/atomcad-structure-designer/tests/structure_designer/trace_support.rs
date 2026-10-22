//! Shared helpers for the output-trace tests (`trace_*_test.rs`,
//! `doc/design_output_driven_editing.md`): the test-only node types, small
//! network builders, and the renderer that turns a `TraceReport` into indented
//! text so a failing assertion shows the whole trace.

#![allow(dead_code)]

use std::cell::Cell;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::structure::Structure;
use atomcad_geo_tree::GeoNode;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use atomcad_structure_designer::node_data::{EvalOutput, NodeData};
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::node_type::{
    NodeType, NodeTypeCategory, OutputPinDefinition, Parameter, no_data_loader, no_data_saver,
};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::value::add_typed_value_node;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::trace::{
    Contribution, RowContribution, TraceContext, TraceHit, TraceHitTarget, TraceNode, TraceProbe,
    TraceReport, TraceStep,
};
use glam::f64::{DVec2, DVec3};

pub const MAIN: &str = "main";

// ============================================================================
// Designer setup
// ============================================================================

/// A designer with an active network `main` and the test-only node types
/// registered.
pub fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    register_test_nodes(&mut designer.node_type_registry);
    designer.add_node_network(MAIN);
    designer.set_active_node_network_name(Some(MAIN.to_string()));
    designer
}

pub fn register_test_nodes(registry: &mut NodeTypeRegistry) {
    for node_type in [
        scripted_trace_node_type(),
        untraced_move_node_type(),
        untraced_fill_node_type(),
        counting_leaf_node_type(),
        recording_source_node_type(),
    ] {
        registry
            .built_in_node_types
            .insert(node_type.name.clone(), node_type);
    }
}

/// Replace a node's data in network `network` (re-deriving its custom type).
pub fn set_node_data(
    designer: &mut StructureDesigner,
    network: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Give a node the name the rendered trace shows.
pub fn name(designer: &mut StructureDesigner, network: &str, node_id: u64, label: &str) {
    let node = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap()
        .nodes
        .get_mut(&node_id)
        .unwrap();
    node.custom_name = Some(label.to_string());
}

/// Add a node of a built-in (or test-registered) type to the active network
/// and name it.
pub fn add(designer: &mut StructureDesigner, node_type: &str, label: &str) -> u64 {
    let id = designer.add_node(node_type, DVec2::ZERO);
    let network = designer.active_node_network_name.clone().unwrap();
    name(designer, &network, id, label);
    id
}

// ============================================================================
// Values
// ============================================================================

pub fn structure(atoms: &[(i16, DVec3)]) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    for (z, p) in atoms {
        structure.add_atom(*z, *p);
    }
    structure
}

pub fn molecule(atoms: &[(i16, DVec3)]) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure(atoms),
        geo_tree_root: None,
    })
}

pub fn crystal(atoms: &[(i16, DVec3)], geo: Option<GeoNode>) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: structure(atoms),
        geo_tree_root: geo,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

pub fn blueprint(geo: GeoNode) -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root: geo,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

/// Add a typed `value` leaf holding `value` to the active network.
pub fn add_value(designer: &mut StructureDesigner, value: NetworkResult, label: &str) -> u64 {
    let data_type = value.infer_data_type().unwrap();
    let network_name = designer.active_node_network_name.clone().unwrap();
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(&network_name)
        .unwrap();
    let id = add_typed_value_node(network, DVec2::ZERO, value, data_type);
    name(designer, &network_name, id, label);
    id
}

// ============================================================================
// Hits and rendering
// ============================================================================

pub fn atom_hit(node_id: u64, position: DVec3) -> TraceHit {
    TraceHit {
        node: NodeRef::top(node_id),
        pin: 0,
        target: TraceHitTarget::Atom { position },
    }
}

pub fn label(contribution: &RowContribution) -> String {
    match contribution {
        RowContribution::Traced(Contribution::Created(l))
        | RowContribution::Traced(Contribution::Modified(l)) => l.to_string(),
        RowContribution::Traced(Contribution::Passed) => "passed".to_string(),
        RowContribution::Unknown => "possibly contributed".to_string(),
        RowContribution::Unreached => "wired upstream".to_string(),
        RowContribution::Opaque => "trace stops here".to_string(),
    }
}

/// The trace as indented text: one line per row, `<name> <label>`, children
/// indented by two spaces, and a final `… more` line when truncated.
pub fn render(designer: &StructureDesigner, report: &TraceReport) -> String {
    let mut lines = Vec::new();
    fn walk(designer: &StructureDesigner, node: &TraceNode, depth: usize, lines: &mut Vec<String>) {
        let row = &node.row;
        let name = designer
            .node_type_registry
            .node_networks
            .get(&row.network)
            .and_then(|n| n.nodes.get(&row.node.node_id))
            .map(|n| {
                n.custom_name
                    .clone()
                    .unwrap_or_else(|| format!("{}#{}", n.node_type_name, row.node.node_id))
            })
            .unwrap_or_else(|| format!("?#{}", row.node.node_id));
        lines.push(format!(
            "{}{} {}",
            "  ".repeat(depth),
            name,
            label(&row.contribution)
        ));
        for child in &node.children {
            walk(designer, child, depth + 1, lines);
        }
    }
    for node in &report.rows {
        walk(designer, node, 0, &mut lines);
    }
    if report.truncated {
        lines.push("… more".to_string());
    }
    lines.join("\n")
}

/// Trace `hit` and render it.
pub fn trace(designer: &mut StructureDesigner, hit: &TraceHit) -> String {
    let report = designer.trace_hit(hit);
    render(designer, &report)
}

// ============================================================================
// Test-only node: ScriptedTraceNode
// ============================================================================

/// What a `ScriptedTraceNode` answers, given the trace call's arguments.
pub type Script = Arc<
    dyn Fn(usize, Option<usize>, &TraceProbe, &TraceContext) -> Option<TraceStep> + Send + Sync,
>;

/// A node whose `trace` is given by the test. Its `eval` passes the first
/// available atomic input through (pin 0, else the first element of pin 2,
/// else pin 1), so a hit on it resolves.
///
/// Pins: 0 `input: HasAtoms`, 1 `other: HasAtoms`, 2 `many: [Molecule]`,
/// 3 `shape: Blueprint`.
#[derive(Clone)]
pub struct ScriptedTraceData {
    pub script: Script,
    /// How many times `trace` was called.
    pub calls: Arc<AtomicUsize>,
}

impl ScriptedTraceData {
    pub fn new(script: Script) -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                script,
                calls: calls.clone(),
            },
            calls,
        )
    }
}

impl NodeData for ScriptedTraceData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn atomcad_structure_designer::node_network_gadget::NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let input = evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
        if !matches!(input, NetworkResult::None) {
            return EvalOutput::single(input);
        }
        if let NetworkResult::Array(items) =
            evaluator.evaluate_arg(network_stack, node_id, registry, context, 2)
            && let Some(first) = items.into_iter().next()
        {
            return EvalOutput::single(first);
        }
        EvalOutput::single(evaluator.evaluate_arg(network_stack, node_id, registry, context, 1))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }

    fn trace(
        &self,
        pin: usize,
        element: Option<usize>,
        probe: &TraceProbe,
        ctx: &TraceContext,
    ) -> Option<TraceStep> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.script)(pin, element, probe, ctx)
    }
}

fn param(name: &str, data_type: DataType) -> Parameter {
    Parameter {
        id: None,
        name: name.to_string(),
        data_type,
    }
}

fn test_node_type(
    name: &str,
    parameters: Vec<Parameter>,
    output_pins: Vec<atomcad_structure_designer::node_type::OutputPinDefinition>,
    creator: fn() -> Box<dyn NodeData>,
) -> NodeType {
    NodeType {
        name: name.to_string(),
        description: "Test-only node for the output-trace tests.".to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters,
        output_pins,
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: false,
        node_data_creator: creator,
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    }
}

fn scripted_trace_node_type() -> NodeType {
    test_node_type(
        "scripted_trace",
        vec![
            param("input", DataType::HasAtoms),
            param("other", DataType::HasAtoms),
            param("many", DataType::Array(Box::new(DataType::Molecule))),
            param("shape", DataType::Blueprint),
        ],
        vec![OutputPinDefinition::same_as_input_or_default(
            "result",
            "input",
            DataType::Molecule,
        )],
        || Box::new(ScriptedTraceData::new(Arc::new(|_, _, _, _| None)).0),
    )
}

/// Add a `ScriptedTraceNode` running `script`; returns its id and call count.
pub fn add_scripted(
    designer: &mut StructureDesigner,
    label: &str,
    script: Script,
) -> (u64, Arc<AtomicUsize>) {
    let id = add(designer, "scripted_trace", label);
    let (data, calls) = ScriptedTraceData::new(script);
    let network = designer.active_node_network_name.clone().unwrap();
    set_node_data(designer, &network, id, Box::new(data));
    (id, calls)
}

/// A script that reads input 0 and its own output, and links input 0 by the
/// probe's own id-less position (`Passed`).
pub fn pass_by_position() -> Script {
    Arc::new(|_, _, probe, ctx| {
        let _input = ctx.input(0);
        let _output = ctx.output(0);
        Some(TraceStep {
            contribution: Contribution::Passed,
            upstream: vec![atomcad_structure_designer::trace::TraceLink {
                pin: 0,
                element: None,
                probe: TraceProbe::Atom {
                    id: None,
                    position: probe.position(),
                },
            }],
        })
    })
}

// ============================================================================
// Test-only node: UntracedMoveNode
// ============================================================================

/// How far `UntracedMoveNode` moves every atom (Å, along +x).
pub const UNTRACED_MOVE: DVec3 = DVec3::new(5.0, 0.0, 0.0);

/// Translates its atomic input by [`UNTRACED_MOVE`] and keeps the default
/// `trace`: the fallback's "moved, so lost" subject.
#[derive(Clone)]
pub struct UntracedMoveData;

impl NodeData for UntracedMoveData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn atomcad_structure_designer::node_network_gadget::NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let mut value =
            evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        let atoms = match &mut value {
            NetworkResult::Molecule(m) => &mut m.atoms,
            NetworkResult::Crystal(c) => &mut c.atoms,
            _ => return EvalOutput::single(value),
        };
        atoms.transform(&glam::DQuat::IDENTITY, &UNTRACED_MOVE);
        EvalOutput::single(value)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

fn untraced_move_node_type() -> NodeType {
    test_node_type(
        "untraced_move",
        vec![param("input", DataType::HasAtoms)],
        OutputPinDefinition::single_same_as("input"),
        || Box::new(UntracedMoveData),
    )
}

// ============================================================================
// Test-only node: UntracedFillNode
// ============================================================================

thread_local! {
    /// Where the next-evaluated `UntracedFillNode` puts its atom. Thread-local
    /// so the node's data stays a plain unit struct.
    pub static FILL_POSITION: Cell<DVec3> = const { Cell::new(DVec3::ZERO) };
}

/// Takes a Blueprint and outputs a Molecule with one carbon at
/// [`FILL_POSITION`], keeping the default `trace`: the fallback's atom → point
/// conversion subject.
#[derive(Clone)]
pub struct UntracedFillData;

impl NodeData for UntracedFillData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn atomcad_structure_designer::node_network_gadget::NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let shape = evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = shape {
            return EvalOutput::single(shape);
        }
        EvalOutput::single(molecule(&[(6, FILL_POSITION.get())]))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

fn untraced_fill_node_type() -> NodeType {
    test_node_type(
        "untraced_fill",
        vec![param("shape", DataType::Blueprint)],
        OutputPinDefinition::single_fixed(DataType::Molecule),
        || Box::new(UntracedFillData),
    )
}

// ============================================================================
// Test-only node: counting leaf
// ============================================================================

thread_local! {
    /// `eval` calls of every `counting_leaf` on this thread.
    pub static LEAF_EVALS: Cell<usize> = const { Cell::new(0) };
}

/// A leaf that outputs a Molecule with one carbon at the origin and counts its
/// `eval` calls (the `CounterUnitNodeData` pattern).
#[derive(Clone)]
pub struct CountingLeafData;

impl NodeData for CountingLeafData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn atomcad_structure_designer::node_network_gadget::NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        LEAF_EVALS.set(LEAF_EVALS.get() + 1);
        EvalOutput::single(molecule(&[(6, DVec3::ZERO)]))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }
}

fn counting_leaf_node_type() -> NodeType {
    test_node_type(
        "counting_leaf",
        vec![],
        OutputPinDefinition::single_fixed(DataType::Molecule),
        || Box::new(CountingLeafData),
    )
}

// ============================================================================
// Test-only node: recording source
// ============================================================================

/// What a recording source outputs.
#[derive(Clone)]
pub enum SourceKind {
    Molecule,
    Crystal,
    /// A Blueprint of this geometry (the `elements` are ignored).
    Blueprint(Box<GeoNode>),
}

/// A leaf whose output is a Molecule (or, with `as_array`, an array of them;
/// or a Crystal or Blueprint, by `kind`) and whose `trace` records the output
/// `element` and the probe it was asked about, then answers
/// `Created("source")`.
#[derive(Clone)]
pub struct RecordingSourceData {
    pub elements: Vec<Vec<(i16, DVec3)>>,
    pub as_array: bool,
    pub kind: SourceKind,
    pub record: Arc<std::sync::Mutex<Vec<Option<usize>>>>,
    pub probes: Arc<std::sync::Mutex<Vec<TraceProbe>>>,
}

impl RecordingSourceData {
    fn value(&self, atoms: &[(i16, DVec3)]) -> NetworkResult {
        match &self.kind {
            SourceKind::Molecule => molecule(atoms),
            SourceKind::Crystal => crystal(atoms, None),
            SourceKind::Blueprint(geo) => blueprint(geo.as_ref().clone()),
        }
    }

    fn element_type(&self) -> DataType {
        match self.kind {
            SourceKind::Molecule => DataType::Molecule,
            SourceKind::Crystal => DataType::Crystal,
            SourceKind::Blueprint(_) => DataType::Blueprint,
        }
    }
}

impl NodeData for RecordingSourceData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn atomcad_structure_designer::node_network_gadget::NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();
        custom.output_pins = OutputPinDefinition::single_fixed(if self.as_array {
            DataType::Array(Box::new(self.element_type()))
        } else {
            self.element_type()
        });
        Some(custom)
    }

    fn eval<'a>(
        &self,
        _evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let values: Vec<NetworkResult> = self.elements.iter().map(|e| self.value(e)).collect();
        if self.as_array {
            EvalOutput::single(NetworkResult::Array(values))
        } else {
            EvalOutput::single(values.into_iter().next().unwrap())
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }

    fn trace(
        &self,
        _pin: usize,
        element: Option<usize>,
        probe: &TraceProbe,
        _ctx: &TraceContext,
    ) -> Option<TraceStep> {
        self.record.lock().unwrap().push(element);
        self.probes.lock().unwrap().push(probe.clone());
        Some(TraceStep {
            contribution: Contribution::Created("source"),
            upstream: vec![],
        })
    }
}

pub fn recording_source_node_type() -> NodeType {
    test_node_type(
        "recording_source",
        vec![],
        OutputPinDefinition::single_fixed(DataType::Molecule),
        || {
            Box::new(RecordingSourceData {
                elements: vec![vec![(6, DVec3::ZERO)]],
                as_array: false,
                kind: SourceKind::Molecule,
                record: Arc::new(std::sync::Mutex::new(Vec::new())),
                probes: Arc::new(std::sync::Mutex::new(Vec::new())),
            })
        },
    )
}

/// Add a recording source; returns its id and the elements it was traced at.
pub fn add_recording_source(
    designer: &mut StructureDesigner,
    label: &str,
    elements: Vec<Vec<(i16, DVec3)>>,
    as_array: bool,
) -> (u64, Arc<std::sync::Mutex<Vec<Option<usize>>>>) {
    let id = add(designer, "recording_source", label);
    let record = Arc::new(std::sync::Mutex::new(Vec::new()));
    let data = RecordingSourceData {
        elements,
        as_array,
        kind: SourceKind::Molecule,
        record: record.clone(),
        probes: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let network = designer.active_node_network_name.clone().unwrap();
    set_node_data(designer, &network, id, Box::new(data));
    (id, record)
}

/// Add a recording source of `kind` holding one value made of `atoms`;
/// returns its id and the probes it was traced with.
pub fn add_probe_source(
    designer: &mut StructureDesigner,
    label: &str,
    kind: SourceKind,
    atoms: Vec<(i16, DVec3)>,
) -> (u64, Arc<std::sync::Mutex<Vec<TraceProbe>>>) {
    let id = add(designer, "recording_source", label);
    let probes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let data = RecordingSourceData {
        elements: vec![atoms],
        as_array: false,
        kind,
        record: Arc::new(std::sync::Mutex::new(Vec::new())),
        probes: probes.clone(),
    };
    let network = designer.active_node_network_name.clone().unwrap();
    set_node_data(designer, &network, id, Box::new(data));
    (id, probes)
}

/// The data of node `node_id` in the active network, as `T`.
pub fn data_mut<T: 'static>(designer: &mut StructureDesigner, node_id: u64) -> &mut T {
    let network = designer.active_node_network_name.clone().unwrap();
    designer
        .node_type_registry
        .node_networks
        .get_mut(&network)
        .unwrap()
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .data
        .as_any_mut()
        .downcast_mut::<T>()
        .unwrap()
}

/// The atoms of node `node_id`'s output pin 0 in the active network.
pub fn output_atoms(designer: &mut StructureDesigner, node_id: u64) -> AtomicStructure {
    match designer.evaluate_node_output(&[], node_id, 0) {
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Error(e) => panic!("node {node_id} failed: {e}"),
        other => panic!(
            "node {node_id} is not atomic: {:?}",
            other.infer_data_type()
        ),
    }
}

// ============================================================================
// Zone bodies
// ============================================================================

/// Add a node of `node_type` (default data) to the body of `hof_id`.
pub fn add_to_body(designer: &mut StructureDesigner, hof_id: u64, node_type: &str) -> u64 {
    let network = designer.active_node_network_name.clone().unwrap();
    let registry = &mut designer.node_type_registry;
    let node_type_def = registry.built_in_node_types.get(node_type).unwrap();
    let data = (node_type_def.node_data_creator)();
    let arg_count = node_type_def.parameters.len();
    let body = registry
        .node_networks
        .get_mut(&network)
        .unwrap()
        .nodes
        .get_mut(&hof_id)
        .unwrap()
        .zone_mut()
        .expect("HOF node missing zone");
    body.add_node(node_type, DVec2::ZERO, arg_count, data)
}

/// Wire the HOF's zone-input pin `zone_pin` into `param` of a body node.
pub fn wire_zone_input(
    designer: &mut StructureDesigner,
    hof_id: u64,
    zone_pin: usize,
    body_node: u64,
    param: usize,
) {
    use atomcad_structure_designer::node_network::{IncomingWire, SourcePin};
    let network = designer.active_node_network_name.clone().unwrap();
    let body = designer
        .node_type_registry
        .node_networks
        .get_mut(&network)
        .unwrap()
        .nodes
        .get_mut(&hof_id)
        .unwrap()
        .zone_mut()
        .unwrap();
    body.nodes.get_mut(&body_node).unwrap().arguments[param]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: hof_id,
            source_pin: SourcePin::ZoneInput {
                pin_index: zone_pin,
            },
            source_scope_depth: 1,
        });
}

/// Wire a body node's output to the HOF's first zone-output pin.
pub fn wire_zone_output(designer: &mut StructureDesigner, hof_id: u64, body_node: u64) {
    use atomcad_structure_designer::node_network::{Argument, IncomingWire, SourcePin};
    let network = designer.active_node_network_name.clone().unwrap();
    let hof = designer
        .node_type_registry
        .node_networks
        .get_mut(&network)
        .unwrap()
        .nodes
        .get_mut(&hof_id)
        .unwrap();
    if hof.zone_output_arguments.is_empty() {
        hof.zone_output_arguments.push(Argument::new());
    }
    hof.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire {
            source_node_id: body_node,
            source_pin: SourcePin::NodeOutput { pin_index: 0 },
            source_scope_depth: 0,
        });
}

/// Configure a `parameter` node (in the active network).
pub fn configure_parameter(
    designer: &mut StructureDesigner,
    node_id: u64,
    param_name: &str,
    data_type: DataType,
) {
    use atomcad_structure_designer::nodes::parameter::ParameterData;
    let existing_param_id = designer
        .get_active_node_network()
        .and_then(|n| n.nodes.get(&node_id))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<ParameterData>())
        .and_then(|p| p.param_id);
    designer.set_node_network_data(
        node_id,
        Box::new(ParameterData {
            param_id: existing_param_id,
            param_index: 0,
            param_name: param_name.to_string(),
            data_type,
            sort_order: 0,
            data_type_str: None,
            error: None,
        }),
    );
}
